#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unsafe_op_in_unsafe_fn)]
mod artwork;
mod assistant;
mod assistant_image;
mod assistant_ui;
mod assistant_voice;
mod audio;
mod calendar;
mod cover_fallback;
mod dpi;
mod figma_ui;
mod focus;
mod focus_ui;
mod host;
mod icons;
use host::*;
mod i18n;
mod input;
mod layout;
mod lyrics;
mod lyrics_disk;
mod mascot;
#[cfg(test)]
mod mascot_rig;
mod media;
mod model;
mod monitor;
mod motion;
mod polish_checks;
mod power;
mod probes;
mod reminders;
mod render;
mod settings_icons;
mod settings_ui;
mod shelf;
mod shelf_ui;
mod shell_policy;
mod spotify;
mod spotify_ui;
use spotify_ui::{spotify_search_action, spotify_search_update};
mod startup;
mod taskbar;
mod tasks_ui;
mod ui;
use chrono::{Local, TimeZone};
use model::*;
use render::Canvas;
use std::{cell::RefCell, rc::Rc, time::Instant};
static APP_STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
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
            if !ptr.is_null() && (*ptr).expanded && !(*ptr).assistant.choosing_file {
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
    AiSend,
    AiVoice,
    AiHistory,
    AiOpenChat(usize),
    AiDeleteChat(usize),
    AiAttach,
    AiFromShelf(usize),
    AiDetach(usize),
    AiScrollAttachments(i32),
    AiClear,
    AiCopy,
    AiAddTask,
    AiSaveKey,
    AiDeleteKey,
    AiProvider(usize),
    Page(usize),
    ToggleMonitorCompact,
    OpenTaskManager,
    Setting(usize, i32),
    Settings,
    #[allow(dead_code)]
    SettingsAt(usize),
    TestSound,
    Focus,
    Reset,
    Media(u8),
    Seek,
    SpotifyVolume,
    SpotifyMute,
    SpotifySearchPanel,
    SpotifySearchSubmit,
    SpotifySearchWeb,
    SpotifySearchSetup,
    SpotifySearchPlay(usize),
    SpotifySearchLink(usize),
    SpotifySearchScroll(i32),
    SpotifySearchDisconnect,
    SpotifySearchCopyRedirect,
    SpotifySearchHelp,
    Favorite,
    CoverSource,
    Drink,
    DrinkAmount(u32),
    ResetWater,
    SetFocusMinutes(u32),
    SetBreakMinutes(u32),
    FocusTaskMenu,
    FocusTask(Option<usize>),
    FocusTaskScroll(i32),
    StartFocusBreak,
    ExtendFocus,
    FinishFocusTask,
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
    ToggleShelfPreview(usize),
    ShelfList,
    OpenFileFolder(usize),
    CopyFilePath(usize),
    DismissAlert,
    OpenAlert,
    PetMascot,
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
    spotify_volume: Option<audio::SpotifyVolume>,
    store: std::path::PathBuf,
    alert: Option<Alert>,
}
struct Window {
    shared: Rc<RefCell<Shared>>,
    canvas: Option<Canvas>,
    alpha_shell: bool,
    dpi: dpi::Scale,
    settings: bool,
    expanded: bool,
    hovered: bool,
    hover_tick: Option<Instant>,
    mode: shell_policy::Mode,
    idle_transition: Option<shell_policy::Transition>,
    island_style: motion::Tween,
    compact_activity_size: motion::Tween,
    compact_activity_progress: f32,
    entrance: Option<Instant>,
    startup_registration_pending: bool,
    first_frame_ms: f64,
    cover_render_error: RefCell<String>,
    battery: power::Battery,
    battery_checked: Instant,
    power_notice: Option<power::Notice>,
    power_notification: Option<power::Notification>,
    taskbar_watch: Option<taskbar::ForegroundWatch>,
    power_changes: u64,
    last_power_change: Option<power::ConnectionChange>,
    compact_activity: figma_ui::CompactActivity,
    companion: mascot::Companion,
    companion_pointer: Option<(i32, i32)>,
    last_policy_check: Instant,
    page: usize,
    hits: Vec<Hit>,
    input: HWND,
    ai_input: HWND,
    ai_model_input: HWND,
    ai_url_input: HWND,
    ai_input_visible: bool,
    spotify_search: spotify::Search,
    spotify_search_reveal: motion::Tween,
    assistant: assistant::Assistant,
    monitor: monitor::Monitor,
    event_inputs: [HWND; 4],
    input_layout: Option<[i32; 9]>,
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
    scrubbing: Option<Scrub>,
    volume_drag: Option<(i32, i32)>,
    task_priority: u8,
    focus_task_menu: bool,
    focus_task_offset: usize,
    sidebar_scroll: f32,
    sidebar_target_scroll: f32,
    shelf_selected: usize,
    shelf_preview_open: bool,
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

#[derive(Clone, Copy)]
struct Scrub {
    x: i32,
    width: i32,
    generation: u64,
}
impl Scrub {
    fn fraction(self, mouse_x: i32) -> f64 {
        ((mouse_x - self.x) as f64 / self.width as f64).clamp(0., 1.)
    }
}
fn start_scrub(w: &Window, x: i32, width: i32) -> Option<Scrub> {
    let state = w.shared.borrow();
    let track = state.media.as_ref()?.snapshot.lock().ok()?;
    (width > 0 && track.can_seek).then_some(Scrub {
        x,
        width,
        generation: track.generation,
    })
}
fn spotify_volume_to(w: &Window, mouse_x: i32, hit_x: i32, hit_w: i32) {
    if let Some(level) = audio::slider_level(mouse_x, hit_x, hit_w)
        && let Some(volume) = &w.shared.borrow().spotify_volume
    {
        volume.set_level(level);
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
const SIDEBAR_TOP: f32 = 10.;
const SIDEBAR_SIZE: f32 = 32.;
const SIDEBAR_GAP: f32 = 4.;
const SIDEBAR_STEP: f32 = SIDEBAR_SIZE + SIDEBAR_GAP;
const SIDEBAR_SETTINGS_GAP: f32 = 15.;
fn expanded_radius(settings: &Settings) -> f32 {
    (EXPANDED_RADIUS + (settings.radius - 21) as f32).clamp(14., 48.)
}
const AGENDA_PAGE_SIZE: usize = 2;
fn panel_height(_s: &Settings) -> i32 {
    EXPANDED_HEIGHT
}
fn host_height() -> i32 {
    EXPANDED_HEIGHT
}
fn sidebar_settings_y() -> f32 {
    host_height() as f32 - SIDEBAR_SIZE - 10.
}
fn sidebar_bottom() -> f32 {
    sidebar_settings_y() - SIDEBAR_SETTINGS_GAP
}
fn sidebar_max_scroll(page_count: usize) -> f32 {
    let content = page_count as f32 * SIDEBAR_STEP - SIDEBAR_GAP;
    (SIDEBAR_TOP + content - sidebar_bottom()).max(0.)
}
fn shell_buffer_height(w: &Window) -> i32 {
    if w.mode == shell_policy::Mode::Compact
        && !w.expanded
        && w.expansion == 0.
        && w.idle_transition.is_none()
    {
        // Keep the same one-pixel guard as the antialiased window region.
        layout::COMPACT_BUFFER_HEIGHT as i32
    } else {
        host_height()
    }
}
fn record_focus(s: &mut Shared) {
    if s.data.focus.phase == focus::Phase::Break {
        return;
    }
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
    {
        let state = w.shared.borrow();
        w.island_style.retarget(
            f32::from(state.data.settings.dynamic_island),
            motion::SHELL,
            state.data.settings.motion,
            Instant::now(),
        );
    }
    sync_inputs(hwnd, w);
    let s = &w.shared.borrow().data.settings;
    let (mut width, mut height) = if w.settings {
        (710, 550)
    } else {
        (w.client_width(s), shell_buffer_height(w))
    };
    width = w.dpi.px(width);
    height = w.dpi.px(height);
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
        let now = Instant::now();
        let bar_height = if w.island_style.sample(now) > 0. {
            taskbar::primary().map(|(_, height)| height)
        } else {
            None
        };
        shell_top(w, bar_height, now)
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
    }
    {
        let mut alpha = 0;
        let _ = GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), None);
        let opacity = (surface_opacity(w, s) as f32
            * if w.settings {
                1.
            } else {
                motion::entrance_progress(w.entrance, Instant::now())
            })
        .round() as u8;
        if alpha != opacity {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), opacity, LWA_ALPHA);
        }
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
}
fn shell_top(w: &Window, taskbar_height: Option<i32>, now: Instant) -> i32 {
    let height = w.idle_geometry().height;
    let compact = ((101. - height) / (101. - layout::COMPACT_HEIGHT)).clamp(0., 1.);
    let taskbar_height = taskbar_height.map(|height| w.dpi.dip(height));
    let y = layout::island_gap(taskbar_height, compact * (1. - w.expansion))
        * w.island_style.sample(now)
        - 8. * (1. - motion::entrance_progress(w.entrance, now));
    w.dpi.physical(y).round() as i32
}
fn surface_opacity(w: &Window, s: &Settings) -> u8 {
    if !w.settings && w.expanded && w.page == monitor::PAGE {
        return s.opacity_for(MONITOR_OPACITY);
    }
    if !w.settings && w.expanded && w.page == assistant::PAGE {
        return s.opacity_for(ASSISTANT_OPACITY);
    }
    s.opacity_for(if w.settings {
        8
    } else if w.expanded {
        w.page.min(5)
    } else if w.mode == shell_policy::Mode::Peek {
        7
    } else {
        6
    })
}
fn shell_outline(w: &Window) -> render::ShellOutline {
    let s = &w.shared.borrow().data.settings;
    let p = w.expansion;
    let full_w = w.client_width(s) as f32;
    let full_h = if w.page <= 2 {
        host_height()
    } else {
        panel_height(s)
    } as f32;
    let idle = w.idle_geometry();
    let medium =
        ((idle.height - layout::COMPACT_HEIGHT) / (101. - layout::COMPACT_HEIGHT)).clamp(0., 1.);
    let radius = (37. - 5. * medium) * (1. - p) + expanded_radius(s) * p;
    let width = idle.width + (full_w - idle.width) * p;
    let height = idle.height + (full_h - idle.height) * p;
    let offset = (full_w - width) / 2.;
    // Compact's exported shoulder is 9px; peek has straight sides. Expansion
    // alone cannot distinguish those two contours, so include the peek phase.
    let shape = idle.shape + (1. - idle.shape) * p;
    render::ShellOutline {
        floating: w.island_style.sample(Instant::now()),
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
    let guard = if w.alpha_shell {
        w.dpi.logical(1.5)
    } else {
        0.
    };
    let raw_points: Vec<POINT> = layout::contour_styled(
        outline.width + guard * 2.,
        outline.height + guard,
        outline.shape,
        outline.radius + guard,
        outline.floating,
    )
    .into_iter()
    .map(|(x, y)| POINT {
        x: w.dpi.physical(x + outline.offset - guard).round() as i32,
        y: w.dpi.physical(y).round() as i32,
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
    if w.show_monitor_badges() {
        let badges = monitor::Badges::new(outline);
        for (x, y, width, height) in [badges.cpu, badges.memory] {
            let left = w.dpi.physical(x).floor() as i32;
            let top = w.dpi.physical(y).floor() as i32;
            let right = w.dpi.physical(x + width).ceil() as i32;
            let bottom = w.dpi.physical(y + height).ceil() as i32;
            let badge = if w.alpha_shell {
                // Let Direct2D define the curved edge, with room for fractional coverage.
                CreateRectRgn(left - 1, top - 1, right + 1, bottom + 1)
            } else {
                let diameter = w.dpi.physical(height).round() as i32;
                CreateRoundRectRgn(left, top, right, bottom, diameter, diameter)
            };
            if !badge.is_invalid() {
                CombineRgn(Some(rgn), Some(rgn), Some(badge), RGN_OR);
                let _ = DeleteObject(badge.into());
            }
        }
    }
    // The animation owns repainting; avoid a separate default-frame redraw
    // between exposing the new region and presenting its Direct2D frame.
    if SetWindowRgn(hwnd, Some(rgn), false) == 0 {
        let _ = DeleteObject(rgn.into());
    }
}
impl Window {
    fn client_width(&self, s: &Settings) -> i32 {
        if self.mode == shell_policy::Mode::Compact && !self.expanded && s.monitor_in_compact {
            s.width.max(monitor::COMPACT_CLIENT_WIDTH)
        } else {
            s.width
        }
    }

    fn show_monitor_badges(&self) -> bool {
        !self.settings
            && self.mode == shell_policy::Mode::Compact
            && !self.expanded
            && self.expansion <= 0.001
            && self.idle_transition.is_none()
            && self.shared.borrow().data.settings.monitor_in_compact
    }

    fn mouse_point(&self, position: LPARAM) -> (i32, i32) {
        let x = position.0 as i16 as i32;
        let y = (position.0 >> 16) as i16 as i32;
        self.dpi.point(x, y)
    }

    fn compact_width(&self, floating: f32) -> f32 {
        layout::compact_activity_width(floating, self.compact_activity_progress)
    }

    fn idle_geometry(&self) -> shell_policy::Size {
        let mut size = self.base_idle_geometry();
        let compact =
            ((591. - size.width) / 281.).clamp(0., 1.) * ((size.width - 54.) / 256.).clamp(0., 1.);
        size.width -=
            (310. - self.compact_width(self.island_style.sample(Instant::now()))) * compact;
        size.height -= (layout::COMPACT_HEIGHT
            - layout::compact_height(self.island_style.sample(Instant::now())))
            * compact;
        size
    }
    fn base_idle_geometry(&self) -> shell_policy::Size {
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
    let from = w.base_idle_geometry();
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
    if matches!(
        next,
        shell_policy::Mode::Hidden | shell_policy::Mode::Indicator
    ) {
        w.companion.hide();
    }
}
fn update_battery(w: &mut Window, next: power::Battery) -> bool {
    w.battery_checked = Instant::now();
    if next == w.battery {
        return false;
    }
    if let Some(change) = w.battery.connection_change(next) {
        w.power_notice = Some(power::Notice::new(change, next.percent, Instant::now()));
        w.power_changes += 1;
        w.last_power_change = Some(change);
        w.overlay = None;
    }
    w.battery = next;
    true
}
fn sync_compact_activity(w: &mut Window) -> bool {
    let state = w.shared.borrow();
    let track = state
        .media
        .as_ref()
        .and_then(|media| media.snapshot.lock().ok().map(|track| track.clone()))
        .unwrap_or_default();
    w.compact_activity
        .observe_track(&track, state.running.is_some());
    w.compact_activity.observe_alert(state.alert.as_ref());
    let active = track.playing
        || state.running.is_some()
        || state.alert.is_some()
        || w.power_notice.is_some();
    w.compact_activity_size.retarget(
        f32::from(active),
        motion::SHELL,
        state.data.settings.motion,
        Instant::now(),
    );
    if !state.data.settings.motion {
        let progress = f32::from(active);
        return std::mem::replace(&mut w.compact_activity_progress, progress) != progress;
    }
    false
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
        monitor_pinned: state.data.settings.monitor_in_compact,
        hide_fullscreen: true,
        has_alert: state.alert.is_some() || w.power_notice.is_some(),
        taskbar_overlay: state.data.settings.taskbar_layout,
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
    if w.shared.borrow().data.settings.taskbar_layout && w.mode != shell_policy::Mode::Hidden {
        let bar = taskbar::primary();
        let mut bounds = RECT::default();
        let top = shell_top(w, bar.map(|(_, height)| height), Instant::now());
        if GetWindowRect(hwnd, &mut bounds).is_ok() && bounds.top != top {
            apply_frame(hwnd, w);
        }
        let active = GetForegroundWindow();
        let lost_topmost = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 == 0;
        let covered = !active.is_invalid()
            && active != hwnd
            && active != w.shared.borrow().settings_window
            && IsWindowVisible(active).as_bool()
            && taskbar::above(active, hwnd);
        if lost_topmost || covered || bar.is_some_and(|(bar, _)| taskbar::above(bar, hwnd)) {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
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
    i18n::set_language(state.data.settings.language);
    if key == 5
        && let Err(error) = startup(state.data.settings.startup)
    {
        state.data.settings = original;
        return Err(error);
    }
    if key == 6 && state.data.focus.phase == focus::Phase::Work {
        record_focus(state);
        let elapsed = (original.focus_minutes as u64 * 60).saturating_sub(remaining(state));
        state.focus_left = (state.data.settings.focus_minutes as u64 * 60).saturating_sub(elapsed);
        if state.running.is_some() {
            state.running = Some(Instant::now());
            state.focus_accounted = None;
        }
    }
    if matches!(key, 9 | 11 | 12 | 13) {
        state.data.water_due =
            Local::now().timestamp() + state.data.settings.water_interval as i64 * 60;
    }
    Ok(())
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
                | Action::ToggleShelfPreview(_)
                | Action::ShelfList
                | Action::RemoveShelfFile(_)
                | Action::ClearShelfFiles
        )
    {
        begin_transition(w, false, motion::CONTENT);
    }
    match a {
        Action::CoverSource => {
            let link = w.shared.borrow().media.as_ref().and_then(|m| {
                m.snapshot
                    .lock()
                    .ok()
                    .and_then(|t| t.cover_source_url.clone())
            });
            if let Some(link) = link {
                let _ = calendar::open_url(hwnd, &link);
            }
        }
        Action::Settings => {
            let _ = create(w.shared.clone(), true, w.offscreen);
        }
        Action::SettingsAt(page) => {
            if let Ok(dialog) = create(w.shared.clone(), true, w.offscreen) {
                let p = GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Window;
                if !p.is_null() {
                    (*p).page = page;
                    (*p).focused = None;
                    sync_inputs(dialog, &mut *p);
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
        Action::PetMascot => {
            w.companion.pet();
            return;
        }
        Action::Exit => {
            persist(w);
            let _ = PostMessageW(None, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        Action::Page(p) => {
            let order = |page| {
                if w.settings {
                    page
                } else {
                    figma_ui::sidebar_index(page)
                }
            };
            if w.page != p {
                let direction = if order(p) > order(w.page) { -1. } else { 1. };
                begin_transition(w, false, motion::PAGE);
                if let Some(overlay) = &mut w.overlay {
                    overlay.direction = direction;
                }
            }
            if w.page == 5 && p != 5 {
                w.preview_worker = None;
                if let Some(canvas) = &w.canvas {
                    canvas.release_preview();
                }
            }
            w.page = p;
            w.focus_task_menu = false;
            w.focus_task_offset = 0;
            w.focused = None;
            w.list_offset = 0;
            w.priority_open = false;
            w.shelf_detail = false;
            w.shelf_preview_open = false;
            if !w.settings {
                let max_scroll = sidebar_max_scroll(figma_ui::SIDEBAR_ICONS.len());
                let top = figma_ui::sidebar_index(p) as f32 * SIDEBAR_STEP;
                let viewport = sidebar_bottom() - SIDEBAR_TOP;
                w.sidebar_target_scroll = w
                    .sidebar_target_scroll
                    .clamp(top + SIDEBAR_SIZE - viewport, top)
                    .clamp(0., max_scroll);
                if !w.shared.borrow().data.settings.motion {
                    w.sidebar_scroll = w.sidebar_target_scroll;
                }
            }
        }
        Action::ToggleMonitorCompact => {
            let enabled = w.shared.borrow().data.settings.monitor_in_compact;
            w.shared.borrow_mut().data.settings.monitor_in_compact = !enabled;
            if !persist(w) {
                w.shared.borrow_mut().data.settings.monitor_in_compact = enabled;
            }
        }
        Action::OpenTaskManager if !w.offscreen => {
            w.monitor.notice = monitor::open_task_manager().is_err();
        }
        Action::OpenTaskManager => {}
        Action::SpotifySearchPanel => {
            if !w.settings && w.expanded && w.page == 0 {
                w.spotify_search
                    .restore(&w.shared.borrow().data.settings.spotify_client_id);
                w.set_spotify_search_open(!w.spotify_search.open);
                sync_inputs(hwnd, w);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            return;
        }
        Action::SpotifySearchSubmit
        | Action::SpotifySearchWeb
        | Action::SpotifySearchSetup
        | Action::SpotifySearchPlay(_)
        | Action::SpotifySearchLink(_)
        | Action::SpotifySearchScroll(_)
        | Action::SpotifySearchDisconnect
        | Action::SpotifySearchCopyRedirect
        | Action::SpotifySearchHelp => {
            if !w.settings && w.expanded && w.page == 0 && w.spotify_search.open {
                spotify_search_action(hwnd, w, a);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            return;
        }
        Action::AiVoice if !w.settings && w.page == assistant::PAGE && !w.offscreen => {
            if w.ai_input_visible && !w.assistant.waiting() && w.assistant.dictation.is_none() {
                match assistant_voice::start(hwnd) {
                    Ok(dictation) => {
                        w.assistant.dictation = Some(dictation);
                        w.assistant.notice = "";
                    }
                    Err(_) => w.assistant.notice = "Dikte belum bisa dimulai.",
                }
                w.assistant.scroll_to_end = true;
            }
        }
        Action::AiHistory if !w.settings && w.page == assistant::PAGE => {
            w.assistant.history_open = !w.assistant.history_open;
            w.assistant.history_offset = 0;
        }
        Action::AiOpenChat(index) if !w.settings && w.page == assistant::PAGE => {
            if w.assistant.open_chat(index) && !w.ai_input.is_invalid() {
                let _ = SetWindowTextW(w.ai_input, w!(""));
            }
        }
        Action::AiDeleteChat(index) if !w.settings && w.page == assistant::PAGE => {
            if !w.assistant.waiting()
                && w.assistant
                    .chats
                    .as_ref()
                    .is_some_and(|chats| index < chats.len())
            {
                let selected = w.assistant.selected_chat;
                w.assistant.chats.as_mut().unwrap().remove(index);
                if selected == Some(index) {
                    w.assistant.clear_chat();
                    w.assistant.history_open = true;
                    let _ = SetWindowTextW(w.ai_input, w!(""));
                } else if let Some(selected) = selected.filter(|selected| *selected > index) {
                    w.assistant.selected_chat = Some(selected - 1);
                }
                let path = w
                    .shared
                    .borrow()
                    .store
                    .with_file_name("assistant-chats.json");
                w.assistant.save_history(&path);
            }
        }
        Action::AiSend if !w.settings && w.page == assistant::PAGE => {
            if w.offscreen
                || w.assistant.waiting()
                || w.assistant.dictation.is_some()
                || w.ai_input.is_invalid()
            {
                return;
            }
            let prompt = text_input(w.ai_input);
            let prompt = prompt.trim();
            if prompt.is_empty() && w.assistant.attachment_name().is_none() {
                return;
            }
            w.assistant.history_open = false;
            let prompt = if prompt.is_empty() {
                i18n::tr("Jelaskan lampiran ini.")
            } else {
                prompt
            };
            let settings = w.shared.borrow().data.settings.clone();
            match w.assistant.start(prompt.to_owned(), &settings, hwnd) {
                Ok(()) => {
                    let _ = SetWindowTextW(w.ai_input, w!(""));
                }
                Err(message) => w.assistant.notice = message,
            }
        }
        Action::AiAttach if !w.settings && w.page == assistant::PAGE => {
            if !w.offscreen && !w.assistant.waiting() {
                w.assistant.choosing_file = true;
                let files = assistant_ui::choose_files(hwnd);
                w.assistant.choosing_file = false;
                if !files.is_empty() {
                    w.assistant
                        .attach(files.into_iter().map(assistant::Attachment::File));
                }
            }
        }
        Action::AiFromShelf(index) if !w.settings && w.page == 5 && !w.assistant.waiting() => {
            let path = w
                .shared
                .borrow()
                .data
                .shelf_files
                .get(index)
                .map(|file| std::path::PathBuf::from(&file.path));
            if let Some(path) = path.filter(|path| assistant::supports_file(path)) {
                w.assistant.attach([assistant::Attachment::File(path)]);
                action(hwnd, w, Action::Page(assistant::PAGE));
                return;
            }
        }
        Action::AiDetach(index) if !w.assistant.waiting() => {
            if index < w.assistant.attachments.len() {
                w.assistant.attachments.remove(index);
                w.assistant.attachment_revision = w.assistant.attachment_revision.wrapping_add(1);
            }
        }
        Action::AiScrollAttachments(delta) => {
            w.assistant.attachment_offset = w
                .assistant
                .attachment_offset
                .saturating_add_signed(delta as isize);
        }
        Action::AiClear => {
            if !w.assistant.waiting() && w.assistant.dictation.is_none() {
                w.assistant.clear_chat();
                let _ = SetWindowTextW(w.ai_input, w!(""));
            }
        }
        Action::AiCopy => {
            if let Some(answer) = &w.assistant.latest {
                copy_to_clipboard(&answer.reply);
            }
        }
        Action::AiAddTask if !w.settings && w.page == assistant::PAGE => {
            let task = w
                .assistant
                .latest
                .as_ref()
                .and_then(|answer| answer.task.clone());
            if let Some(text) = task {
                w.shared.borrow_mut().data.tasks.push(Task {
                    text,
                    done: false,
                    due: None,
                    notified: false,
                    priority: 0,
                });
                if persist(w) {
                    w.assistant.mark_task_saved();
                    let path = w
                        .shared
                        .borrow()
                        .store
                        .with_file_name("assistant-chats.json");
                    w.assistant.save_history(&path);
                    w.assistant.notice = "Tugas ditambahkan.";
                } else {
                    w.shared.borrow_mut().data.tasks.pop();
                    w.assistant.notice = "Tugas belum tersimpan.";
                }
            }
        }
        Action::AiSaveKey | Action::AiDeleteKey
            if w.settings && w.page == assistant::SETTINGS_PAGE =>
        {
            if w.offscreen || w.ai_input.is_invalid() || w.ai_model_input.is_invalid() {
                return;
            }
            let save = a == Action::AiSaveKey;
            let previous = w.shared.borrow().data.settings.clone();
            let mut next = previous.clone();
            let key = text_input(w.ai_input);
            let result = (|| {
                if save {
                    let model = text_input(w.ai_model_input);
                    next.ai_models[next.ai_provider] = model.trim().to_owned();
                    if next.ai_provider == assistant::CUSTOM {
                        next.ai_endpoint = text_input(w.ai_url_input).trim().to_owned();
                    }
                    assistant::Connection::new(&next)?;
                }
                let target = assistant::Connection::credential(&next)
                    .ok_or("Masukkan URL dasar HTTPS yang valid.")?;
                if !save {
                    assistant::delete_key(&target)?;
                } else {
                    if !key.trim().is_empty() {
                        assistant::save_key(&target, &key)?;
                    }
                    w.shared.borrow_mut().data.settings = next;
                    if !persist(w) {
                        w.shared.borrow_mut().data.settings = previous;
                        return Err("Konfigurasi AI belum tersimpan.");
                    }
                }
                Ok(())
            })();
            match result {
                Ok(()) => {
                    let _ = SetWindowTextW(w.ai_input, w!(""));
                    w.assistant.configured = None;
                    w.assistant.notice = if save {
                        "Konfigurasi AI tersimpan."
                    } else {
                        "API key dihapus."
                    };
                    let root = w.shared.borrow().notch;
                    if !root.is_invalid() {
                        let _ =
                            PostMessageW(Some(root), assistant::READY + 1, WPARAM(0), LPARAM(0));
                    }
                }
                Err(message) => w.assistant.notice = message,
            }
        }
        Action::AiProvider(provider)
            if w.settings
                && w.page == assistant::SETTINGS_PAGE
                && provider < assistant::PROVIDERS.len() =>
        {
            let previous = w.shared.borrow().data.settings.ai_provider;
            if previous != provider {
                w.shared.borrow_mut().data.settings.ai_provider = provider;
                if !persist(w) {
                    w.shared.borrow_mut().data.settings.ai_provider = previous;
                    w.assistant.notice = "Konfigurasi AI belum tersimpan.";
                } else {
                    w.assistant.notice = "";
                    w.assistant.configured = None;
                    assistant_ui::refresh_provider(w);
                    let root = w.shared.borrow().notch;
                    if !root.is_invalid() {
                        let _ =
                            PostMessageW(Some(root), assistant::READY + 1, WPARAM(0), LPARAM(0));
                    }
                }
            }
        }
        Action::AiSend
        | Action::AiVoice
        | Action::AiHistory
        | Action::AiOpenChat(_)
        | Action::AiDeleteChat(_)
        | Action::AiAttach
        | Action::AiFromShelf(_)
        | Action::AiDetach(_)
        | Action::AiAddTask
        | Action::AiSaveKey
        | Action::AiDeleteKey
        | Action::AiProvider(_) => return,
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
            w.focus_task_menu = false;
            let mut s = w.shared.borrow_mut();
            if let Some(message) = focus::complete_if_due(&mut s) {
                focus::notify_completion(&mut s, message, w.expanded && w.page == 1);
                if s.data.settings.sound && !w.offscreen {
                    let _ = MessageBeep(MB_OK);
                }
            } else if s.running.is_some() {
                s.focus_left = remaining(&s);
                s.running = None;
                s.focus_accounted = None;
            } else if focus::finished(s.data.focus.phase) || s.focus_left == 0 {
                focus::begin(&mut s, focus::Phase::Work);
            } else {
                s.running = Some(Instant::now());
            }
        }
        Action::FocusTaskMenu => {
            if focus::can_configure(&w.shared.borrow()) {
                w.focus_task_menu = !w.focus_task_menu;
                w.focus_task_offset = 0;
                w.focused = None;
            }
        }
        Action::FocusTaskScroll(delta) => {
            if w.focus_task_menu {
                let count = w
                    .shared
                    .borrow()
                    .data
                    .tasks
                    .iter()
                    .filter(|task| !task.done)
                    .count();
                w.focus_task_offset = w
                    .focus_task_offset
                    .saturating_add_signed(delta as isize)
                    .min(count.saturating_sub(3));
                w.focused = None;
            }
        }
        Action::FocusTask(index) => {
            let mut s = w.shared.borrow_mut();
            if focus::can_configure(&s)
                && index.is_none_or(|i| s.data.tasks.get(i).is_some_and(|task| !task.done))
            {
                s.data.focus.task = index;
            }
            w.focus_task_menu = false;
            w.focused = None;
        }
        Action::SetBreakMinutes(minutes) => {
            let mut s = w.shared.borrow_mut();
            if focus::can_configure(&s) && [3, 5, 10].contains(&minutes) {
                s.data.settings.focus_break_minutes = minutes;
            }
        }
        Action::StartFocusBreak | Action::ExtendFocus => {
            let mut s = w.shared.borrow_mut();
            if s.data.focus.phase == focus::Phase::Finished {
                let phase = if a == Action::StartFocusBreak {
                    focus::Phase::Break
                } else {
                    focus::Phase::Extension
                };
                focus::begin(&mut s, phase);
            }
        }
        Action::FinishFocusTask => {
            let index = {
                let mut s = w.shared.borrow_mut();
                if s.data.focus.phase == focus::Phase::Finished {
                    s.data
                        .focus
                        .task
                        .filter(|&i| s.data.tasks.get(i).is_some_and(|task| !task.done))
                        .inspect(|&i| s.data.tasks[i].done = true)
                } else {
                    None
                }
            };
            if let Some(index) = index
                && !persist(w)
            {
                w.shared.borrow_mut().data.tasks[index].done = false;
            }
        }
        Action::Seek | Action::SpotifyVolume => return,
        Action::SpotifyMute => {
            if let Some(volume) = &w.shared.borrow().spotify_volume {
                volume.toggle_mute();
            }
        }
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
            w.compact_activity.reset_focus();
            w.focus_task_menu = false;
            let mut s = w.shared.borrow_mut();
            focus::reset(&mut s);
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
            w.compact_activity.reset_focus();
            let mut s = w.shared.borrow_mut();
            if focus::can_configure(&s) {
                s.data.settings.focus_minutes = minutes;
                focus::reset(&mut s);
            }
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
                focus::task_deleted(&mut s.data.focus, i);
            }
            drop(s);
            let count = w.shared.borrow().data.tasks.len();
            w.list_offset = w.list_offset.min(count.saturating_sub(4));
        }
        Action::RemoveShelfFile(i) => {
            w.shelf_preview_open = false;
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
            w.shelf_preview_open = false;
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
            if w.shelf_selected != i {
                w.shelf_preview_open = false;
            }
            w.shelf_selected = i;
            w.shelf_detail = w.shared.borrow().data.settings.width < 488;
            w.shared.borrow_mut().status = "Siap".into();
        }
        Action::ToggleShelfPreview(i) => {
            if w.expanded
                && w.page == 5
                && w.shared
                    .borrow()
                    .data
                    .shelf_files
                    .get(i)
                    .is_some_and(|file| shelf::supports_preview(&file.path))
            {
                w.shelf_preview_open = !w.shelf_preview_open;
            }
        }
        Action::ShelfList => {
            w.shelf_detail = false;
            w.shelf_preview_open = false;
        }
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
            let mut s = w.shared.borrow_mut();
            let configurable = focus::can_configure(&s);
            if let Some(t) = s.data.tasks.get_mut(i) {
                t.done = !t.done;
                if t.done && configurable && s.data.focus.task == Some(i) {
                    s.data.focus.task = None;
                }
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
                state.focus_accounted,
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
                state.focus_accounted = original.4;
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
                // Motion changes also wake the main shell.
                let root = w.shared.borrow().notch;
                if !root.is_invalid() {
                    let _ = PostMessageW(Some(root), WM_APP + 8, WPARAM(0), LPARAM(0));
                }
            }
            if k == 18
                && let Some(media) = &w.shared.borrow().media
            {
                media.cover_fallback_enabled.store(
                    w.shared.borrow().data.settings.cover_fallback,
                    std::sync::atomic::Ordering::Relaxed,
                );
                let _ = media.send.send(media::Command::Refresh);
            }
        }
    }
    if matches!(
        a,
        Action::Focus
            | Action::Reset
            | Action::SetFocusMinutes(_)
            | Action::SetBreakMinutes(_)
            | Action::FocusTask(_)
            | Action::StartFocusBreak
            | Action::ExtendFocus
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
                let mut bounds = RECT::default();
                if GetClientRect(hwnd, &mut bounds).is_ok() && bounds.right > 0 && bounds.bottom > 0
                {
                    w.canvas = Canvas::new(
                        hwnd,
                        bounds.right as u32,
                        bounds.bottom as u32,
                        w.alpha_shell,
                        w.dpi,
                    )
                    .ok();
                }
            }
            sample_companion_cursor(hwnd, w);
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
        WM_NCHITTEST if !w.settings => {
            let x = (lp.0 as u16) as i16 as i32;
            let y = ((lp.0 >> 16) as u16) as i16 as i32;
            if w.show_monitor_badges() {
                let mut bounds = RECT::default();
                if GetWindowRect(hwnd, &mut bounds).is_ok() {
                    let (x, y) = w.dpi.point(x - bounds.left, y - bounds.top);
                    if monitor::Badges::new(shell_outline(w)).contains(x, y) {
                        return LRESULT(HTTRANSPARENT as isize);
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_KILLFOCUS => {
            w.focused = None;
            let _ = InvalidateRect(Some(hwnd), None, false);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
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
                    if w.focused
                        .and_then(|i| w.hits.get(i))
                        .is_some_and(|hit| hit.action == Action::SpotifyVolume)
                    {
                        if let Some(volume) = &w.shared.borrow().spotify_volume {
                            let level = volume.snapshot.lock().ok().and_then(|s| s.level);
                            if let Some(level) = level {
                                volume.set_level(level + if wp.0 == 37 { -0.05 } else { 0.05 });
                            }
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
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
                    if !w.settings && w.page == 0 && w.spotify_search.open && w.expanded {
                        action(hwnd, w, Action::SpotifySearchPanel);
                    } else if w.priority_open {
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
            // The outline marks keyboard navigation, not pointer dragging.
            w.focused = None;
            w.pressed = true;
            let _ = InvalidateRect(Some(hwnd), None, false);
            let (x, y) = w.mouse_point(lp);
            w.pointer = (x, y);
            // Present the physical press before a fast release can overtake it.
            let _ = UpdateWindow(hwnd);
            if let Some(h) = w.hits.iter().find(|h| {
                x >= h.x
                    && x < h.x + h.w
                    && y >= h.y
                    && y < h.y + h.h
                    && matches!(h.action, Action::Seek | Action::SpotifyVolume)
            }) {
                if h.action == Action::SpotifyVolume {
                    let (hit_x, hit_w) = (h.x, h.w);
                    w.volume_drag = Some((hit_x, hit_w));
                    let _ = SetCapture(hwnd);
                    spotify_volume_to(w, x, hit_x, hit_w);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                w.scrubbing = start_scrub(w, h.x, h.w);
                if w.scrubbing.is_some() {
                    let _ = SetCapture(hwnd);
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let p = w.mouse_point(lp);
            if p != w.pointer {
                w.pointer = p;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            if let Some((hit_x, hit_w)) = w.volume_drag {
                spotify_volume_to(w, p.0, hit_x, hit_w);
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }
            if w.scrubbing.is_some() {
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
        spotify::READY => {
            spotify_search_update(hwnd, w);
            LRESULT(0)
        }
        spotify::EDIT_ACTION => {
            action(
                hwnd,
                w,
                if wp.0 == 27 {
                    Action::SpotifySearchPanel
                } else {
                    Action::SpotifySearchSubmit
                },
            );
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
                match (w.settings, w.page) {
                    (true, assistant::SETTINGS_PAGE) => Action::AiSaveKey,
                    (false, assistant::PAGE) => Action::AiSend,
                    (false, 2) => Action::AddEvent,
                    _ => Action::AddTask,
                },
            );
            LRESULT(0)
        }
        m if m == WM_APP + 8 => {
            sync_compact_activity(w);
            if !w.shared.borrow().data.settings.motion {
                w.entrance = None;
                apply_frame(hwnd, w);
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
        WM_MOUSEWHEEL if w.expanded && (0..62).contains(&w.pointer.0) => {
            let max_scroll = sidebar_max_scroll(figma_ui::SIDEBAR_ICONS.len());
            if max_scroll > 0. {
                let delta = ((wp.0 >> 16) as u16) as i16;
                w.sidebar_target_scroll = (w.sidebar_target_scroll
                    - (delta as f32 / 120.) * SIDEBAR_STEP)
                    .clamp(0., max_scroll);
                if !w.shared.borrow().data.settings.motion {
                    w.sidebar_scroll = w.sidebar_target_scroll;
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
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
        WM_MOUSEWHEEL if w.expanded && w.page == 0 && w.spotify_search.open => {
            let delta = ((wp.0 >> 16) as u16) as i16;
            if w.spotify_search_contains(w.pointer.0, w.pointer.1) {
                action(
                    hwnd,
                    w,
                    Action::SpotifySearchScroll(-i32::from(delta.signum())),
                );
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL if w.expanded && w.page == assistant::PAGE => {
            let delta = ((wp.0 >> 16) as u16) as i16;
            if w.assistant.history_open {
                w.assistant.history_offset = w
                    .assistant
                    .history_offset
                    .saturating_add_signed(-(delta.signum() as isize));
            } else {
                w.assistant.scroll = (w.assistant.scroll - delta as f32 / 120. * 42.)
                    .clamp(0., w.assistant.scroll_max);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_MOUSEWHEEL if w.expanded && w.page == 1 && w.focus_task_menu => {
            let delta = ((wp.0 >> 16) as u16) as i16;
            action(hwnd, w, Action::FocusTaskScroll(-i32::from(delta.signum())));
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
            let width = (lp.0 & 0xffff) as u32;
            let height = ((lp.0 >> 16) & 0xffff) as u32;
            if w.canvas
                .as_mut()
                .is_some_and(|canvas| w.settings || canvas.resize_shell(width, height).is_err())
            {
                w.canvas = None;
                w.overlay = None;
            }
            LRESULT(0)
        }
        m if m == WM_APP + 5 => {
            w.overlay = None;
            apply_frame(hwnd, w);
            LRESULT(0)
        }
        m if m == WM_APP + 9 => {
            if sync_compact_activity(w) {
                region(hwnd, w);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        m if m == assistant_voice::READY => {
            if let Some(receiver) = w.assistant.dictation.take() {
                match receiver.try_recv() {
                    Ok(Ok(text)) if !w.ai_input.is_invalid() => {
                        let existing = text_input(w.ai_input);
                        let combined = if existing.trim().is_empty() {
                            text
                        } else {
                            format!("{} {text}", existing.trim_end())
                        };
                        let limited: Vec<_> = combined.encode_utf16().take(1500).collect();
                        let _ = SetWindowTextW(
                            w.ai_input,
                            &HSTRING::from(String::from_utf16_lossy(&limited)),
                        );
                    }
                    Ok(Err(message)) => w.assistant.notice = message,
                    _ => w.assistant.notice = "Dikte belum bisa dimulai.",
                }
                w.assistant.scroll_to_end = true;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        m if m == assistant::READY => {
            let saved = w.assistant.finish();
            if saved {
                let path = w
                    .shared
                    .borrow()
                    .store
                    .with_file_name("assistant-chats.json");
                w.assistant.save_history(&path);
            }
            if !saved
                && !w.assistant.notice.is_empty()
                && !w.ai_input.is_invalid()
                && text_input(w.ai_input).is_empty()
            {
                let _ = SetWindowTextW(w.ai_input, &HSTRING::from(&w.assistant.prompt));
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        m if m == assistant::READY + 1 => {
            w.assistant.configured = None;
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        m if m == WM_APP + 12 => {
            let _ = probes::running_media_diagnostics(hwnd, w);
            LRESULT(0)
        }
        m if m == WM_APP + 13 => {
            if !w.settings && !w.shared.borrow().data.settings.taskbar_layout {
                action(hwnd, w, Action::Setting(17, 1));
            }
            LRESULT(0)
        }
        m if m == WM_APP + 14 => {
            if !w.settings {
                update_policy(hwnd, w);
            }
            LRESULT(0)
        }
        m if m == WM_APP + 15 => {
            if !w.settings && !w.shared.borrow().data.settings.cover_fallback {
                action(hwnd, w, Action::Setting(18, 1));
            }
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
            let (x, y) = w.mouse_point(lp);
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
            if let Some((hit_x, hit_w)) = w.volume_drag.take() {
                let _ = ReleaseCapture();
                spotify_volume_to(w, x, hit_x, hit_w);
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }
            if let Some(scrub) = w.scrubbing.take() {
                let _ = ReleaseCapture();
                if let Some(m) = &w.shared.borrow().media {
                    let _ = m.send.send(media::Command::Seek {
                        fraction: scrub.fraction(x),
                        generation: scrub.generation,
                    });
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }
            if let Some(h) = w
                .hits
                .iter()
                .find(|h| x >= h.x && x < h.x + h.w && y >= h.y && y < h.y + h.h)
            {
                if h.action != Action::Seek {
                    let a = h.action;
                    action(hwnd, w, a);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            } else if !w.settings {
                if input::focus_at(w, x, y) || w.spotify_search_contains(x, y) {
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
            w.volume_drag = None;
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_TIMER => {
            if w.startup_registration_pending && w.entrance.is_none() {
                w.startup_registration_pending = false;
                let enabled = w.shared.borrow().data.settings.startup;
                if enabled && let Err(error) = startup::apply(true) {
                    w.shared.borrow_mut().status = format!("Startup belum diperbarui: {error}");
                }
            }
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
                if let Some(message) = focus::complete_if_due(&mut s) {
                    focus::notify_completion(&mut s, message, w.expanded && w.page == 1);
                    let _ = s.data.save(&s.store);
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
            if sync_compact_activity(w) {
                region(hwnd, w);
            }
            if !w.settings {
                update_policy(hwnd, w);
                let page_visible = w.expanded && w.page == monitor::PAGE;
                let compact_enabled = !w.expanded
                    && matches!(
                        w.mode,
                        shell_policy::Mode::Compact | shell_policy::Mode::Peek
                    )
                    && w.shared.borrow().data.settings.monitor_in_compact;
                changed |= w.monitor.poll(page_visible, compact_enabled);
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
                if sync_compact_activity(w) {
                    region(hwnd, w);
                }
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
    let initial_island = f32::from(shared.borrow().data.settings.dynamic_island);
    let initial_activity = f32::from(shared.borrow().alert.is_some());
    let startup_registration_pending =
        !settings && !offscreen && shared.borrow().data.settings.startup;
    let w = Box::new(Window {
        shared,
        canvas: None,
        alpha_shell: false,
        dpi: if offscreen {
            dpi::Scale::STANDARD
        } else {
            dpi::Scale::system()
        },
        settings,
        expanded: false,
        hovered: false,
        hover_tick: None,
        mode: shell_policy::Mode::Compact,
        idle_transition: None,
        island_style: motion::Tween::new(initial_island, Instant::now()),
        compact_activity_size: motion::Tween::new(initial_activity, Instant::now()),
        compact_activity_progress: initial_activity,
        entrance: None,
        startup_registration_pending,
        first_frame_ms: 0.,
        cover_render_error: RefCell::new(String::new()),
        battery: if offscreen {
            power::Battery::default()
        } else {
            power::Battery::read().unwrap_or_default()
        },
        battery_checked: Instant::now(),
        power_notice: None,
        power_notification: None,
        taskbar_watch: None,
        power_changes: 0,
        last_power_change: None,
        compact_activity: figma_ui::CompactActivity::default(),
        companion: mascot::Companion::default(),
        companion_pointer: None,
        last_policy_check: Instant::now(),
        page: if settings { 0 } else { 1 },
        hits: vec![],
        input: HWND::default(),
        ai_input: HWND::default(),
        ai_model_input: HWND::default(),
        ai_url_input: HWND::default(),
        ai_input_visible: false,
        spotify_search: spotify::Search::default(),
        spotify_search_reveal: motion::Tween::new(0., Instant::now()),
        assistant: assistant::Assistant::default(),
        monitor: monitor::Monitor::default(),
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
        volume_drag: None,
        task_priority: 0,
        focus_task_menu: false,
        focus_task_offset: 0,
        sidebar_scroll: 0.,
        sidebar_target_scroll: 0.,
        shelf_selected: 0,
        shelf_preview_open: false,
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
            WS_EX_LAYERED
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
        w.taskbar_watch = taskbar::ForegroundWatch::register(hwnd);
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
            lfHeight: w.dpi.px(-14),
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
    if !settings
        && !offscreen
        && w.shared.borrow().data.settings.motion
        && w.mode != shell_policy::Mode::Hidden
    {
        w.entrance = Some(Instant::now());
        apply_frame(hwnd, w);
    }
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
            if (*ptr).spotify_search.edit == hwnd {
                if msg == WM_KEYDOWN && matches!(wp.0, 13 | 27) {
                    let _ = PostMessageW(Some(parent), spotify::EDIT_ACTION, wp, LPARAM(0));
                    return LRESULT(0);
                }
                if msg == WM_CHAR && matches!(wp.0, 13 | 27 | 9) {
                    return LRESULT(0);
                }
                if msg == WM_KEYDOWN && wp.0 == 9 {
                    (*ptr).focused = (*ptr)
                        .hits
                        .iter()
                        .position(|hit| hit.action == Action::SpotifySearchSubmit);
                    let _ = SetFocus(Some(parent));
                    let _ = InvalidateRect(Some(parent), None, false);
                    return LRESULT(0);
                }
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
            let assistant_field =
                [(*ptr).ai_input, (*ptr).ai_model_input, (*ptr).ai_url_input].contains(&hwnd);
            if !(*ptr).settings
                && (*ptr).page == assistant::PAGE
                && (*ptr).ai_input == hwnd
                && !(*ptr).assistant.waiting()
                // TranslateMessage turns Ctrl+V into WM_CHAR. Handle it here
                // so the native EDIT cannot paste it again after WM_KEYDOWN.
                && (msg == WM_PASTE || msg == WM_CHAR && wp.0 == 0x16)
                && let Some(result) = assistant_image::paste(hwnd)
            {
                match result {
                    Ok(bytes) => {
                        (*ptr).assistant.attach_clipboard_image(bytes);
                    }
                    Err(_) => (*ptr).assistant.notice = "Gambar clipboard tidak dapat disiapkan.",
                }
                let _ = InvalidateRect(Some(parent), None, false);
                return LRESULT(0);
            }
            if msg == WM_KEYDOWN && wp.0 == 9 && assistant_field {
                if (*ptr).settings {
                    let fields = [(*ptr).ai_input, (*ptr).ai_model_input, (*ptr).ai_url_input];
                    let backwards = GetKeyState(VK_SHIFT.0 as i32) < 0;
                    let index = fields.iter().position(|field| *field == hwnd).unwrap();
                    let next = if backwards {
                        index.checked_sub(1)
                    } else {
                        Some(index + 1)
                    };
                    if let Some(next) = next.and_then(|index| fields.get(index))
                        && IsWindowVisible(*next).as_bool()
                    {
                        let _ = SetFocus(Some(*next));
                        return LRESULT(0);
                    }
                }
                (*ptr).focused = (*ptr).hits.iter().position(|hit| {
                    hit.action
                        == if (*ptr).settings {
                            Action::AiSaveKey
                        } else {
                            Action::AiSend
                        }
                });
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
            if msg == WM_CHAR && wp.0 == 13 && assistant_field {
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
    let _ = APP_STARTED.set(Instant::now());
    if let Err(error) = run() {
        if std::env::args().any(|a| a == "--offscreen") {
            eprintln!("{error}");
            std::process::exit(1);
        }
        unsafe {
            let _ = MessageBoxW(
                None,
                &HSTRING::from(i18n::message(&error.to_string()).as_ref()),
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
    if std::env::args().any(|arg| arg == "--cover-fallback-probe") {
        return probes::cover_fallback_diagnostics();
    }
    if std::env::args().any(|arg| arg == "--cover-fallback-on") {
        unsafe {
            PostMessageW(
                Some(FindWindowW(w!("Horizon.Native"), w!("Horizon"))?),
                WM_APP + 15,
                WPARAM(0),
                LPARAM(0),
            )?;
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--taskbar-layout-on") {
        unsafe {
            PostMessageW(
                Some(FindWindowW(w!("Horizon.Native"), w!("Horizon"))?),
                WM_APP + 13,
                WPARAM(0),
                LPARAM(0),
            )?;
        }
        return Ok(());
    }
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
                if std::env::args().any(|arg| arg == "--autostart") {
                    return Ok(());
                }
                if let Ok(data) = Data::load(&Data::path()) {
                    i18n::set_language(data.settings.language);
                }
                return Err(Error::new(
                    E_FAIL,
                    i18n::tr(
                        "Horizon masih berjalan. Tutup versi sebelumnya lewat Settings > Tentang > Keluar, lalu buka versi ini.",
                    ),
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
        let icon_resource = PCWSTR(std::ptr::without_provenance(1));
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: w!("Horizon.Native"),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            // Win32 integer resource ID, never dereferenced as a string.
            hIcon: LoadIconW(Some(instance.into()), icon_resource)?,
            hIconSm: HICON(
                LoadImageW(
                    Some(instance.into()),
                    icon_resource,
                    IMAGE_ICON,
                    GetSystemMetrics(SM_CXSMICON),
                    GetSystemMetrics(SM_CYSMICON),
                    LR_SHARED,
                )?
                .0,
            ),
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(Error::from_thread());
        }
        let offscreen = std::env::args().any(|a| a == "--offscreen");
        let mut data = if offscreen {
            Data::default()
        } else {
            Data::load(&Data::path())
                .map_err(|e| Error::new(E_FAIL, format!("Data tidak diubah: {e}")))?
        };
        if offscreen && std::env::args().any(|arg| arg == "--startup-island") {
            data.settings.dynamic_island = true;
        }
        i18n::set_language(data.settings.language);
        let focus_left = if data.focus.phase == focus::Phase::Work {
            data.focus_remaining
                .filter(|&seconds| seconds > 0)
                .unwrap_or(u64::from(data.settings.focus_minutes) * 60)
        } else {
            data.focus_remaining.unwrap_or(0)
        };
        let shared = Rc::new(RefCell::new(Shared {
            alert: data.reminders.current(),
            data,
            status: "Siap".into(),
            focus_left,
            running: None,
            focus_accounted: None,
            notch: HWND::default(),
            settings_window: HWND::default(),
            media: None,
            spotify_volume: None,
            store: if offscreen {
                std::env::temp_dir().join(format!("horizon-probe-{}.json", std::process::id()))
            } else {
                Data::path()
            },
        }));
        let _font = host::PrivateFont::load()?;
        let root = create(shared.clone(), false, offscreen)?;
        if !offscreen {
            let mut state = shared.borrow_mut();
            let media = media::Media::start(
                state.data.settings.show_lyrics,
                state.data.settings.cover_fallback,
            );
            media
                .notify
                .store(root.0 as usize, std::sync::atomic::Ordering::Relaxed);
            state.media = Some(media);
            state.spotify_volume = Some(audio::SpotifyVolume::start(root.0 as usize));
        }
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
                source_app: "Spotify.exe".into(),
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
            shared.borrow_mut().spotify_volume = Some(audio::SpotifyVolume::fixture(0.65));
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
            for (name, title) in [
                ("two-line", "Somewhere Beyond The City"),
                (
                    "long-title",
                    "A Very Long Song Title That Must Not Move The Artist Or Spotify Link",
                ),
            ] {
                if let Some(media) = &shared.borrow().media
                    && let Ok(mut track) = media.snapshot.lock()
                {
                    track.title = title.into();
                }
                state.page = 0;
                let width = shared.borrow().data.settings.width as u32;
                state.canvas = Some(Canvas::image(width, host_height() as u32)?);
                state.paint()?;
                state
                    .canvas
                    .as_ref()
                    .ok_or_else(|| Error::new(E_FAIL, "No canvas"))?
                    .save_bmp(std::path::Path::new(&format!(
                        "artifacts/previews/music-{name}.bmp"
                    )))?;
            }
            if let Some(media) = &shared.borrow().media
                && let Ok(mut track) = media.snapshot.lock()
            {
                track.title = "Night Drive".into();
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
            for page in 0..=assistant::SETTINGS_PAGE {
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
            let mut bounds = RECT::default();
            GetClientRect(root, &mut bounds)?;
            (*raw).canvas = Some(Canvas::new(
                root,
                bounds.right as u32,
                bounds.bottom as u32,
                (*raw).alpha_shell,
                (*raw).dpi,
            )?);
            (*raw).paint()?;
            if std::env::args().any(|a| a == "--settings") {
                let dialog = create(shared.clone(), true, true)?;
                let raw = GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Window;
                for p in 0..=assistant::SETTINGS_PAGE {
                    (*raw).page = p;
                    if (*raw).canvas.is_none() {
                        (*raw).canvas = Some(Canvas::new(dialog, 710, 550, false, (*raw).dpi)?);
                    }
                    (*raw).paint()?;
                }
            }
        }
        if offscreen && std::env::args().any(|a| a == "--verify") {
            let dialog = create(shared.clone(), true, true)?;
            let settings = &mut *(GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Window);
            let mut wired = std::collections::BTreeSet::new();
            for page in 0..=assistant::SETTINGS_PAGE {
                settings.page = page;
                settings.canvas = Some(Canvas::image(710, 550)?);
                settings.paint()?;
                for h in &settings.hits {
                    if h.x < 0 || h.y < 0 || h.x + h.w > 710 || h.y + h.h > 495 {
                        return Err(Error::new(
                            E_FAIL,
                            "Settings action is outside its content area",
                        ));
                    }
                    if let Action::Setting(key, _) = h.action {
                        wired.insert(key);
                    }
                }
            }
            sync_inputs(dialog, settings);
            if settings.ai_input.is_invalid()
                || (GetWindowLongPtrW(settings.ai_input, GWL_STYLE) & 0x20) == 0
                || !settings
                    .hits
                    .iter()
                    .any(|hit| hit.action == Action::AiSaveKey)
                || !settings
                    .hits
                    .iter()
                    .any(|hit| hit.action == Action::AiProvider(assistant::CUSTOM))
                || settings.ai_model_input.is_invalid()
            {
                return Err(Error::new(
                    E_FAIL,
                    "AI Settings needs a masked key field and model controls",
                ));
            }
            settings.pointer = (30, 110);
            settings.paint()?;
            if frame_interval(settings).is_none() {
                return Err(Error::new(
                    E_FAIL,
                    "Settings hover did not schedule animation frames",
                ));
            }
            settings.pointer = (-1, -1);
            if wired != (0..SETTINGS_COUNT).collect() {
                return Err(Error::new(E_FAIL, "Some settings controls are missing"));
            }
            // Exercise a width change below the configured maximum, including
            // when the current default already starts at that maximum.
            shared.borrow_mut().data.settings.width = 591;
            let before = shared.borrow().data.settings.width;
            for key in 0..SETTINGS_COUNT {
                action(dialog, settings, Action::Setting(key, 1));
            }
            let store = shared.borrow().store.clone();
            let loaded = Data::load(&store).map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            if loaded.settings.language != i18n::Language::English
                || i18n::tr("Tampilan") != "Appearance"
            {
                return Err(Error::new(E_FAIL, "Language selection did not round-trip"));
            }
            if loaded.settings.width != before + 1 {
                return Err(Error::new(E_FAIL, "Settings did not round-trip"));
            }
            let root_state = &mut *(GetWindowLongPtrW(root, GWLP_USERDATA) as *mut Window);
            apply_frame(root, root_state);
            if !loaded.settings.dynamic_island
                || root_state.island_style.sample(Instant::now()) != 1.
            {
                return Err(Error::new(
                    E_FAIL,
                    "Dynamic Island selection did not persist/apply",
                ));
            }
            let mut client = RECT::default();
            GetClientRect(root, &mut client)?;
            if client.right - client.left != shared.borrow().data.settings.width {
                return Err(Error::new(E_FAIL, "Width was not applied to HWND"));
            }
            let mut alpha = 0;
            GetLayeredWindowAttributes(root, None, Some(&mut alpha), None)?;
            if alpha != surface_opacity(root_state, &shared.borrow().data.settings) {
                return Err(Error::new(E_FAIL, "Opacity was not applied to HWND"));
            }
            let saved_shell = (root_state.expanded, root_state.mode, root_state.page);
            for surface in 0..OPACITY_SURFACES {
                let expected = 80 + surface as u8 * 17;
                shared.borrow_mut().data.settings.opacity_overrides[surface] = Some(expected);
                let target = if surface == 8 {
                    apply_frame(dialog, settings);
                    dialog
                } else {
                    root_state.expanded = surface < 6 || surface >= ASSISTANT_OPACITY;
                    root_state.page = match surface {
                        ASSISTANT_OPACITY => assistant::PAGE,
                        MONITOR_OPACITY => monitor::PAGE,
                        _ => surface.min(5),
                    };
                    root_state.mode = if surface == 7 {
                        shell_policy::Mode::Peek
                    } else {
                        shell_policy::Mode::Compact
                    };
                    apply_frame(root, root_state);
                    root
                };
                GetLayeredWindowAttributes(target, None, Some(&mut alpha), None)?;
                if alpha != expected {
                    return Err(Error::new(
                        E_FAIL,
                        "Independent surface opacity was not applied",
                    ));
                }
            }
            (root_state.expanded, root_state.mode, root_state.page) = saved_shell;
            shared.borrow_mut().data.settings.opacity_overrides = loaded.settings.opacity_overrides;
            apply_frame(root, root_state);
            apply_frame(dialog, settings);
            for language in [i18n::Language::Indonesian, i18n::Language::English] {
                shared.borrow_mut().data.settings.language = language;
                i18n::set_language(language);
                root_state.input_layout = None;
                sync_inputs(root, root_state);
                let mut cue = [0u16; 128];
                SendMessageW(
                    root_state.input,
                    0x1502,
                    Some(WPARAM(cue.as_mut_ptr() as usize)),
                    Some(LPARAM(cue.len() as isize)),
                );
                let end = cue.iter().position(|c| *c == 0).unwrap_or(cue.len());
                if String::from_utf16_lossy(&cue[..end]) != i18n::tr("Tambah tugas") {
                    return Err(Error::new(
                        E_FAIL,
                        format!(
                            "Input cue language did not refresh: got {:?}, expected {:?}",
                            String::from_utf16_lossy(&cue[..end]),
                            i18n::tr("Tambah tugas")
                        ),
                    ));
                }
                for page in 0..=assistant::SETTINGS_PAGE {
                    settings.page = page;
                    settings.paint()?;
                }
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
            SetWindowTextW(root_state.input, w!("Focus fixture"))?;
            action(root, root_state, Action::AddTask);
            action(root, root_state, Action::FocusTask(Some(0)));
            action(root, root_state, Action::SetBreakMinutes(3));
            action(root, root_state, Action::Focus);
            if shared.borrow().running.is_none() {
                return Err(Error::new(E_FAIL, "Focus start failed"));
            }
            {
                let mut state = shared.borrow_mut();
                state.running =
                    Some(Instant::now() - std::time::Duration::from_secs(focus::duration(&state)));
                if focus::complete_if_due(&mut state).is_none()
                    || state.data.focus.task != Some(0)
                    || focus::complete_if_due(&mut state).is_some()
                {
                    return Err(Error::new(
                        E_FAIL,
                        "Focus did not complete exactly once with the linked task",
                    ));
                }
            }
            action(root, root_state, Action::FinishFocusTask);
            if !shared.borrow().data.tasks[0].done {
                return Err(Error::new(E_FAIL, "Linked task was not completed"));
            }
            action(root, root_state, Action::ExtendFocus);
            {
                let mut state = shared.borrow_mut();
                if state.data.focus.phase != focus::Phase::Extension || state.focus_left != 300 {
                    return Err(Error::new(E_FAIL, "Focus extension did not start"));
                }
                state.running = Some(Instant::now() - std::time::Duration::from_secs(300));
                focus::complete_if_due(&mut state);
                let today = Local::now().format("%Y-%m-%d").to_string();
                if state.data.focus.completed.get(&today) != Some(&1) {
                    return Err(Error::new(
                        E_FAIL,
                        "Extension was counted as a separate session",
                    ));
                }
            }
            action(root, root_state, Action::StartFocusBreak);
            {
                let mut state = shared.borrow_mut();
                if state.data.focus.phase != focus::Phase::Break || state.focus_left != 180 {
                    return Err(Error::new(E_FAIL, "Configured break did not start"));
                }
                let history = state.data.focus_history.clone();
                state.running = Some(Instant::now() - std::time::Duration::from_secs(180));
                if focus::complete_if_due(&mut state).is_none()
                    || state.data.focus.phase != focus::Phase::BreakFinished
                    || history != state.data.focus_history
                {
                    return Err(Error::new(E_FAIL, "Break completion affected work history"));
                }
            }
            action(root, root_state, Action::Reset);
            action(root, root_state, Action::DeleteTask(0));
            if shared.borrow().running.is_some() {
                return Err(Error::new(E_FAIL, "Focus reset failed"));
            }
            std::fs::create_dir_all("artifacts").map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            probes::calendar_checks(root, root_state)?;
            polish_checks::run(root, root_state)?;
            probes::mascot_checks(root, root_state)?;
            probes::volume_input_checks(root)?;
            std::fs::write("artifacts/ui-checks.json",r#"{"settings_roundtrip":true,"all_32_setting_controls_rendered":true,"bilingual_settings_pages_rendered":18,"native_input_cues_switch_language":true,"independent_surface_opacity":true,"native_width_and_opacity":true,"task_create_complete_delete":true,"focus_start_reset":true,"calendar_month_grid_and_form_roundtrip":true,"monitor_toggle_and_compact_region":true,"desktop_input_used":false,"startup_modified":false}"#).map_err(|e|Error::new(E_FAIL,e.to_string()))?;
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
            let settings_hwnd = state.shared.borrow().settings_window;
            let settings_ptr =
                if !settings_hwnd.is_invalid() && IsWindow(Some(settings_hwnd)).as_bool() {
                    GetWindowLongPtrW(settings_hwnd, GWLP_USERDATA) as *mut Window
                } else {
                    std::ptr::null_mut()
                };
            let settings_interval = if settings_ptr.is_null() {
                None
            } else {
                frame_interval(&*settings_ptr)
            };
            if let Some(interval) = match (frame_interval(state), settings_interval) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            } {
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
                    if settings_interval.is_some() && IsWindow(Some(settings_hwnd)).as_bool() {
                        let live = GetWindowLongPtrW(settings_hwnd, GWLP_USERDATA) as *mut Window;
                        if !live.is_null() {
                            frame_step(settings_hwnd, &mut *live);
                        }
                    }
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
    #[test]
    fn opacity_is_independent_per_surface_and_global_reset_is_explicit() {
        let mut s = Settings::default();
        s.adjust(21, -104).unwrap();
        s.adjust(27, -52).unwrap();
        s.adjust(OPACITY_SETTING_START + ASSISTANT_OPACITY, -80)
            .unwrap();
        assert_eq!(s.opacity_for(0), 151);
        assert_eq!(s.opacity_for(6), 203);
        for index in [1, 2, 3, 4, 5, 7, 8] {
            assert_eq!(s.opacity_for(index), 255);
        }
        let saved: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(saved.opacity_for(0), 151);
        assert_eq!(saved.opacity_for(ASSISTANT_OPACITY), 175);
        s.adjust(1, -26).unwrap();
        assert!((0..OPACITY_SURFACES).all(|i| s.opacity_for(i) == 229));
        let legacy: Settings = serde_json::from_str(r#"{"opacity":242}"#).unwrap();
        assert!((0..OPACITY_SURFACES).all(|i| legacy.opacity_for(i) == 242));
        let legacy: Settings = serde_json::from_str(
            r#"{"opacity":242,"opacity_overrides":[151,null,null,null,null,null,203,null,null]}"#,
        )
        .unwrap();
        assert_eq!(legacy.opacity_for(0), 151);
        assert_eq!(legacy.opacity_for(6), 203);
        assert_eq!(legacy.opacity_for(ASSISTANT_OPACITY), 242);
    }
    #[test]
    fn island_selection_is_explicit_persistent_and_legacy_defaults_to_notch() {
        let mut s: Settings = serde_json::from_str(r#"{"width":680}"#).unwrap();
        assert!(!s.dynamic_island);
        s.adjust(19, 1).unwrap();
        s.adjust(19, 1).unwrap();
        let mut saved: Settings =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert!(saved.dynamic_island);
        saved.adjust(19, -1).unwrap();
        assert!(!saved.dynamic_island);
        assert_eq!(saved.width, 680);
    }
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
