#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unsafe_op_in_unsafe_fn)]
mod artwork;
mod calendar;
mod figma_ui;
mod focus_ui;
mod host;
mod icons;
use host::*;
mod input;
mod layout;
mod lyrics;
mod lyrics_disk;
mod media;
mod model;
mod motion;
mod polish_checks;
mod power;
mod probes;
mod reminders;
mod render;
mod settings_icons;
mod shelf;
mod shelf_ui;
mod shell_policy;
mod startup;
mod tasks_ui;
mod ui;
use chrono::{Local, TimeZone};
use model::*;
use render::Canvas;
use std::{cell::RefCell, rc::Rc, time::Instant};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::*,
};

static NOTCH_HWND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static MOUSE_HOOK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

unsafe extern "system" {
    fn DragAcceptFiles(hwnd: HWND, f_accept: BOOL);
    fn DragQueryFileW(hdrop: isize, ifile: u32, lpsz_file: *mut u16, cch: u32) -> u32;
    fn DragFinish(hdrop: isize);
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 && wp.0 == WM_LBUTTONDOWN as usize {
        let root = NOTCH_HWND.load(std::sync::atomic::Ordering::Relaxed);
        if root != 0 {
            let hwnd = HWND(root as _);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Window;
            if !ptr.is_null() && (*ptr).expanded {
                let ms = &*(lp.0 as *const MSLLHOOKSTRUCT);
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    let inside_notch = ms.pt.x >= rect.left
                        && ms.pt.x < rect.right
                        && ms.pt.y >= rect.top
                        && ms.pt.y < rect.bottom;
                    if !inside_notch {
                        let settings_wnd = (*ptr).shared.borrow().settings_window;
                        let inside_settings = if !settings_wnd.is_invalid()
                            && IsWindow(Some(settings_wnd)).as_bool()
                        {
                            let mut sr = RECT::default();
                            GetWindowRect(settings_wnd, &mut sr).is_ok()
                                && ms.pt.x >= sr.left
                                && ms.pt.x < sr.right
                                && ms.pt.y >= sr.top
                                && ms.pt.y < sr.bottom
                        } else {
                            false
                        };
                        if !inside_settings {
                            let _ = PostMessageW(Some(hwnd), WM_APP + 10, WPARAM(0), LPARAM(0));
                        }
                    }
                }
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Page(usize),
    Setting(usize, i32),
    Settings,
    #[allow(dead_code)]
    SettingsAt(usize),
    TestSound,
    Focus,
    Reset,
    Media(u8),
    Seek,
    Favorite,
    Drink,
    DrinkAmount(u32),
    ResetWater,
    SetFocusMinutes(u32),
    ToggleTask(usize),
    DeleteTask(usize),
    SetTaskPriority(u8),
    TaskPriorityMenu,
    AgendaPage(i32),
    CalendarAgenda,
    AddTask,
    AddEvent,
    Month(i32),
    Date(i32),
    Today,
    EventForm,
    EditEvent(usize),
    CancelEditEvent,
    DeleteEvent(usize),
    EventLink(usize),
    RemoveShelfFile(usize),
    ClearShelfFiles,
    OpenShelfFile(usize),
    SelectShelfFile(usize),
    ShelfList,
    OpenFileFolder(usize),
    CopyFilePath(usize),
    DismissAlert,
    OpenAlert,
    Exit,
}
struct Hit {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    action: Action,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Alert {
    pub text: String,
    pub is_water: bool,
    pub page: usize,
    pub url: String,
}
#[derive(Default)]
struct Shared {
    data: Data,
    status: String,
    focus_left: u64,
    running: Option<Instant>,
    focus_accounted: Option<Instant>,
    notch: HWND,
    settings_window: HWND,
    media: Option<media::Media>,
    store: std::path::PathBuf,
    alert: Option<Alert>,
}
struct Window {
    shared: Rc<RefCell<Shared>>,
    canvas: Option<Canvas>,
    alpha_shell: bool,
    settings: bool,
    expanded: bool,
    hovered: bool,
    hover_tick: Option<Instant>,
    mode: shell_policy::Mode,
    idle_transition: Option<shell_policy::Transition>,
    cover_render_error: RefCell<String>,
    battery: power::Battery,
    battery_checked: Instant,
    battery_phase: Instant,
    power_notice: Option<power::Notice>,
    power_notification: Option<power::Notification>,
    power_changes: u64,
    last_power_change: Option<power::ConnectionChange>,
    compact_activity: figma_ui::CompactActivity,
    last_policy_check: Instant,
    page: usize,
    hits: Vec<Hit>,
    input: HWND,
    event_inputs: [HWND; 4],
    input_layout: Option<[i32; 8]>,
    calendar: calendar::Calendar,
    palette: motion::Palette,
    lyric_glow_phase: f64,
    lyric_glow_seed: u64,
    lyric_key: Option<String>,
    overlay: Option<Overlay>,
    expansion_velocity: f32,
    frame_tick: Instant,
    offscreen: bool,
    minute: i64,
    list_offset: usize,
    expansion: f32,
    peek: f32,
    peek_velocity: f32,
    pointer: (i32, i32),
    focused: Option<usize>,
    marquee_text: String,
    marquee_tick: Instant,
    title_text: String,
    title_tick: Instant,
    input_brush: HBRUSH,
    task_input_brush: HBRUSH,
    input_font: HFONT,
    input_proc: WNDPROC,
    scrubbing: Option<(i32, i32)>,
    task_priority: u8,
    sidebar_scroll: f32,
    sidebar_target_scroll: f32,
    shelf_selected: usize,
    priority_open: bool,
    preview_worker: Option<shelf::Worker>,
    preview_notify: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pressed: bool,
    feedback: Option<(usize, Instant)>,
    water_fill: f32,
    shelf_detail: bool,
}

impl Shared {
    fn enqueue_alert(&mut self, alert: Alert) {
        self.data.reminders.push(alert);
        self.alert = self.data.reminders.current();
    }
    fn dismiss_alert(&mut self) {
        self.data.reminders.next();
        self.alert = self.data.reminders.current();
    }
    fn clear_water_alerts(&mut self) {
        self.data.reminders.remove_water();
        self.alert = self.data.reminders.current();
    }
}

fn seek_to(w: &mut Window, mouse_x: i32, hit_x: i32, hit_w: i32) {
    if hit_w <= 0 {
        return;
    }
    let fraction = ((mouse_x - hit_x) as f64 / hit_w as f64).clamp(0.0, 1.0);
    let state = w.shared.borrow();
    if let Some(m) = &state.media
        && let Ok(mut snap) = m.snapshot.lock()
        && snap.duration > 0.
    {
        let target = fraction * snap.duration;
        snap.position = target;
        snap.updated = Some(Instant::now());
        snap.seek_guard = Some((target, Instant::now()));
    }
}
struct Overlay {
    bitmap: windows::Win32::Graphics::Direct2D::ID2D1Bitmap,
    started: Instant,
    duration: f32,
    shell: bool,
    direction: f32,
}
const INK: u32 = 0xf2f4f6;
const MUTED: u32 = 0x8e9aa7;
fn parse_task(input: &str) -> (String, Option<i64>) {
    if let Some((text, time)) = input.rsplit_once('@')
        && let Ok(time) = chrono::NaiveTime::parse_from_str(time.trim(), "%H:%M")
    {
        let now = Local::now();
        if let Some(mut due) = Local
            .from_local_datetime(&now.date_naive().and_time(time))
            .single()
        {
            if due <= now {
                due += chrono::Duration::days(1);
            }
            return (text.trim().into(), Some(due.timestamp()));
        }
    }
    (input.trim().into(), None)
}
const EXPANDED_HEIGHT: i32 = 280;
const EXPANDED_RADIUS: f32 = 32.;
const AGENDA_PAGE_SIZE: usize = 2;
fn panel_height(_s: &Settings) -> i32 {
    EXPANDED_HEIGHT
}
fn host_height() -> i32 {
    EXPANDED_HEIGHT
}
fn sidebar_max_scroll(_page: usize, _s: &Settings) -> f32 {
    (364. - host_height() as f32).max(0.)
}
fn record_focus(s: &mut Shared) {
    let Some(start) = s.running else {
        s.focus_accounted = None;
        return;
    };
    let now = Instant::now();
    let accounted = s.focus_accounted.unwrap_or(start).max(start);
    let available = s
        .focus_left
        .saturating_sub(accounted.saturating_duration_since(start).as_secs());
    let seconds = now
        .saturating_duration_since(accounted)
        .as_secs()
        .min(available);
    if seconds > 0 {
        *s.data
            .focus_history
            .entry(Local::now().format("%Y-%m-%d").to_string())
            .or_default() += seconds;
        s.focus_accounted = Some(accounted + std::time::Duration::from_secs(seconds));
        while s.data.focus_history.len() > 90 {
            s.data.focus_history.pop_first();
        }
    }
}
fn remaining(s: &Shared) -> u64 {
    s.focus_left
        .saturating_sub(s.running.map(|t| t.elapsed().as_secs()).unwrap_or(0))
}
fn copy_to_clipboard(text: &str) {
    if let Ok(mut child) = std::process::Command::new("clip")
        .stdin(std::process::Stdio::piped())
        .spawn()
        && let Some(mut stdin) = child.stdin.take()
    {
        let _ = std::io::Write::write_all(&mut stdin, text.as_bytes());
    }
}
unsafe fn text_input(hwnd: HWND) -> String {
    let length = GetWindowTextLengthW(hwnd);
    let mut buf = vec![0u16; length as usize + 1];
    let n = GetWindowTextW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..n as usize])
}
unsafe fn apply_frame(hwnd: HWND, w: &mut Window) {
    sync_inputs(hwnd, w);
    let s = &w.shared.borrow().data.settings;
    let (mut width, mut height) = if w.settings {
        (710, 550)
    } else {
        (s.width, host_height())
    };
    if w.settings {
        let mut frame = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        let _ = AdjustWindowRectEx(
            &mut frame,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            false,
            WINDOW_EX_STYLE(0),
        );
        width = frame.right - frame.left;
        height = frame.bottom - frame.top;
    }
    let x = if w.offscreen {
        -20000
    } else {
        (GetSystemMetrics(SM_CXSCREEN) - width) / 2
    };
    let y = if w.offscreen {
        -20000
    } else if w.settings {
        (GetSystemMetrics(SM_CYSCREEN) - height) / 2
    } else {
        0
    };
    let mut previous = RECT::default();
    let _ = GetWindowRect(hwnd, &mut previous);
    if previous.left != x
        || previous.top != y
        || previous.right - previous.left != width
        || previous.bottom - previous.top != height
    {
        let _ = SetWindowPos(
            hwnd,
            if w.settings { None } else { Some(HWND_TOPMOST) },
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE,
        );
    }
    if !w.settings {
        region(hwnd, w);
        let mut alpha = 0;
        let _ = GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), None);
        let opacity = if !w.expanded || w.page <= 2 || w.page == 4 {
            255
        } else {
            s.opacity
        };
        if alpha != opacity {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), opacity, LWA_ALPHA);
        }
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
}
fn shell_outline(w: &Window) -> render::ShellOutline {
    let s = &w.shared.borrow().data.settings;
    let p = w.expansion;
    let full_w = s.width as f32;
    let full_h = if w.page <= 2 {
        host_height()
    } else {
        panel_height(s)
    } as f32;
    let radius = 37. * (1. - p) + EXPANDED_RADIUS * p;

    let idle = w.idle_geometry();
    let width = idle.width + (full_w - idle.width) * p;
    let height = idle.height + (full_h - idle.height) * p;
    let offset = (full_w - width) / 2.;
    // Compact's exported shoulder is 9px; peek has straight sides. Expansion
    // alone cannot distinguish those two contours, so include the peek phase.
    let shape = idle.shape + (1. - idle.shape) * p;
    render::ShellOutline {
        width,
        height,
        shape,
        radius,
        offset,
    }
}
pub(crate) unsafe fn region(hwnd: HWND, w: &Window) {
    if w.mode == shell_policy::Mode::Hidden && w.expansion <= 0.0001 {
        return;
    }
    let outline = shell_outline(w);
    // Win32 regions only contain whole pixels. Leave a one-pixel guard around
    // the antialiased mask instead of cutting away its fractional coverage.
    let guard = if w.alpha_shell { 1. } else { 0. };
    let raw_points: Vec<POINT> = layout::contour(
        outline.width + guard * 2.,
        outline.height + guard,
        outline.shape,
        outline.radius + guard,
    )
    .into_iter()
    .map(|(x, y)| POINT {
        x: (x + outline.offset - guard).round() as i32,
        y: y.round() as i32,
    })
    .collect();
    let mut points: Vec<POINT> = Vec::with_capacity(raw_points.len());
    for pt in raw_points {
        if points
            .last()
            .is_none_or(|last| last.x != pt.x || last.y != pt.y)
        {
            points.push(pt);
        }
    }
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    let rgn = CreatePolygonRgn(&points, WINDING);
    // The animation owns repainting; avoid a separate default-frame redraw
    // between exposing the new region and presenting its Direct2D frame.
    if SetWindowRgn(hwnd, Some(rgn), false) == 0 {
        let _ = DeleteObject(rgn.into());
    }
}
impl Window {
    fn idle_geometry(&self) -> shell_policy::Size {
        self.idle_transition
            .as_ref()
            .map(|t| t.current)
            .unwrap_or_else(|| {
                self.mode
                    .idle_geometry(self.shared.borrow().data.settings.width as f32)
            })
    }
}
fn set_mode(w: &mut Window, next: shell_policy::Mode) {
    if next == w.mode {
        return;
    }
    let from = w.idle_geometry();
    let settings = &w.shared.borrow().data.settings;
    let to = next.idle_geometry(settings.width as f32);
    if settings.motion && from != to {
        if let Some(transition) = &mut w.idle_transition {
            transition.retarget(to, Instant::now());
        } else {
            w.idle_transition = Some(shell_policy::Transition::new(from, to, Instant::now()));
        }
    } else {
        w.idle_transition = None;
    }
    w.mode = next;
}
fn update_battery(w: &mut Window, next: power::Battery) -> bool {
    w.battery_checked = Instant::now();
    if next == w.battery {
        return false;
    }
    if let Some(change) = w.battery.connection_change(next) {
        w.power_notice = Some(power::Notice::new(change, next.percent, Instant::now()));
        w.battery_phase = Instant::now();
        w.power_changes += 1;
        w.last_power_change = Some(change);
        w.overlay = None;
    }
    w.battery = next;
    true
}
fn sync_compact_activity(w: &mut Window) {
    let state = w.shared.borrow();
    let track = state
        .media
        .as_ref()
        .and_then(|media| media.snapshot.lock().ok().map(|track| track.clone()))
        .unwrap_or_default();
    w.compact_activity
        .observe_track(&track, state.running.is_some());
    w.compact_activity.observe_alert(state.alert.as_ref());
}
unsafe fn desired_mode(hwnd: HWND, w: &Window) -> shell_policy::Mode {
    let fg = shell_policy::foreground(hwnd, w.shared.borrow().settings_window);
    let state = w.shared.borrow();
    let playing = state.media.as_ref().is_some_and(|m| {
        m.snapshot
            .lock()
            .ok()
            .is_some_and(|s| s.playing && !s.title.is_empty())
    });
    let inputs = shell_policy::Inputs {
        expanded: w.expanded,
        hovered: w.hovered,
        other_app: fg.other_app,
        fullscreen: fg.fullscreen,
        lyrics_pinned: state.data.settings.show_lyrics && playing,
        hide_fullscreen: true,
        has_alert: state.alert.is_some() || w.power_notice.is_some(),
    };
    drop(state);
    shell_policy::decide(inputs)
}
pub(crate) unsafe fn update_policy(hwnd: HWND, w: &mut Window) {
    if w.settings || w.offscreen {
        return;
    }
    let next_mode = desired_mode(hwnd, w);
    if next_mode != w.mode {
        let was_hidden = w.mode == shell_policy::Mode::Hidden;
        set_mode(w, next_mode);
        if !w.shared.borrow().data.settings.motion {
            w.peek_velocity = 0.;
            w.peek = if w.expanded || w.mode == shell_policy::Mode::Peek {
                1.
            } else {
                0.
            };
        }
        if next_mode == shell_policy::Mode::Hidden {
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            if was_hidden {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            apply_frame(hwnd, w);
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}
unsafe fn persist(w: &Window) -> bool {
    let mut state = w.shared.borrow_mut();
    state.data.focus_remaining = Some(remaining(&state));
    match state.data.save(&state.store) {
        Ok(()) => {
            state.status = "Tersimpan di perangkat ini.".into();
            true
        }
        Err(error) => {
            state.status = format!("Belum tersimpan: {error}");
            false
        }
    }
}
fn update_setting(
    state: &mut Shared,
    key: usize,
    delta: i32,
    mut startup: impl FnMut(bool) -> Result<()>,
) -> Result<()> {
    let original = state.data.settings.clone();
    state
        .data
        .settings
        .adjust(key, delta)
        .map_err(|e| Error::new(E_INVALIDARG, e))?;
    if key == 5
        && let Err(error) = startup(state.data.settings.startup)
    {
        state.data.settings = original;
        return Err(error);
    }
    if key == 6 {
        let elapsed = (original.focus_minutes as u64 * 60).saturating_sub(remaining(state));
        state.focus_left = (state.data.settings.focus_minutes as u64 * 60).saturating_sub(elapsed);
        if state.running.is_some() {
            state.running = Some(Instant::now());
        }
    }
    if matches!(key, 9 | 11 | 12 | 13) {
        state.data.water_due =
            Local::now().timestamp() + state.data.settings.water_interval as i64 * 60;
    }
    Ok(())
}
unsafe fn send_spotify_like() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
        VK_MENU, VK_SHIFT,
    };
    let vk_b = VIRTUAL_KEY(0x42);
    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_MENU,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_SHIFT,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk_b,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk_b,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_SHIFT,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_MENU,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
    ];
    let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
}
unsafe fn action(hwnd: HWND, w: &mut Window, a: Action) {
    if matches!(
        a,
        Action::Focus | Action::Reset | Action::SetFocusMinutes(_)
    ) {
        record_focus(&mut w.shared.borrow_mut());
    }
    if let Some(hit) = w
        .hits
        .iter()
        .find(|h| {
            h.action == a
                && w.pointer.0 >= h.x
                && w.pointer.0 < h.x + h.w
                && w.pointer.1 >= h.y
                && w.pointer.1 < h.y + h.h
        })
        .or_else(|| w.hits.iter().find(|h| h.action == a))
        && let Some(canvas) = &w.canvas
    {
        let (scale, offset) = if w.settings {
            (1., 0.)
        } else {
            w.figma_transform()
        };
        canvas.pulse((
            ((hit.x as f32 - offset) / scale).round() as i32,
            hit.y,
            (hit.w as f32 / scale).round() as i32,
            hit.h,
        ));
    }
    if !w.settings
        && matches!(
            a,
            Action::Month(_)
                | Action::Today
                | Action::Date(_)
                | Action::AgendaPage(_)
                | Action::EventForm
                | Action::EditEvent(_)
                | Action::CancelEditEvent
                | Action::AddEvent
                | Action::DeleteEvent(_)
                | Action::Focus
                | Action::Reset
                | Action::SetFocusMinutes(_)
                | Action::DeleteTask(_)
                | Action::AddTask
                | Action::TaskPriorityMenu
                | Action::SetTaskPriority(_)
                | Action::SelectShelfFile(_)
                | Action::ShelfList
                | Action::RemoveShelfFile(_)
                | Action::ClearShelfFiles
        )
    {
        begin_transition(w, false, motion::CONTENT);
    }
    match a {
        Action::Settings => {
            let _ = create(w.shared.clone(), true, w.offscreen);
        }
        Action::SettingsAt(page) => {
            if let Ok(dialog) = create(w.shared.clone(), true, w.offscreen) {
                let p = GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Window;
                if !p.is_null() {
                    (*p).page = page;
                    (*p).focused = None;
                    let _ = InvalidateRect(Some(dialog), None, false);
                }
            }
        }
        Action::TestSound => {
            if w.shared.borrow().data.settings.sound && !w.offscreen {
                let _ = MessageBeep(MB_ICONASTERISK);
            }
            return;
        }
        Action::Exit => {
            persist(w);
            let _ = PostMessageW(None, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        Action::Page(p) => {
            if !w.settings && w.page != p {
                begin_transition(w, false, motion::PAGE);
                if let Some(overlay) = &mut w.overlay {
                    overlay.direction = if p > w.page { -1. } else { 1. };
                }
            }
            if w.page == 5 && p != 5 {
                w.preview_worker = None;
                if let Some(canvas) = &w.canvas {
                    canvas.release_preview();
                }
            }
            w.page = p;
            w.focused = None;
            w.list_offset = 0;
            w.priority_open = false;
            w.shelf_detail = false;
            let step = 48.;
            let item_h = 40.;
            let viewport = (host_height() - 84) as f32;
            let target_y = p as f32 * step;
            if target_y < w.sidebar_target_scroll {
                w.sidebar_target_scroll = target_y;
            } else if target_y + item_h > w.sidebar_target_scroll + viewport {
                w.sidebar_target_scroll = target_y + item_h - viewport;
            }
            w.sidebar_target_scroll = w
                .sidebar_target_scroll
                .clamp(0., sidebar_max_scroll(p, &w.shared.borrow().data.settings));
            if !w.shared.borrow().data.settings.motion {
                w.sidebar_scroll = w.sidebar_target_scroll;
            }
        }
        Action::Month(delta) => w.calendar.move_month(delta),
        Action::CalendarAgenda => w.calendar.target_scroll = 202.,
        Action::AgendaPage(delta) => {
            let count = w
                .shared
                .borrow()
                .data
                .events
                .iter()
                .filter(|event| {
                    Local
                        .timestamp_opt(event.at, 0)
                        .single()
                        .is_some_and(|date| date.date_naive() == w.calendar.selected)
                })
                .count();
            w.calendar.agenda_offset = if delta < 0 {
                w.calendar.agenda_offset.saturating_sub(AGENDA_PAGE_SIZE)
            } else {
                (w.calendar.agenda_offset + AGENDA_PAGE_SIZE)
                    .min(count.saturating_sub(1) / AGENDA_PAGE_SIZE * AGENDA_PAGE_SIZE)
            };
        }
        Action::Date(day) => {
            if let Some(date) = chrono::NaiveDate::from_num_days_from_ce_opt(day) {
                w.calendar.select(date);
                if w.calendar.editing.is_some() {
                    w.calendar.editing = None;
                    let _ = SetWindowTextW(w.event_inputs[0], w!(""));
                    let _ = SetWindowTextW(w.event_inputs[1], w!(""));
                    let lead = w
                        .shared
                        .borrow()
                        .data
                        .settings
                        .event_lead_minutes
                        .to_string();
                    let _ = SetWindowTextW(w.event_inputs[2], &HSTRING::from(lead));
                    let _ = SetWindowTextW(w.event_inputs[3], w!(""));
                    w.calendar.status.clear();
                }
            }
        }
        Action::Today => {
            w.calendar = calendar::Calendar::default();
        }
        Action::EventForm => {
            if w.calendar.editing.is_some() {
                w.calendar.editing = None;
                let _ = SetWindowTextW(w.event_inputs[0], w!(""));
                let _ = SetWindowTextW(w.event_inputs[1], w!(""));
                let lead = w
                    .shared
                    .borrow()
                    .data
                    .settings
                    .event_lead_minutes
                    .to_string();
                let _ = SetWindowTextW(w.event_inputs[2], &HSTRING::from(lead));
                let _ = SetWindowTextW(w.event_inputs[3], w!(""));
                w.calendar.status.clear();
                w.calendar.form = true;
                w.calendar.target_scroll = (w.calendar.form_y - 26.).max(0.);
            } else {
                w.calendar.form = !w.calendar.form;
                if w.calendar.form {
                    let lead = w
                        .shared
                        .borrow()
                        .data
                        .settings
                        .event_lead_minutes
                        .to_string();
                    let _ = SetWindowTextW(w.event_inputs[2], &HSTRING::from(lead));
                    w.calendar.target_scroll = (w.calendar.form_y - 26.).max(0.);
                } else {
                    w.calendar.target_scroll = 0.;
                }
            }
        }
        Action::EditEvent(index) => {
            let state = w.shared.borrow();
            if let Some(event) = state.data.events.get(index) {
                let _ = SetWindowTextW(w.event_inputs[0], &HSTRING::from(&event.title));
                let time_str = Local
                    .timestamp_opt(event.at, 0)
                    .single()
                    .map(|d| d.format("%H:%M").to_string())
                    .unwrap_or_default();
                let _ = SetWindowTextW(w.event_inputs[1], &HSTRING::from(time_str));
                let lead = event
                    .lead_minutes
                    .unwrap_or(state.data.settings.event_lead_minutes)
                    .to_string();
                let _ = SetWindowTextW(w.event_inputs[2], &HSTRING::from(lead));
                let _ = SetWindowTextW(w.event_inputs[3], &HSTRING::from(&event.url));
                drop(state);
                w.calendar.editing = Some(index);
                w.calendar.form = true;
                w.calendar.status = "Ubah detail acara lalu klik Perbarui acara.".into();
                w.calendar.target_scroll = (w.calendar.form_y - 26.).max(0.);
            }
        }
        Action::CancelEditEvent => {
            w.calendar.form = false;
            w.calendar.target_scroll = 0.;
            w.calendar.editing = None;
            let _ = SetWindowTextW(w.event_inputs[0], w!(""));
            let _ = SetWindowTextW(w.event_inputs[1], w!(""));
            let lead = w
                .shared
                .borrow()
                .data
                .settings
                .event_lead_minutes
                .to_string();
            let _ = SetWindowTextW(w.event_inputs[2], &HSTRING::from(lead));
            let _ = SetWindowTextW(w.event_inputs[3], w!(""));
            w.calendar.status.clear();
        }
        Action::DeleteEvent(index) => {
            let mut state = w.shared.borrow_mut();
            if index < state.data.events.len() {
                state.data.events.remove(index);
                drop(state);
                if let Some(editing_idx) = w.calendar.editing {
                    if editing_idx == index {
                        w.calendar.editing = None;
                        let _ = SetWindowTextW(w.event_inputs[0], w!(""));
                        let _ = SetWindowTextW(w.event_inputs[1], w!(""));
                        let lead = w
                            .shared
                            .borrow()
                            .data
                            .settings
                            .event_lead_minutes
                            .to_string();
                        let _ = SetWindowTextW(w.event_inputs[2], &HSTRING::from(lead));
                        let _ = SetWindowTextW(w.event_inputs[3], w!(""));
                        w.calendar.status.clear();
                    } else if index < editing_idx {
                        w.calendar.editing = Some(editing_idx - 1);
                    }
                }
                let _ = persist(w);
            }
        }
        Action::EventLink(index) => {
            if index == 999 {
                if let Some(url) = w.shared.borrow().alert.as_ref().map(|a| a.url.clone())
                    && !w.offscreen
                    && !url.is_empty()
                {
                    let _ = calendar::open_url(hwnd, &url);
                }
                return;
            }
            let state = w.shared.borrow();
            if let Some(event) = state.data.events.get(index)
                && !w.offscreen
                && !calendar::open_url(hwnd, &event.url)
            {
                w.calendar.status = "Link belum bisa dibuka.".into();
            }
        }
        Action::Focus => {
            let mut s = w.shared.borrow_mut();
            if s.running.is_some() {
                s.focus_left = remaining(&s);
                s.running = None;
            } else {
                if s.focus_left == 0 {
                    s.focus_left = s.data.settings.focus_minutes as u64 * 60;
                }
                s.running = Some(Instant::now());
            }
        }
        Action::Seek => return,
        Action::Favorite => {
            let mut s = w.shared.borrow_mut();
            let track = s
                .media
                .as_ref()
                .and_then(|m| m.snapshot.lock().ok().map(|t| t.clone()))
                .unwrap_or_default();
            if !track.title.is_empty() {
                let key = format!("{}|{}", track.title, track.artist);
                if let Some(pos) = s.data.favorites.iter().position(|f| f == &key) {
                    s.data.favorites.remove(pos);
                } else {
                    s.data.favorites.push(key);
                }
                let _ = s.data.save(&s.store);
            }
            unsafe {
                send_spotify_like();
            }
        }
        Action::Media(command) => {
            if let Some(m) = &w.shared.borrow().media {
                let _ = m.send.send(match command {
                    1 => media::Command::Previous,
                    2 => media::Command::Next,
                    _ => media::Command::Toggle,
                });
            }
        }
        Action::Reset => {
            let mut s = w.shared.borrow_mut();
            s.running = None;
            s.focus_left = s.data.settings.focus_minutes as u64 * 60;
        }
        Action::Drink => {
            let mut s = w.shared.borrow_mut();
            s.data.water = s.data.water.saturating_add(s.data.settings.cup);
            s.data.water_due =
                Local::now().timestamp() + s.data.settings.water_interval as i64 * 60;
            s.clear_water_alerts();
            w.feedback = Some((3, Instant::now()));
        }
        Action::DrinkAmount(amt) => {
            let mut s = w.shared.borrow_mut();
            s.data.water = s.data.water.saturating_add(amt);
            s.data.water_due =
                Local::now().timestamp() + s.data.settings.water_interval as i64 * 60;
            s.clear_water_alerts();
            w.feedback = Some((3, Instant::now()));
        }
        Action::ResetWater => {
            let mut s = w.shared.borrow_mut();
            s.data.water = 0;
            s.clear_water_alerts();
            w.feedback = Some((3, Instant::now()));
        }
        Action::SetFocusMinutes(minutes) => {
            let mut s = w.shared.borrow_mut();
            s.data.settings.focus_minutes = minutes;
            s.running = None;
            s.focus_left = minutes as u64 * 60;
        }
        Action::DismissAlert => {
            w.shared.borrow_mut().dismiss_alert();
            persist(w);
        }
        Action::OpenAlert => {
            let has_music = w
                .shared
                .borrow()
                .media
                .as_ref()
                .is_some_and(|m| m.snapshot.lock().is_ok_and(|s| s.playing));
            let default_page = if has_music { 0 } else { 2 };
            let page = w
                .shared
                .borrow()
                .alert
                .as_ref()
                .map(|a| a.page)
                .unwrap_or(default_page);
            w.page = page;
            w.shared.borrow_mut().dismiss_alert();
            persist(w);
            w.expanded = true;
            set_mode(w, shell_policy::Mode::Expanded);
            animate(hwnd, w);
            apply_frame(hwnd, w);
        }
        Action::DeleteTask(i) => {
            let mut s = w.shared.borrow_mut();
            if i < s.data.tasks.len() {
                s.data.tasks.remove(i);
            }
            drop(s);
            let count = w.shared.borrow().data.tasks.len();
            w.list_offset = w.list_offset.min(count.saturating_sub(4));
        }
        Action::RemoveShelfFile(i) => {
            let mut s = w.shared.borrow_mut();
            if i < s.data.shelf_files.len() {
                s.data.shelf_files.remove(i);
            }
            drop(s);
            let count = w.shared.borrow().data.shelf_files.len();
            w.list_offset = w.list_offset.min(count.saturating_sub(4));
            w.shelf_selected = w.shelf_selected.min(count.saturating_sub(1));
        }
        Action::ClearShelfFiles => {
            w.shelf_detail = false;
            w.shared.borrow_mut().data.shelf_files.clear();
            w.list_offset = 0;
            w.shelf_selected = 0;
        }
        Action::OpenShelfFile(i) => {
            let state = w.shared.borrow();
            if let Some(f) = state.data.shelf_files.get(i) {
                let path = f.path.clone();
                drop(state);
                if !w.offscreen && !calendar::open_file(hwnd, &path) {
                    w.shared.borrow_mut().status = "Berkas tidak tersedia".into();
                }
            }
        }
        Action::SelectShelfFile(i) => {
            w.shelf_selected = i;
            w.shelf_detail = w.shared.borrow().data.settings.width < 488;
            w.shared.borrow_mut().status = "Siap".into();
        }
        Action::ShelfList => w.shelf_detail = false,
        Action::OpenFileFolder(i) => {
            let state = w.shared.borrow();
            if let Some(f) = state.data.shelf_files.get(i) {
                let path = f.path.clone();
                drop(state);
                let _ = std::process::Command::new("explorer")
                    .arg(format!("/select,\"{path}\""))
                    .spawn();
            }
        }
        Action::CopyFilePath(i) => {
            let state = w.shared.borrow();
            if let Some(f) = state.data.shelf_files.get(i) {
                let path = f.path.clone();
                drop(state);
                copy_to_clipboard(&path);
            }
        }
        Action::ToggleTask(i) => {
            if let Some(t) = w.shared.borrow_mut().data.tasks.get_mut(i) {
                t.done = !t.done;
            }
            w.feedback = Some((4, Instant::now()));
        }
        Action::TaskPriorityMenu => {
            let keyboard = w.focused.is_some();
            w.priority_open = !w.priority_open;
            w.focused = if keyboard && w.priority_open {
                Some(match w.task_priority {
                    1 => 0,
                    2 => 2,
                    _ => 1,
                })
            } else {
                None
            };
        }
        Action::SetTaskPriority(p) => {
            w.task_priority = p;
            w.priority_open = false;
            w.focused = None;
        }
        Action::AddTask => {
            let input = text_input(w.input);
            if !input.trim().is_empty() {
                let (text, due) = parse_task(&input);
                if text.is_empty() {
                    return;
                }
                w.shared.borrow_mut().data.tasks.push(Task {
                    text,
                    done: false,
                    due,
                    notified: false,
                    priority: w.task_priority,
                });
                let _ = SetWindowTextW(w.input, w!(""));
            }
        }
        Action::AddEvent => {
            let fields = w.event_inputs.map(|h| text_input(h));
            let existing_at = w
                .calendar
                .editing
                .and_then(|idx| w.shared.borrow().data.events.get(idx).map(|e| e.at));
            match calendar::make_event_for_edit(
                w.calendar.selected,
                &fields[0],
                &fields[1],
                &fields[2],
                &fields[3],
                existing_at,
                Local::now().timestamp(),
            ) {
                Ok(mut event) => {
                    let mut state = w.shared.borrow_mut();
                    let was_editing = w.calendar.editing;
                    let prev_event = if let Some(idx) = was_editing {
                        if idx < state.data.events.len() {
                            let old = state.data.events[idx].clone();
                            if old.at != event.at {
                                event.notified = false;
                            } else {
                                event.notified = old.notified;
                            }
                            state.data.events[idx] = event;
                            Some((idx, old))
                        } else {
                            state.data.events.push(event);
                            None
                        }
                    } else {
                        state.data.events.push(event);
                        None
                    };
                    drop(state);
                    if !persist(w) {
                        let mut state = w.shared.borrow_mut();
                        if let Some((idx, old)) = prev_event {
                            state.data.events[idx] = old;
                        } else {
                            state.data.events.pop();
                        }
                        w.calendar.status = state.status.clone();
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return;
                    }
                    w.calendar.editing = None;
                    let _ = SetWindowTextW(w.event_inputs[0], w!(""));
                    let _ = SetWindowTextW(w.event_inputs[1], w!(""));
                    let lead = w
                        .shared
                        .borrow()
                        .data
                        .settings
                        .event_lead_minutes
                        .to_string();
                    let _ = SetWindowTextW(w.event_inputs[2], &HSTRING::from(lead));
                    let _ = SetWindowTextW(w.event_inputs[3], w!(""));
                    w.calendar.status = if was_editing.is_some() {
                        "Perubahan acara berhasil disimpan.".into()
                    } else {
                        "Acara tersimpan. Pengingat aktif selama Horizon berjalan.".into()
                    };
                }
                Err(error) => {
                    w.calendar.status = error;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return;
                }
            }
        }
        Action::Setting(k, d) => {
            let mut state = w.shared.borrow_mut();
            let original = (
                state.data.settings.clone(),
                state.focus_left,
                state.running,
                state.data.water_due,
            );
            let result = update_setting(&mut state, k, d, |enabled| {
                if w.offscreen {
                    Ok(())
                } else {
                    startup::apply(enabled)
                }
            });
            if let Err(error) = result {
                state.status = format!("Pengaturan belum diubah: {error}");
                drop(state);
                apply_frame(hwnd, w);
                return;
            }
            drop(state);
            if !persist(w) {
                let error = w.shared.borrow().status.clone();
                let rollback = if k == 5 && !w.offscreen {
                    startup::apply(original.0.startup)
                } else {
                    Ok(())
                };
                let mut state = w.shared.borrow_mut();
                state.data.settings = original.0;
                state.focus_left = original.1;
                state.running = original.2;
                state.data.water_due = original.3;
                state.status = if let Err(e) = rollback {
                    format!("{error}; startup belum dipulihkan: {e}")
                } else {
                    error
                };
                drop(state);
                apply_frame(hwnd, w);
                return;
            }
            if k == 16
                && let Some(m) = &w.shared.borrow().media
            {
                m.lyrics_enabled.store(
                    w.shared.borrow().data.settings.show_lyrics,
                    std::sync::atomic::Ordering::Relaxed,
                );
                let _ = m.send.send(media::Command::LyricsChanged);
            }
            if k == 4 {
                let root = w.shared.borrow().notch;
                if !root.is_invalid() {
                    let _ = PostMessageW(Some(root), WM_APP + 8, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
    if matches!(
        a,
        Action::Focus
            | Action::Reset
            | Action::SetFocusMinutes(_)
            | Action::Drink
            | Action::DrinkAmount(_)
            | Action::ResetWater
            | Action::ToggleTask(_)
            | Action::DeleteTask(_)
            | Action::AddTask
            | Action::DeleteEvent(_)
            | Action::RemoveShelfFile(_)
            | Action::ClearShelfFiles
    ) {
        persist(w);
    }
    let root = w.shared.borrow().notch;
    sync_compact_activity(w);
    if w.settings && !root.is_invalid() {
        let _ = PostMessageW(Some(root), WM_APP + 5, WPARAM(0), LPARAM(0));
    }
    update_policy(hwnd, w);
    apply_frame(hwnd, w);
}
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = &*(lp.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Window;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let w = &mut *ptr;
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut ps);
            if w.canvas.is_none() {
                let size = if w.settings {
                    (710, 550)
                } else {
                    {
                        let s = &w.shared.borrow().data.settings;
                        (s.width, host_height())
                    }
                };
                w.canvas = Canvas::new(hwnd, size.0 as u32, size.1 as u32, w.alpha_shell).ok();
            }
            if w.paint().is_err() {
                w.canvas = None;
                w.overlay = None;
            }
            sync_inputs(hwnd, w);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        // The notch has no system frame. In particular, the initial non-client
        // paint/activation must not insert a light border around its region.
        WM_NCPAINT if !w.settings => LRESULT(0),
        WM_NCACTIVATE if !w.settings => LRESULT(1),
        WM_KEYDOWN => {
            match wp.0 {
                9 => {
                    let count = w.hits.len();
                    if count > 0 {
                        let backwards = GetKeyState(VK_SHIFT.0 as i32) < 0;
                        let index = w.focused.unwrap_or(if backwards { 0 } else { count - 1 });
                        w.focused = Some(if backwards {
                            (index + count - 1) % count
                        } else {
                            (index + 1) % count
                        });
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                13 | 32 => {
                    if let Some(a) = w.focused.and_then(|i| w.hits.get(i).map(|h| h.action)) {
                        action(hwnd, w, a);
                    }
                }
                38 | 40 if !w.settings && w.page == 4 => {
                    if w.priority_open {
                        let selected = match w.task_priority {
                            1 => 0,
                            2 => 2,
                            _ => 1,
                        };
                        w.focused = Some(
                            w.focused
                                .filter(|i| *i < 3)
                                .map_or(selected, |i| (i + if wp.0 == 38 { 2 } else { 1 }) % 3),
                        );
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    } else if w
                        .focused
                        .and_then(|i| w.hits.get(i))
                        .is_some_and(|h| matches!(h.action, Action::TaskPriorityMenu))
                    {
                        action(hwnd, w, Action::TaskPriorityMenu);
                    }
                }
                37 | 39 => {
                    if let Some(Action::Setting(key, step)) =
                        w.focused.and_then(|i| w.hits.get(i).map(|h| h.action))
                        && !matches!(key, 4 | 5 | 10 | 11 | 15 | 16)
                    {
                        action(
                            hwnd,
                            w,
                            Action::Setting(key, if wp.0 == 37 { -step.abs() } else { step.abs() }),
                        );
                    }
                }
                27 => {
                    if w.priority_open {
                        w.priority_open = false;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    } else if w.settings {
                        let _ = DestroyWindow(hwnd);
                    } else {
                        w.expanded = false;
                        let mode = if w.hovered {
                            shell_policy::Mode::Peek
                        } else {
                            shell_policy::Mode::Compact
                        };
                        set_mode(w, mode);
                        animate(hwnd, w);
                        apply_frame(hwnd, w);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let hit_test = (lp.0 & 0xffff) as i16;
            if hit_test == HTCLIENT as i16 {
                let (x, y) = w.pointer;
                let hand = w.scrubbing.is_some()
                    || w.hits
                        .iter()
                        .any(|h| x >= h.x && x < h.x + h.w && y >= h.y && y < h.y + h.h);
                let cursor_name = if hand { IDC_HAND } else { IDC_ARROW };
                unsafe {
                    let _ = SetCursor(LoadCursorW(None, cursor_name).ok());
                }
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_LBUTTONDOWN => {
            w.pressed = true;
            let _ = InvalidateRect(Some(hwnd), None, false);
            let x = (lp.0 as u16) as i16 as i32;
            let y = ((lp.0 >> 16) as u16) as i16 as i32;
            w.pointer = (x, y);
            // Present the physical press before a fast release can overtake it.
            let _ = UpdateWindow(hwnd);
            if let Some(h) = w.hits.iter().find(|h| {
                x >= h.x
                    && x < h.x + h.w
                    && y >= h.y
                    && y < h.y + h.h
                    && matches!(h.action, Action::Seek)
            }) {
                w.scrubbing = Some((h.x, h.w));
                unsafe {
                    let _ = SetCapture(hwnd);
                }
                seek_to(w, x, h.x, h.w);
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let p = (
                (lp.0 as u16) as i16 as i32,
                ((lp.0 >> 16) as u16) as i16 as i32,
            );
            if p != w.pointer {
                w.pointer = p;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            if let Some((hit_x, hit_w)) = w.scrubbing {
                seek_to(w, p.0, hit_x, hit_w);
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }
            if !w.settings {
                w.hover_tick = None;
                if !w.hovered && !w.expanded {
                    w.hovered = true;
                    update_policy(hwnd, w);
                }
            }
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                ..Default::default()
            };
            let _ = TrackMouseEvent(&mut tracking);
            LRESULT(0)
        }
        0x02A3 => {
            w.pressed = false;
            w.pointer = (-1, -1);
            if !w.settings {
                w.hover_tick = Some(Instant::now());
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
            let dc = HDC(wp.0 as *mut _);
            SetTextColor(dc, input::TEXT_COLORREF);
            if !w.settings && w.page == 4 {
                SetBkColor(dc, input::TASK_FIELD_COLORREF);
                return LRESULT(w.task_input_brush.0 as isize);
            }
            SetBkColor(dc, input::FIELD_COLORREF);
            LRESULT(w.input_brush.0 as isize)
        }
        m if m == WM_APP + 7 => {
            action(
                hwnd,
                w,
                if w.page == 2 {
                    Action::AddEvent
                } else {
                    Action::AddTask
                },
            );
            LRESULT(0)
        }
        m if m == WM_APP + 8 => {
            if !w.shared.borrow().data.settings.motion {
                let _ = KillTimer(Some(hwnd), 2);
                w.expansion = if w.expanded { 1. } else { 0. };
                w.peek = if w.expanded || w.mode == shell_policy::Mode::Peek {
                    1.
                } else {
                    0.
                };
                w.overlay = None;
                region(hwnd, w);
            }
            LRESULT(0)
        }
        0x0233 => {
            let hdrop = wp.0 as isize;
            let count = unsafe { DragQueryFileW(hdrop, 0xFFFFFFFF, std::ptr::null_mut(), 0) };
            let mut added = false;
            for i in 0..count {
                let length = DragQueryFileW(hdrop, i, std::ptr::null_mut(), 0);
                if length == 0 || length > 32767 {
                    continue;
                }
                let mut buf = vec![0u16; length as usize + 1];
                let len = unsafe { DragQueryFileW(hdrop, i, buf.as_mut_ptr(), buf.len() as u32) };
                if len > 0 {
                    let path = String::from_utf16_lossy(&buf[..len as usize]);
                    w.shared.borrow_mut().data.add_shelf_file(path);
                    added = true;
                }
            }
            unsafe {
                DragFinish(hdrop);
            }
            if added {
                persist(w);
                w.page = 5;
                w.expanded = true;
                w.list_offset = 0;
                w.feedback = Some((5, Instant::now()));
                animate(hwnd, w);
                update_policy(hwnd, w);
                apply_frame(hwnd, w);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL if w.expanded && w.pointer.0 >= 0 && w.pointer.0 < 62 => {
            let delta = ((wp.0 >> 16) as u16) as i16;
            let max_scroll = sidebar_max_scroll(w.page, &w.shared.borrow().data.settings);
            w.sidebar_target_scroll =
                (w.sidebar_target_scroll - (delta as f32 / 120.) * 36.).clamp(0., max_scroll);
            if !w.shared.borrow().data.settings.motion {
                w.sidebar_scroll = w.sidebar_target_scroll;
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_MOUSEWHEEL if w.expanded && w.page == 2 => {
            let delta = ((wp.0 >> 16) as u16) as i16;
            let viewport = panel_height(&w.shared.borrow().data.settings) as f32 - 34.;
            w.calendar.wheel(-(delta as f32 / 120.) * 66., viewport);
            if !w.shared.borrow().data.settings.motion {
                w.calendar.scroll = w.calendar.target_scroll;
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_MOUSEWHEEL if w.expanded && w.page == 4 => {
            let count = w.shared.borrow().data.tasks.len();
            let delta = ((wp.0 >> 16) as u16) as i16;
            begin_transition(w, false, motion::CONTENT);
            if delta < 0 {
                w.list_offset = (w.list_offset + 1).min(count.saturating_sub(4));
            } else {
                w.list_offset = w.list_offset.saturating_sub(1);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_MOUSEWHEEL if w.expanded && w.page == 5 => {
            let count = w.shared.borrow().data.shelf_files.len();
            let delta = ((wp.0 >> 16) as u16) as i16;
            begin_transition(w, false, motion::CONTENT);
            if delta < 0 {
                w.list_offset = (w.list_offset + 1).min(count.saturating_sub(4));
            } else {
                w.list_offset = w.list_offset.saturating_sub(1);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_SIZE => {
            w.canvas = None;
            w.overlay = None;
            LRESULT(0)
        }
        m if m == WM_APP + 5 => {
            w.overlay = None;
            apply_frame(hwnd, w);
            LRESULT(0)
        }
        m if m == WM_APP + 9 => {
            sync_compact_activity(w);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        m if m == WM_APP + 12 => {
            let _ = probes::running_media_diagnostics(w);
            LRESULT(0)
        }
        m if m == WM_APP + 10 => {
            if w.expanded {
                w.expanded = false;
                let mode = if w.hovered {
                    shell_policy::Mode::Peek
                } else {
                    shell_policy::Mode::Compact
                };
                set_mode(w, mode);
                animate(hwnd, w);
                update_policy(hwnd, w);
                apply_frame(hwnd, w);
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == 3 => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_HOTKEY => {
            w.expanded = !w.expanded;
            let mode = if w.expanded {
                shell_policy::Mode::Expanded
            } else if w.hovered {
                shell_policy::Mode::Peek
            } else {
                shell_policy::Mode::Compact
            };
            set_mode(w, mode);
            animate(hwnd, w);
            apply_frame(hwnd, w);
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let _ = create(w.shared.clone(), true, w.offscreen);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            w.pressed = false;
            let x = (lp.0 as u16) as i16 as i32;
            let y = ((lp.0 >> 16) as u16) as i16 as i32;
            if w.priority_open {
                let width = w.shared.borrow().data.settings.width;
                if !input::priority_popup(width).contains(x, y)
                    && !input::priority_pill(width).contains(x, y)
                {
                    w.priority_open = false;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
            }
            if let Some((hit_x, hit_w)) = w.scrubbing.take() {
                unsafe {
                    let _ = ReleaseCapture();
                }
                if hit_w > 0 {
                    let fraction = ((x - hit_x) as f64 / hit_w as f64).clamp(0.0, 1.0);
                    let state = w.shared.borrow();
                    if let Some(m) = &state.media {
                        if let Ok(mut snap) = m.snapshot.lock()
                            && snap.duration > 0.
                        {
                            let target = fraction * snap.duration;
                            snap.position = target;
                            snap.updated = Some(Instant::now());
                            snap.seek_guard = Some((target, Instant::now()));
                        }
                        let _ = m.send.send(media::Command::Seek(fraction));
                    }
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }
            if let Some(h) = w
                .hits
                .iter()
                .find(|h| x >= h.x && x < h.x + h.w && y >= h.y && y < h.y + h.h)
            {
                if matches!(h.action, Action::Seek) {
                    if h.w > 0 {
                        let fraction = ((x - h.x) as f64 / h.w as f64).clamp(0.0, 1.0);
                        let state = w.shared.borrow();
                        if let Some(m) = &state.media {
                            if let Ok(mut snap) = m.snapshot.lock()
                                && snap.duration > 0.
                            {
                                let target = fraction * snap.duration;
                                snap.position = target;
                                snap.updated = Some(Instant::now());
                                snap.seek_guard = Some((target, Instant::now()));
                            }
                            let _ = m.send.send(media::Command::Seek(fraction));
                        }
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else {
                    let a = h.action;
                    action(hwnd, w, a);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            } else if !w.settings {
                if input::focus_at(w, x, y) {
                    return LRESULT(0);
                }
                w.expanded = !w.expanded;
                let mode = if w.expanded {
                    shell_policy::Mode::Expanded
                } else if w.hovered {
                    shell_policy::Mode::Peek
                } else {
                    shell_policy::Mode::Compact
                };
                set_mode(w, mode);
                animate(hwnd, w);
                apply_frame(hwnd, w);
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            w.pressed = false;
            w.scrubbing = None;
            LRESULT(0)
        }
        WM_TIMER => {
            let mut power_changed = false;
            if w.power_notice
                .as_ref()
                .is_some_and(|notice| notice.expired(Instant::now()))
            {
                w.power_notice = None;
                power_changed = true;
            }
            if !w.settings && !w.offscreen && w.battery_checked.elapsed().as_secs() >= 1 {
                w.battery_checked = Instant::now();
                if let Some(battery) = power::Battery::read() {
                    power_changed |= update_battery(w, battery);
                }
            }
            let mut s = w.shared.borrow_mut();
            record_focus(&mut s);
            let minute = Local::now().timestamp() / 60;
            let mut changed = power_changed || (w.expanded && w.page == 0) || minute != w.minute;
            w.minute = minute;
            s.data.reset_day();
            if s.running.is_some() {
                changed = true;
                if remaining(&s) == 0 {
                    s.running = None;
                    s.focus_left = 0;
                    s.data.focus_remaining = Some(0);
                    let _ = s.data.save(&s.store);
                    s.status = "Sesi fokus selesai. Ambil jeda.".into();
                    if s.data.settings.sound && !w.offscreen {
                        let _ = MessageBeep(MB_OK);
                    }
                }
            }
            let mut alerts = reminders::collect(&mut s.data, Local::now().timestamp());
            if s.data.water_tick() {
                alerts.push(Alert {
                    text: format!("{} / {} ml", s.data.water, s.data.settings.water_goal),
                    is_water: true,
                    page: 3,
                    url: String::new(),
                });
            }
            let alert = !alerts.is_empty();
            for alert in alerts {
                s.enqueue_alert(alert);
            }
            if alert {
                s.status = "Ada pengingat. Ambil jeda sebentar.".into();
                if s.data.settings.sound && !w.offscreen {
                    let _ = MessageBeep(MB_OK);
                }
                let _ = s.data.save(&s.store);
                changed = true;
            }
            drop(s);
            sync_compact_activity(w);
            if !w.settings {
                update_policy(hwnd, w);
            }
            if changed {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
            if !w.settings
                && !w.offscreen
                && let Some(battery) = power::Battery::read()
            {
                update_battery(w, battery);
                update_policy(hwnd, w);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(1)
        }
        WM_CLOSE => {
            if w.settings {
                let _ = DestroyWindow(hwnd);
            } else {
                persist(w);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            w.preview_notify
                .store(0, std::sync::atomic::Ordering::Relaxed);
            if w.settings {
                w.shared.borrow_mut().settings_window = HWND::default();
            } else {
                let hook = MOUSE_HOOK.swap(0, std::sync::atomic::Ordering::Relaxed);
                if hook != 0 {
                    let _ = UnhookWindowsHookEx(HHOOK(hook as _));
                }
                NOTCH_HWND.store(0, std::sync::atomic::Ordering::Relaxed);
                if let Some(m) = &w.shared.borrow().media {
                    m.notify.store(0, std::sync::atomic::Ordering::Relaxed);
                }
            }
            let _ = KillTimer(Some(hwnd), 1);
            let _ = KillTimer(Some(hwnd), 2);
            let _ = UnregisterHotKey(Some(hwnd), 1);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !w.input_brush.is_invalid() {
                let _ = DeleteObject(w.input_brush.into());
            }
            if !w.task_input_brush.is_invalid() {
                let _ = DeleteObject(w.task_input_brush.into());
            }
            if !w.input_font.is_invalid() {
                let _ = DeleteObject(w.input_font.into());
            }
            drop(Box::from_raw(ptr));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn create(shared: Rc<RefCell<Shared>>, settings: bool, offscreen: bool) -> Result<HWND> {
    if settings {
        let existing = shared.borrow().settings_window;
        if !existing.is_invalid() && IsWindow(Some(existing)).as_bool() {
            if !offscreen {
                let _ = ShowWindow(existing, SW_RESTORE);
                let _ = SetForegroundWindow(existing);
            }
            return Ok(existing);
        }
    }
    trace("create: module");
    let instance = GetModuleHandleW(None)?;
    let w = Box::new(Window {
        shared,
        canvas: None,
        alpha_shell: false,
        settings,
        expanded: false,
        hovered: false,
        hover_tick: None,
        mode: shell_policy::Mode::Compact,
        idle_transition: None,
        cover_render_error: RefCell::new(String::new()),
        battery: if offscreen {
            power::Battery::default()
        } else {
            power::Battery::read().unwrap_or_default()
        },
        battery_checked: Instant::now(),
        battery_phase: Instant::now(),
        power_notice: None,
        power_notification: None,
        power_changes: 0,
        last_power_change: None,
        compact_activity: figma_ui::CompactActivity::default(),
        last_policy_check: Instant::now(),
        page: if settings { 0 } else { 1 },
        hits: vec![],
        input: HWND::default(),
        event_inputs: [HWND::default(); 4],
        input_layout: None,
        calendar: calendar::Calendar::default(),
        palette: motion::Palette::default(),
        lyric_glow_phase: 0.,
        lyric_glow_seed: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos() as u64),
        lyric_key: None,
        overlay: None,
        expansion_velocity: 0.,
        frame_tick: Instant::now(),
        offscreen,
        minute: Local::now().timestamp() / 60,
        list_offset: 0,
        expansion: 0.,
        peek: 0.,
        peek_velocity: 0.,
        pointer: (-1, -1),
        focused: None,
        marquee_text: String::new(),
        marquee_tick: Instant::now(),
        title_text: String::new(),
        title_tick: Instant::now(),
        input_brush: HBRUSH::default(),
        task_input_brush: HBRUSH::default(),
        input_font: HFONT::default(),
        input_proc: None,
        scrubbing: None,
        task_priority: 0,
        sidebar_scroll: 0.,
        sidebar_target_scroll: 0.,
        shelf_selected: 0,
        priority_open: false,
        preview_worker: None,
        preview_notify: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        pressed: false,
        feedback: None,
        water_fill: -1.,
        shelf_detail: false,
    });
    let raw = Box::into_raw(w);
    trace("create: hwnd");
    let hwnd = CreateWindowExW(
        if settings {
            WINDOW_EX_STYLE(0)
        } else {
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED
        },
        w!("Horizon.Native"),
        if settings {
            w!("Horizon Settings")
        } else {
            w!("Horizon")
        },
        if settings {
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX
        } else {
            WS_POPUP | WS_CLIPCHILDREN
        },
        -20000,
        -20000,
        710,
        550,
        None,
        None,
        Some(instance.into()),
        Some(raw.cast()),
    )?;
    trace("create: hwnd ready");
    let w = &mut *raw;
    if !settings {
        w.preview_notify
            .store(hwnd.0 as usize, std::sync::atomic::Ordering::Relaxed);
    }
    if !settings {
        use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
        use windows::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DwmSetWindowAttribute};
        use windows::Win32::UI::Controls::MARGINS;
        // Let DWM composite fractional edge coverage from the premultiplied
        // Direct2D surface. The interior remains opaque, including native edits.
        w.alpha_shell = DwmExtendFrameIntoClientArea(
            hwnd,
            &MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            },
        )
        .is_ok();
        // Windows 11: disable the system border before the first show. Older
        // Windows versions retain the non-client message handling above.
        let no_border = COLORREF(0xfffffffe);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &no_border as *const _ as *const _,
            std::mem::size_of::<COLORREF>() as u32,
        );
    }
    if !settings && !offscreen {
        w.power_notification = power::Notification::register(hwnd);
        unsafe {
            DragAcceptFiles(hwnd, BOOL(1));
        }
    }
    if settings {
        w.shared.borrow_mut().settings_window = hwnd;
        let dark = 1i32;
        let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(20),
            &dark as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );
    }
    if !settings {
        trace("create: edit");
        w.input = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            w!(""),
            input::style(),
            84,
            268,
            280,
            29,
            Some(hwnd),
            None,
            Some(instance.into()),
            None,
        )?;
        trace("create: edit style");
        w.input_brush = CreateSolidBrush(input::FIELD_COLORREF);
        w.task_input_brush = CreateSolidBrush(input::TASK_FIELD_COLORREF);
        let mut font = LOGFONTW {
            lfHeight: -14,
            lfWeight: 500,
            ..Default::default()
        };
        for (a, b) in font
            .lfFaceName
            .iter_mut()
            .zip("Plus Jakarta Sans".encode_utf16())
        {
            *a = b;
        }
        w.input_font = CreateFontIndirectW(&font);
        let _ = SendMessageW(
            w.input,
            WM_SETFONT,
            Some(WPARAM(w.input_font.0 as usize)),
            Some(LPARAM(1)),
        );
        trace("create: subclass");
        let old = SetWindowLongPtrW(w.input, GWLP_WNDPROC, edit_proc as *const () as isize);
        w.input_proc = std::mem::transmute::<isize, WNDPROC>(old);
        SetTimer(Some(hwnd), 1, 250, None);
        if !offscreen {
            NOTCH_HWND.store(hwnd.0 as usize, std::sync::atomic::Ordering::Relaxed);
            if let Ok(hook) = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) {
                MOUSE_HOOK.store(hook.0 as usize, std::sync::atomic::Ordering::Relaxed);
            }
            let _ = RegisterHotKey(Some(hwnd), 1, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, 0x48);
        }
        create_event_inputs(hwnd, w)?;
        w.shared.borrow_mut().notch = hwnd;
        if let Some(m) = &w.shared.borrow().media {
            m.notify
                .store(hwnd.0 as usize, std::sync::atomic::Ordering::Relaxed);
        }
    }
    if !settings && !offscreen {
        // Resolve visibility before the first presentation, rather than showing
        // Compact and correcting it on the first policy timer.
        w.mode = desired_mode(hwnd, w);
        w.idle_transition = None;
    }
    apply_frame(hwnd, w);
    prepare_first_frame(hwnd, w)?;
    if settings || w.mode != shell_policy::Mode::Hidden {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
    let _ = UpdateWindow(hwnd);
    Ok(hwnd)
}
unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if let Ok(parent) = GetParent(hwnd) {
        let ptr = GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut Window;
        if !ptr.is_null() {
            if (*ptr).alpha_shell {
                if msg == WM_ERASEBKGND {
                    return LRESULT(1);
                }
                if msg == WM_PAINT && input::paint_opaque(hwnd, (*ptr).input_proc) {
                    return LRESULT(0);
                }
            }
            if msg == WM_LBUTTONDOWN && (*ptr).page == 4 {
                (*ptr).priority_open = false;
                let _ = InvalidateRect(Some(parent), None, false);
            }
            if msg == WM_SETFOCUS || msg == WM_KILLFOCUS {
                input::focus_changed(hwnd);
            }
            if msg == WM_KEYDOWN && wp.0 == 9 && (*ptr).page == 4 {
                (*ptr).focused = (*ptr)
                    .hits
                    .iter()
                    .position(|hit| matches!(hit.action, Action::TaskPriorityMenu));
                let _ = SetFocus(Some(parent));
                let _ = InvalidateRect(Some(parent), None, false);
                return LRESULT(0);
            }
            if msg == WM_KEYDOWN && wp.0 == 9 && (*ptr).page == 2 {
                let fields = (*ptr).event_inputs;
                if let Some(index) = fields.iter().position(|h| *h == hwnd) {
                    let next = if GetKeyState(VK_SHIFT.0 as i32) < 0 {
                        (index + 3) % 4
                    } else {
                        (index + 1) % 4
                    };
                    let _ = SetFocus(Some(fields[next]));
                    return LRESULT(0);
                }
            }
            if msg == WM_KEYDOWN && wp.0 == 13 {
                let _ = PostMessageW(Some(parent), WM_APP + 7, WPARAM(0), LPARAM(0));
                return LRESULT(0);
            }
            let result = CallWindowProcW((*ptr).input_proc, hwnd, msg, wp, lp);
            if (*ptr).alpha_shell {
                input::refresh_opaque(hwnd, msg);
            }
            return result;
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
fn trace(message: &str) {
    if std::env::args().any(|a| a == "--trace") {
        eprintln!("{message}");
    }
}
fn main() {
    if let Err(error) = run() {
        if std::env::args().any(|a| a == "--offscreen") {
            eprintln!("{error}");
            std::process::exit(1);
        }
        unsafe {
            let _ = MessageBoxW(
                None,
                &HSTRING::from(error.to_string()),
                w!("Horizon"),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}
struct Instance(HANDLE);
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn run() -> Result<()> {
    if std::env::args().any(|arg| arg == "--inspect-running-media") {
        unsafe {
            let hwnd = FindWindowW(w!("Horizon.Native"), w!("Horizon"))?;
            PostMessageW(Some(hwnd), WM_APP + 12, WPARAM(0), LPARAM(0))?;
        }
        return Ok(());
    }
    if std::env::args().any(|a| a == "--media-probe") {
        return probes::media_diagnostics();
    }
    let _buffered_paint = input::BufferedPainting::new()?;
    let _instance = if !std::env::args().any(|a| a == "--offscreen") {
        unsafe {
            let handle = windows::Win32::System::Threading::CreateMutexW(
                None,
                false,
                w!("Local\\Horizon.Native.SingleInstance"),
            )?;
            let guard = Instance(handle);
            if GetLastError() == ERROR_ALREADY_EXISTS
                || FindWindowW(w!("Horizon.Native"), w!("Horizon")).is_ok()
            {
                return Err(Error::new(
                    E_FAIL,
                    "Horizon masih berjalan. Tutup versi sebelumnya lewat Settings > Tentang > Keluar, lalu buka versi ini.",
                ));
            }
            Some(guard)
        }
    } else {
        None
    };
    unsafe {
        windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_SINGLETHREADED,
        )?;
        trace("create: module");
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: w!("Horizon.Native"),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(Error::from_thread());
        }
        let offscreen = std::env::args().any(|a| a == "--offscreen");
        let data = if offscreen {
            Data::default()
        } else {
            Data::load(&Data::path())
                .map_err(|e| Error::new(E_FAIL, format!("Data tidak diubah: {e}")))?
        };
        let startup_error = if !offscreen && data.settings.startup {
            startup::apply(true).err()
        } else {
            None
        };
        let focus_left = data
            .focus_remaining
            .unwrap_or(data.settings.focus_minutes as u64 * 60);
        let media = if offscreen {
            None
        } else {
            Some(media::Media::start(data.settings.show_lyrics))
        };
        let shared = Rc::new(RefCell::new(Shared {
            alert: data.reminders.current(),
            data,
            status: startup_error
                .map(|e| format!("Startup belum diperbarui: {e}"))
                .unwrap_or_else(|| "Siap".into()),
            focus_left,
            running: None,
            focus_accounted: None,
            notch: HWND::default(),
            settings_window: HWND::default(),
            media,
            store: if offscreen {
                std::env::temp_dir().join(format!("horizon-probe-{}.json", std::process::id()))
            } else {
                Data::path()
            },
        }));
        let _font = host::PrivateFont::load()?;
        let root = create(shared.clone(), false, offscreen)?;
        if offscreen && std::env::args().any(|a| a == "--startup-probe") {
            let state = &mut *(GetWindowLongPtrW(root, GWLP_USERDATA) as *mut Window);
            probes::startup_checks(root, state)?;
            return Ok(());
        }
        if offscreen && std::env::args().any(|a| a == "--render") {
            std::fs::create_dir_all("artifacts/previews")
                .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            let state = &mut *(GetWindowLongPtrW(root, GWLP_USERDATA) as *mut Window);
            shared.borrow_mut().media = Some(media::Media::fixture(media::Track {
                title: "Night Drive".into(),
                cover: Some(std::sync::Arc::new(
                    include_bytes!("../assets/fixtures/cover.png").to_vec(),
                )),
                artist: "Aster · Midnight Sessions".into(),
                position: 108.,
                duration: 251.,
                playing: true,
                palette: artwork::palette(include_bytes!("../assets/fixtures/cover.png")),
                lyrics: std::sync::Arc::new(lyrics::parse(
                    "[00:00]Tarik napas, pelan saja\n[01:00]Esok kita mulai sekali lagi\n[02:00]Masih ada waktu untuk bermimpi",
                )),
                ..Default::default()
            }));
            state.expanded = true;
            state.expansion = 1.;
            state.peek = 1.;
            shared.borrow_mut().data.water = 1250;
            for page in 0..6 {
                state.page = page;
                let settings = state.shared.borrow().data.settings.clone();
                state.canvas = Some(Canvas::image(settings.width as u32, host_height() as u32)?);
                state.paint()?;
                state
                    .canvas
                    .as_ref()
                    .ok_or_else(|| Error::new(E_FAIL, "No canvas"))?
                    .save_bmp(std::path::Path::new(&format!(
                        "artifacts/previews/page-{page}.bmp"
                    )))?;
            }
            if let Some(media) = &shared.borrow().media
                && let Ok(mut track) = media.snapshot.lock()
            {
                track.playing = false;
            }
            state.page = 0;
            state.canvas = Some(Canvas::image(
                shared.borrow().data.settings.width as u32,
                host_height() as u32,
            )?);
            state.paint()?;
            state
                .canvas
                .as_ref()
                .ok_or_else(|| Error::new(E_FAIL, "No paused canvas"))?
                .save_bmp(std::path::Path::new("artifacts/previews/paused.bmp"))?;
            let dialog = create(shared.clone(), true, true)?;
            let state = &mut *(GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Window);
            for page in 0..7 {
                state.page = page;
                state.canvas = Some(Canvas::image(710, 550)?);
                state.paint()?;
                state
                    .canvas
                    .as_ref()
                    .ok_or_else(|| Error::new(E_FAIL, "No canvas"))?
                    .save_bmp(std::path::Path::new(&format!(
                        "artifacts/previews/settings-{page}.bmp"
                    )))?;
            }
            return Ok(());
        }
        if offscreen {
            SetTimer(Some(root), 3, 15000, None);
            let raw = GetWindowLongPtrW(root, GWLP_USERDATA) as *mut Window;
            (*raw).canvas = Some(Canvas::new(
                root,
                (*raw).shared.borrow().data.settings.width as u32,
                host_height() as u32,
                (*raw).alpha_shell,
            )?);
            (*raw).paint()?;
            if std::env::args().any(|a| a == "--settings") {
                let dialog = create(shared.clone(), true, true)?;
                let raw = GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Window;
                for p in 0..7 {
                    (*raw).page = p;
                    if (*raw).canvas.is_none() {
                        (*raw).canvas = Some(Canvas::new(dialog, 710, 550, false)?);
                    }
                    (*raw).paint()?;
                }
            }
        }
        if offscreen && std::env::args().any(|a| a == "--verify") {
            let dialog = create(shared.clone(), true, true)?;
            let settings = &mut *(GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Window);
            let mut wired = std::collections::BTreeSet::new();
            for page in 0..7 {
                settings.page = page;
                settings.canvas = Some(Canvas::image(710, 550)?);
                settings.paint()?;
                for h in &settings.hits {
                    if let Action::Setting(key, _) = h.action {
                        wired.insert(key);
                    }
                }
            }
            if wired != (0..17).collect() {
                return Err(Error::new(E_FAIL, "Some settings controls are missing"));
            }
            // Exercise a width change below the configured maximum, including
            // when the current default already starts at that maximum.
            shared.borrow_mut().data.settings.width = 591;
            let before = shared.borrow().data.settings.width;
            for key in 0..17 {
                action(dialog, settings, Action::Setting(key, 1));
            }
            let store = shared.borrow().store.clone();
            let loaded = Data::load(&store).map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            if loaded.settings.width != before + 1 {
                return Err(Error::new(E_FAIL, "Settings did not round-trip"));
            }
            let root_state = &mut *(GetWindowLongPtrW(root, GWLP_USERDATA) as *mut Window);
            apply_frame(root, root_state);
            let mut client = RECT::default();
            GetClientRect(root, &mut client)?;
            if client.right - client.left != shared.borrow().data.settings.width {
                return Err(Error::new(E_FAIL, "Width was not applied to HWND"));
            }
            let mut alpha = 0;
            GetLayeredWindowAttributes(root, None, Some(&mut alpha), None)?;
            if alpha != shared.borrow().data.settings.opacity {
                return Err(Error::new(E_FAIL, "Opacity was not applied to HWND"));
            }
            SetWindowTextW(root_state.input, w!("Tugas pengujian @23:59"))?;
            action(root, root_state, Action::AddTask);
            if shared.borrow().data.tasks.len() != 1 {
                return Err(Error::new(E_FAIL, "Task add failed"));
            }
            action(root, root_state, Action::ToggleTask(0));
            if !shared.borrow().data.tasks[0].done {
                return Err(Error::new(E_FAIL, "Task completion failed"));
            }
            action(root, root_state, Action::DeleteTask(0));
            if !shared.borrow().data.tasks.is_empty() {
                return Err(Error::new(E_FAIL, "Task deletion failed"));
            }
            action(root, root_state, Action::Focus);
            if shared.borrow().running.is_none() {
                return Err(Error::new(E_FAIL, "Focus start failed"));
            }
            action(root, root_state, Action::Reset);
            if shared.borrow().running.is_some() {
                return Err(Error::new(E_FAIL, "Focus reset failed"));
            }
            std::fs::create_dir_all("artifacts").map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            probes::calendar_checks(root, root_state)?;
            polish_checks::run(root, root_state)?;
            std::fs::write("artifacts/ui-checks.json",r#"{"settings_roundtrip":true,"all_17_setting_controls_rendered":true,"native_width_and_opacity":true,"task_create_complete_delete":true,"focus_start_reset":true,"calendar_month_grid_and_form_roundtrip":true,"desktop_input_used":false,"startup_modified":false}"#).map_err(|e|Error::new(E_FAIL,e.to_string()))?;
        }
        let mut motion_probe = if offscreen && std::env::args().any(|a| a == "--motion-probe") {
            let w = &mut *(GetWindowLongPtrW(root, GWLP_USERDATA) as *mut Window);
            Some(probes::MotionProbe::start(root, w)?)
        } else {
            None
        };
        let clock = motion::FrameClock::new()?;
        let mut armed = None::<Instant>;
        let mut msg = MSG::default();
        loop {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return Ok(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let state = &mut *(GetWindowLongPtrW(root, GWLP_USERDATA) as *mut Window);
            if !state.offscreen
                && state.last_policy_check.elapsed() >= std::time::Duration::from_millis(250)
            {
                state.last_policy_check = Instant::now();
                update_policy(root, state);
            }
            if let Some(interval) = frame_interval(state) {
                if armed.is_none() {
                    state.frame_tick = Instant::now();
                    let deadline = Instant::now() + interval;
                    clock.arm(interval)?;
                    armed = Some(deadline);
                }
                let result = MsgWaitForMultipleObjectsEx(
                    Some(&[clock.handle()]),
                    u32::MAX,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                );
                if result == WAIT_FAILED {
                    return Err(Error::from_thread());
                }
                if result == WAIT_OBJECT_0 {
                    let previous = armed.take().unwrap_or_else(Instant::now);
                    frame_step(root, state);
                    if let Some(probe) = &mut motion_probe
                        && probe.frame(root, state)?
                    {
                        PostQuitMessage(0);
                    }
                    if let Some(next) = frame_interval(state) {
                        let now = Instant::now();
                        let deadline =
                            (previous + next).max(now + std::time::Duration::from_micros(100));
                        clock.arm(deadline.duration_since(now))?;
                        armed = Some(deadline);
                    }
                }
            } else {
                if armed.take().is_some() {
                    clock.cancel();
                }
                WaitMessage()?;
            }
        }
    }
}

#[cfg(test)]
mod settings_regressions {
    use super::*;
    fn app() -> Shared {
        Shared {
            data: Data::default(),
            focus_left: 1500,
            ..Default::default()
        }
    }
    #[test]
    fn every_numeric_setting_changes_and_persists() {
        let mut state = app();
        state.data.settings.width = 591;
        for (key, delta) in [
            (0, 20),
            (1, -26),
            (2, -4),
            (3, 1),
            (6, 5),
            (7, 250),
            (8, 50),
            (9, -59),
            (12, 1),
            (13, -1),
            (14, 3),
        ] {
            update_setting(&mut state, key, delta, |_| Ok(())).expect("setting");
        }
        let roundtrip: Data =
            serde_json::from_slice(&serde_json::to_vec(&state.data).expect("serialize"))
                .expect("deserialize");
        let s = roundtrip.settings;
        assert_eq!((s.width, s.opacity, s.radius, s.theme), (611, 229, 17, 1));
        assert_eq!(
            (s.focus_minutes, s.water_goal, s.cup, s.water_interval),
            (30, 2250, 300, 1)
        );
        assert_eq!(
            (s.water_start_hour, s.water_end_hour, s.event_lead_minutes),
            (9, 21, 8)
        );
        assert_eq!(state.focus_left, 1800);
    }
    #[test]
    fn focus_duration_preserves_elapsed_and_updates_timer() {
        let mut state = app();
        state.running = Instant::now().checked_sub(std::time::Duration::from_secs(120));
        update_setting(&mut state, 6, 5, |_| Ok(())).expect("duration");
        assert!((1679..=1680).contains(&remaining(&state)));
        assert!(state.running.is_some());
        state.running = None;
        let before = remaining(&state);
        update_setting(&mut state, 6, -10, |_| Ok(())).expect("duration");
        assert!(
            (before.saturating_sub(601)..=before.saturating_sub(599)).contains(&remaining(&state))
        );
    }
    #[test]
    fn failed_startup_keeps_previous_checkbox_state() {
        let mut state = app();
        let result = update_setting(&mut state, 5, 1, |_| {
            Err(Error::new(E_ACCESSDENIED, "fixture failure"))
        });
        assert!(result.is_err());
        assert!(!state.data.settings.startup);
    }
    #[test]
    fn toggles_and_overnight_schedule_work() {
        let mut state = app();
        for key in [4, 5, 10, 11, 15, 16] {
            update_setting(&mut state, key, 1, |_| Ok(())).expect("toggle");
        }
        assert!(
            !state.data.settings.motion
                && state.data.settings.startup
                && !state.data.settings.sound
                && !state.data.settings.water_enabled
                && !state.data.settings.album_colors
                && !state.data.settings.show_lyrics
        );
        assert!(active_hours(23, 22, 6) && active_hours(5, 22, 6));
        assert!(!active_hours(12, 22, 6));
        assert!(active_hours(12, 8, 8));
    }
    #[test]
    fn invalid_setting_is_rejected() {
        let mut state = app();
        assert!(update_setting(&mut state, 99, 1, |_| Ok(())).is_err());
        assert_eq!(state.data.settings.width, 680);
    }
    #[test]
    fn focus_statistics_exclude_pauses_and_count_completion_once() {
        let mut state = app();
        state.focus_left = 120;
        state.running = Some(Instant::now() - std::time::Duration::from_secs(180));
        record_focus(&mut state);
        let day = Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(state.data.focus_history[&day], 120);
        record_focus(&mut state);
        assert_eq!(state.data.focus_history[&day], 120);
        state.running = None;
        record_focus(&mut state);
        state.focus_left = 1500;
        state.running = Some(Instant::now() - std::time::Duration::from_secs(10));
        record_focus(&mut state);
        assert_eq!(state.data.focus_history[&day], 130);
    }
    #[test]
    fn figma_migration_preserves_legacy_opacity_and_only_runs_once() {
        let mut s: Settings =
            serde_json::from_str(r#"{"width":540,"opacity":242,"radius":28}"#).unwrap();
        s.normalize();
        assert_eq!(
            (s.width, s.opacity, s.radius, s.design_revision),
            (680, 242, 28, 2)
        );
        s.width = 600;
        s.normalize();
        assert_eq!(s.width, 600);
    }
}
