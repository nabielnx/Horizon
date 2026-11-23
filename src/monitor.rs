use crate::{Action, Canvas, INK, MUTED, Settings, Window, i18n, motion, render};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, FreeLibrary, HANDLE, HMODULE},
        System::{
            LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
            ProcessStatus::{
                K32EnumProcesses, K32GetPerformanceInfo, K32GetProcessMemoryInfo,
                PERFORMANCE_INFORMATION, PROCESS_MEMORY_COUNTERS,
            },
            Threading::{
                GetSystemTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    },
    core::{PCWSTR, PWSTR, s, w},
};

pub(super) const PAGE: usize = 7;
pub(super) const COMPACT_CLIENT_WIDTH: i32 = 550;
pub(super) const CPU_COLOR: u32 = 0x75e6ba;
const COLORS: [u32; 3] = [CPU_COLOR, 0x9da9d8, 0xb4a0d8];
const METRICS: [&str; 3] = ["CPU", "RAM", "GPU"];
const HISTORY: usize = 30;
const MIB: u64 = 1024 * 1024;
type Bounds = (f32, f32, f32, f32);
type Usage = [Option<f32>; 3];

pub(super) struct Badges {
    pub cpu: Bounds,
    pub memory: Bounds,
}

impl Badges {
    pub fn new(outline: render::ShellOutline) -> Self {
        let h = 28.;
        let y = (outline.height - h) / 2.;
        Self {
            cpu: (outline.offset - 6. - h, y, h, h),
            memory: (outline.offset + outline.width + 6., y, 108., h),
        }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        [self.cpu, self.memory]
            .into_iter()
            .any(|(left, top, w, h)| {
                let (x, y) = (x as f32, y as f32);
                x >= left.floor() - 1.
                    && x <= (left + w).ceil()
                    && y >= top.floor() - 1.
                    && y <= (top + h).ceil()
            })
    }
}

#[derive(Default)]
pub(super) struct Monitor {
    worker: Option<Worker>,
    history: VecDeque<Usage>,
    processes: Vec<(String, u64)>,
    pub(super) notice: bool,
}

struct Worker {
    data: Receiver<Reading>,
    _stop: mpsc::Sender<()>,
    process_details: Arc<AtomicBool>,
}

struct Reading {
    usage: Usage,
    processes: Option<Vec<(String, u64)>>,
}

impl Monitor {
    pub fn poll(&mut self, page_visible: bool, compact_enabled: bool) -> bool {
        if !page_visible && !compact_enabled {
            self.worker = None;
            self.history.clear();
            self.processes.clear();
            return false;
        }
        if self.worker.is_none() {
            self.worker = Worker::start(page_visible);
        }
        let Some(worker) = &self.worker else {
            return false;
        };
        worker.process_details.store(page_visible, Relaxed);
        let Ok(reading) = worker.data.try_recv() else {
            return false;
        };
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(reading.usage);
        if let Some(processes) = reading.processes {
            self.processes = processes;
        }
        true
    }

    fn usage(&self) -> Usage {
        self.history.back().copied().unwrap_or_default()
    }

    pub unsafe fn draw_badges(&self, c: &Canvas, badges: Badges) {
        let [cpu, ram, gpu] = self.usage();
        let (x, y, w, h) = badges.cpu;
        c.rect(x, y, w, h, h / 2., 0x161a1a);
        let center = (x + w / 2., y + h / 2.);
        draw_ring(c, cpu, COLORS[0], |t| {
            let angle = std::f32::consts::TAU * t;
            (center.0 + 12.5 * angle.sin(), center.1 - 12.5 * angle.cos())
        });
        c.design_text(
            &percent(cpu),
            badges.cpu,
            (8.5, 700, render::DESIGN_CENTERED),
            COLORS[0],
        );

        let (x, y, w, h) = badges.memory;
        c.rect(x, y, w, h, h / 2., 0x171821);
        for (index, usage) in [ram, gpu].into_iter().enumerate() {
            let direction = if index == 0 { -1. } else { 1. };
            let color = COLORS[index + 1];
            draw_ring(c, usage, color, |t| half_pill(badges.memory, direction, t));
            c.design_text(
                &format!("{} {}", METRICS[index + 1], percent(usage)),
                (x + 2. + index as f32 * (w / 2. - 1.), y, w / 2. - 3., h),
                (8.5, 600, render::DESIGN_CENTERED),
                color,
            );
        }
        c.line((x + w / 2., y + 8.), (x + w / 2., y + h - 8.), 1., 0x383945);
    }
}

impl Worker {
    fn start(page_visible: bool) -> Option<Self> {
        let (sender, data) = mpsc::sync_channel(1);
        let (stop, cancelled) = mpsc::channel::<()>();
        let process_details = Arc::new(AtomicBool::new(page_visible));
        let details = process_details.clone();
        std::thread::Builder::new()
            .name("horizon-monitor".into())
            .spawn(move || {
                // Publish RAM immediately. PDH initialization and process enumeration
                // can take longer, so they stay off the UI thread.
                let mut cpu = cpu_times();
                let _ = sender.try_send(Reading {
                    usage: [None, ram_usage(), None],
                    processes: None,
                });
                let mut gpu = GpuQuery::open();
                let mut process_wait = 0;
                while let Err(mpsc::RecvTimeoutError::Timeout) =
                    cancelled.recv_timeout(Duration::from_secs(1))
                {
                    let next_cpu = cpu_times();
                    let mut processes = None;
                    if details.load(Relaxed) {
                        if process_wait == 0 {
                            processes = Some(top_processes());
                            process_wait = 3;
                        }
                        process_wait -= 1;
                    } else {
                        process_wait = 0;
                    }
                    let reading = Reading {
                        usage: [
                            cpu_usage(cpu, next_cpu),
                            ram_usage(),
                            gpu.as_mut().and_then(GpuQuery::sample),
                        ],
                        processes,
                    };
                    cpu = next_cpu;
                    if let Err(mpsc::TrySendError::Disconnected(_)) = sender.try_send(reading) {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            data,
            _stop: stop,
            process_details,
        })
    }
}

fn cpu_times() -> Option<(u64, u64)> {
    unsafe {
        let [mut idle, mut kernel, mut user] = [FILETIME::default(); 3];
        GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()?;
        let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        Some((ticks(idle), ticks(kernel) + ticks(user)))
    }
}

fn cpu_usage(previous: Option<(u64, u64)>, next: Option<(u64, u64)>) -> Option<f32> {
    let (idle, total) = previous?;
    let (next_idle, next_total) = next?;
    let delta = next_total.checked_sub(total)?;
    let idle_delta = next_idle.checked_sub(idle)?;
    (delta > 0).then(|| 100. * delta.saturating_sub(idle_delta) as f32 / delta as f32)
}

fn ram_usage() -> Option<f32> {
    let mut status = PERFORMANCE_INFORMATION::default();
    let size = std::mem::size_of_val(&status) as u32;
    status.cb = size;
    if !unsafe { K32GetPerformanceInfo(&mut status, size) }.as_bool() {
        return None;
    }
    (status.PhysicalTotal > 0).then(|| {
        100. * status
            .PhysicalTotal
            .saturating_sub(status.PhysicalAvailable) as f32
            / status.PhysicalTotal as f32
    })
}

const PDH_FMT_DOUBLE: u32 = 0x200;
const PDH_MORE_DATA: u32 = 0x800007d2;

// With PDH_FMT_DOUBLE the Win32 value union contains an f64.
#[repr(C)]
struct PdhValue {
    status: u32,
    value: f64,
}

#[repr(C)]
struct PdhItem {
    name: PWSTR,
    formatted: PdhValue,
}

struct PdhLibrary(HMODULE);

impl Drop for PdhLibrary {
    fn drop(&mut self) {
        let _ = unsafe { FreeLibrary(self.0) };
    }
}

struct PdhApi {
    _library: PdhLibrary,
    open: unsafe extern "system" fn(PCWSTR, usize, *mut HANDLE) -> u32,
    add: unsafe extern "system" fn(HANDLE, PCWSTR, usize, *mut HANDLE) -> u32,
    collect: unsafe extern "system" fn(HANDLE) -> u32,
    array: unsafe extern "system" fn(HANDLE, u32, *mut u32, *mut u32, *mut PdhItem) -> u32,
    close: unsafe extern "system" fn(HANDLE) -> u32,
}

impl PdhApi {
    unsafe fn load() -> Option<Self> {
        // Keep PDH out of startup and load only the copy that ships with Windows.
        let library =
            PdhLibrary(LoadLibraryExW(w!("pdh.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?);
        let handle = library.0;
        Some(Self {
            _library: library,
            open: std::mem::transmute(GetProcAddress(handle, s!("PdhOpenQueryW"))?),
            add: std::mem::transmute(GetProcAddress(handle, s!("PdhAddEnglishCounterW"))?),
            collect: std::mem::transmute(GetProcAddress(handle, s!("PdhCollectQueryData"))?),
            array: std::mem::transmute(GetProcAddress(handle, s!("PdhGetFormattedCounterArrayW"))?),
            close: std::mem::transmute(GetProcAddress(handle, s!("PdhCloseQuery"))?),
        })
    }
}

struct GpuQuery {
    api: PdhApi,
    query: HANDLE,
    counter: HANDLE,
    // PDH writes aligned structures followed by their UTF-16 names.
    buffer: Vec<u64>,
    engines: HashMap<String, f64>,
}

impl GpuQuery {
    fn open() -> Option<Self> {
        unsafe {
            let mut gpu = Self {
                api: PdhApi::load()?,
                query: HANDLE::default(),
                counter: HANDLE::default(),
                buffer: Vec::new(),
                engines: HashMap::new(),
            };
            if (gpu.api.open)(PCWSTR::null(), 0, &mut gpu.query) != 0
                || (gpu.api.add)(
                    gpu.query,
                    w!(r"\GPU Engine(*)\Utilization Percentage"),
                    0,
                    &mut gpu.counter,
                ) != 0
            {
                return None;
            }
            (gpu.api.collect)(gpu.query);
            Some(gpu)
        }
    }

    fn sample(&mut self) -> Option<f32> {
        unsafe {
            if (self.api.collect)(self.query) != 0 {
                return None;
            }
            let mut bytes = (self.buffer.len() * std::mem::size_of::<u64>()) as u32;
            let mut count = 0;
            let mut items = self.buffer.as_mut_ptr().cast::<PdhItem>();
            let mut status = (self.api.array)(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut bytes,
                &mut count,
                if self.buffer.is_empty() {
                    std::ptr::null_mut()
                } else {
                    items
                },
            );
            if status == PDH_MORE_DATA {
                // Probe again with a null buffer: PDH only guarantees the required
                // size for a zero-sized input, including when instances change.
                bytes = 0;
                if (self.api.array)(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut bytes,
                    &mut count,
                    std::ptr::null_mut(),
                ) != PDH_MORE_DATA
                    || bytes == 0
                    || bytes > 4 * 1024 * 1024
                {
                    return None;
                }
                self.buffer
                    .resize((bytes as usize).div_ceil(std::mem::size_of::<u64>()), 0);
                items = self.buffer.as_mut_ptr().cast();
                status =
                    (self.api.array)(self.counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, items);
            }
            if status != 0
                || count == 0
                || count as usize
                    > self.buffer.len() * std::mem::size_of::<u64>()
                        / std::mem::size_of::<PdhItem>()
            {
                return None;
            }
            self.engines.clear();
            for item in std::slice::from_raw_parts(items, count as usize) {
                if item.formatted.status > 1 || item.name.is_null() {
                    continue;
                }
                let value = item.formatted.value;
                if value.is_finite()
                    && let Ok(name) = item.name.to_string()
                    && let Some((_, engine)) = name.split_once("_luid_")
                {
                    *self
                        .engines
                        .entry(engine.split('#').next().unwrap_or(engine).to_owned())
                        .or_insert(0.) += value.max(0.);
                }
            }
            // Combine process contributions per physical engine and show the busiest one.
            self.engines
                .values()
                .copied()
                .reduce(f64::max)
                .map(|v| v.min(100.) as f32)
        }
    }
}

impl Drop for GpuQuery {
    fn drop(&mut self) {
        if !self.query.is_invalid() {
            unsafe { (self.api.close)(self.query) };
        }
    }
}

fn top_processes() -> Vec<(String, u64)> {
    unsafe {
        let mut pids = vec![0_u32; 256];
        let mut bytes = 0;
        loop {
            let capacity = std::mem::size_of_val(pids.as_slice()) as u32;
            if !K32EnumProcesses(pids.as_mut_ptr(), capacity, &mut bytes).as_bool() {
                return Vec::new();
            }
            if bytes < capacity || pids.len() >= 4096 {
                break;
            }
            pids.resize(pids.len() * 2, 0);
        }
        pids.truncate(bytes as usize / std::mem::size_of::<u32>());
        let mut working_sets = Vec::new();
        for pid in pids {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                continue;
            };
            let mut counters = PROCESS_MEMORY_COUNTERS::default();
            let size = std::mem::size_of_val(&counters) as u32;
            counters.cb = size;
            if K32GetProcessMemoryInfo(handle, &mut counters, size).as_bool() {
                working_sets.push((pid, counters.WorkingSetSize as u64));
            }
            let _ = CloseHandle(handle);
        }
        working_sets.sort_unstable_by_key(|&(_, size)| std::cmp::Reverse(size));
        working_sets
            .into_iter()
            .filter_map(|(pid, size)| {
                let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
                let mut path = [0_u16; 1024];
                let mut len = path.len() as u32;
                let result = QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_WIN32,
                    PWSTR(path.as_mut_ptr()),
                    &mut len,
                );
                let _ = CloseHandle(handle);
                result.ok()?;
                let path = String::from_utf16_lossy(&path[..len as usize]);
                Some((path.rsplit('\\').next()?.to_owned(), size))
            })
            .take(3)
            .collect()
    }
}

pub(super) fn open_task_manager() -> std::io::Result<()> {
    let root = std::env::var_os("SystemRoot")
        .ok_or_else(|| std::io::Error::other("Windows system directory unavailable"))?;
    let path = std::path::PathBuf::from(root).join(r"System32\Taskmgr.exe");
    std::process::Command::new(path).spawn().map(drop)
}

fn percent(usage: Option<f32>) -> String {
    usage.map_or_else(|| "—".into(), |value| format!("{value:.0}%"))
}

fn half_pill((x, y, w, h): Bounds, direction: f32, t: f32) -> (f32, f32) {
    let radius = h / 2. - 1.5;
    let flat = w / 2. - h / 2.;
    let half_arc = std::f32::consts::PI * radius;
    let distance = t * (flat * 2. + half_arc);
    let (dx, dy) = if distance <= flat {
        (distance, -radius)
    } else if distance < flat + half_arc {
        let angle = (distance - flat) / radius;
        (flat + radius * angle.sin(), -radius * angle.cos())
    } else {
        (flat * 2. + half_arc - distance, radius)
    };
    (x + w / 2. + direction * dx, y + h / 2. + dy)
}

unsafe fn draw_ring(c: &Canvas, usage: Option<f32>, color: u32, at: impl Fn(f32) -> (f32, f32)) {
    let points: [(f32, f32); 49] = std::array::from_fn(|i| at(i as f32 / 48.));
    let _ = c.polyline(&points, 1.1, motion::mix(0x292a32, color, 0.26));
    if let Some(usage) = usage.filter(|value| *value > 0.) {
        let fraction = (usage / 100.).clamp(0., 1.);
        let segments = (48. * fraction).ceil() as usize;
        let mut progress = [(0., 0.); 49];
        for (i, point) in progress[..=segments].iter_mut().enumerate() {
            *point = at(i as f32 * fraction / segments as f32);
        }
        let _ = c.polyline(&progress[..=segments], 1.65, color);
    }
}

impl Window {
    pub(super) unsafe fn draw_monitor(&mut self, c: &Canvas, settings: &Settings) {
        let x = 70.;
        let w = settings.width as f32 - x - 18.;
        let title = (x, 23., w - 132., 34.);
        c.design_text("Monitor", title, (20., 700, 0), INK);
        let toggle = (x + w - 124., 24., 124., 30.);
        let enabled = settings.monitor_in_compact;
        c.glass_panel(
            toggle,
            10.,
            if enabled { 0x214639 } else { 0x252527 },
            c.hover_amount(toggle, self.figma_hover(toggle)),
        );
        c.design_text(
            i18n::tr("Tampil saat idle"),
            (toggle.0 + 9., toggle.1 + 8., 86., 15.),
            (10., 600, 0),
            INK,
        );
        let switch_x = toggle.0 + 94.;
        c.rect(
            switch_x,
            33.,
            21.,
            12.,
            6.,
            if enabled { CPU_COLOR } else { 0x55585d },
        );
        let knob = c.animate_value(toggle, 2, f32::from(enabled), motion::STATE);
        c.ellipse(
            switch_x + 2. + 9. * knob,
            35.,
            8.,
            8.,
            if enabled { 0x193229 } else { INK },
        );
        self.figma_hit(toggle, Action::ToggleMonitorCompact);

        let card_w = (w - 16.) / 3.;
        for (index, usage) in self.monitor.usage().into_iter().enumerate() {
            let x = x + index as f32 * (card_w + 8.);
            let color = COLORS[index];
            c.glass_panel((x, 63., card_w, 85.), 12., 0x1d2022, 0.);
            c.design_text(
                METRICS[index],
                (x + 10., 72., card_w - 20., 14.),
                (10., 700, 0),
                color,
            );
            c.design_text(
                &percent(usage),
                (x + 10., 88., card_w - 20., 28.),
                (21., 700, 0),
                INK,
            );
            let graph_w = card_w - 20.;
            c.line((x + 10., 137.), (x + 10. + graph_w, 137.), 1., 0x383c40);
            let mut previous = None;
            let start = HISTORY - self.monitor.history.len();
            for (sample, point) in self.monitor.history.iter().enumerate() {
                let next = point[index].map(|v| {
                    (
                        x + 10. + (start + sample) as f32 * graph_w / (HISTORY - 1) as f32,
                        137. - v / 100. * 17.,
                    )
                });
                if let (Some(from), Some(to)) = (previous, next) {
                    c.line(from, to, 1.5, color);
                }
                previous = next;
            }
        }

        c.glass_panel((x, 158., w, 101.), 12., 0x1d2022, 0.);
        c.design_text(
            i18n::tr("RAM per proses"),
            (x + 12., 168., w - 132., 16.),
            (11., 700, 0),
            INK,
        );
        let task_manager = (x + w - 110., 164., 101., 24.);
        c.glass_panel(
            task_manager,
            8.,
            0x303238,
            c.hover_amount(task_manager, self.figma_hover(task_manager)),
        );
        c.design_text(
            "Task Manager",
            task_manager,
            (10., 600, render::DESIGN_CENTERED),
            INK,
        );
        self.figma_hit(task_manager, Action::OpenTaskManager);
        if self.monitor.notice {
            c.design_text(
                i18n::tr("Task Manager gagal dibuka"),
                (x + 12., 202., w - 24., 20.),
                (11., 600, 0),
                MUTED,
            );
        } else if self.monitor.processes.is_empty() {
            c.design_text("—", (x + 12., 202., w - 24., 20.), (11., 600, 0), MUTED);
        } else {
            for (index, (name, size)) in self.monitor.processes.iter().enumerate() {
                let y = 193. + index as f32 * 19.;
                c.design_text(name, (x + 12., y, w - 104., 16.), (11., 500, 0), INK);
                c.design_text(
                    &format!("{} MiB", size / MIB),
                    (x + w - 82., y, 70., 16.),
                    (10., 600, 4),
                    COLORS[1],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_stay_separate_at_minimum_compact_width() {
        for height in [34., 42.] {
            for width in [148., 250., 310.] {
                let offset = (COMPACT_CLIENT_WIDTH as f32 - width) / 2.;
                let outline = render::ShellOutline {
                    floating: 0.,
                    width,
                    height,
                    shape: 0.,
                    radius: 0.,
                    offset,
                };
                let badges = Badges::new(outline);
                assert_eq!(offset - badges.cpu.0 - badges.cpu.2, 6.);
                assert_eq!(badges.memory.0 - offset - width, 6.);
                assert!(badges.memory.0 + badges.memory.2 + 1. < COMPACT_CLIENT_WIDTH as f32);
                assert_eq!(
                    half_pill(badges.memory, -1., 0.),
                    half_pill(badges.memory, 1., 0.)
                );
                assert_eq!(
                    half_pill(badges.memory, -1., 1.),
                    half_pill(badges.memory, 1., 1.)
                );
            }
        }
    }

    #[test]
    fn cpu_calculates_busy_time_across_samples() {
        assert_eq!(cpu_usage(Some((80, 100)), Some((140, 200))), Some(40.));
        assert_eq!(cpu_usage(Some((80, 100)), Some((80, 100))), None);
    }
}
