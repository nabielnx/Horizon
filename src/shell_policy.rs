//! Shell visibility is independent of the renderer and the lyric lookup flag.
//! A playing track can pin the compact lyric view above ordinary apps, while
//! fullscreen hiding remains a separate policy with higher priority.

use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::{
        Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
        Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow},
    },
    UI::WindowsAndMessaging::{
        GWL_STYLE, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, WS_CAPTION, WS_THICKFRAME,
    },
};

pub const HOVER_LEAVE_MS: u64 = 150;

/// Idle bounds interpolate from the surface actually on screen, not an
/// assumed compact frame. This prevents Indicator -> Compact -> Peek flashes.
pub struct Transition {
    from: Size,
    to: Size,
    started: std::time::Instant,
    pub current: Size,
}
impl Transition {
    pub fn new(from: Size, to: Size, now: std::time::Instant) -> Self {
        Self {
            from,
            to,
            started: now,
            current: from,
        }
    }
    pub fn step(&mut self, now: std::time::Instant) -> bool {
        let progress =
            (now.saturating_duration_since(self.started).as_secs_f32() / 0.26).clamp(0., 1.);
        let t = crate::motion::ease(progress);
        self.current = Size {
            width: self.from.width + (self.to.width - self.from.width) * t,
            height: self.from.height + (self.to.height - self.from.height) * t,
            shape: self.from.shape + (self.to.shape - self.from.shape) * t,
        };
        progress < 1.
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Expanded,
    Peek,
    Compact,
    Indicator,
    Hidden,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
    /// Same meaning as the existing outline's expansion parameter.
    pub shape: f32,
}

impl Mode {
    pub fn idle_geometry(self, panel_width: f32) -> Size {
        let mode = if self == Self::Expanded {
            Self::Peek
        } else {
            self
        };
        let mut size = mode.size(panel_width);
        size.shape = match mode {
            Self::Peek => 1.,
            Self::Compact => 1. - 9. / 14.,
            _ => 0.,
        };
        size
    }
    #[allow(dead_code)]
    pub fn size(self, panel_width: f32) -> Size {
        let width = if panel_width.is_finite() {
            panel_width.max(1.)
        } else {
            560.
        };
        let (width, height, shape) = match self {
            Self::Expanded => (width, crate::EXPANDED_HEIGHT as f32, 1.),
            Self::Peek => (width.min(591.), 101., 0.),
            Self::Compact => (310., 50., 0.),
            Self::Indicator => (54., 6., 0.),
            Self::Hidden => (0., 0., 0.),
        };
        Size {
            width,
            height,
            shape,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Inputs {
    pub expanded: bool,
    pub hovered: bool,
    pub other_app: bool,
    pub fullscreen: bool,
    /// True only for ShowLyrics && has_track && playing, not ShowLyrics alone.
    pub lyrics_pinned: bool,
    pub hide_fullscreen: bool,
    pub has_alert: bool,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            expanded: false,
            hovered: false,
            other_app: false,
            fullscreen: false,
            lyrics_pinned: false,
            hide_fullscreen: true,
            has_alert: false,
        }
    }
}

pub fn decide(input: Inputs) -> Mode {
    // Deliberate opening is allowed, matching the previous app. The host can
    // collapse on a new external fullscreen activation before evaluating this.
    if input.expanded {
        Mode::Expanded
    } else if input.hide_fullscreen && input.fullscreen {
        Mode::Hidden
    } else if input.hovered {
        Mode::Peek
    } else if input.other_app && !input.lyrics_pinned && !input.has_alert {
        Mode::Indicator
    } else {
        Mode::Compact
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Foreground {
    pub other_app: bool,
    pub fullscreen: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Bounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl From<RECT> for Bounds {
    fn from(value: RECT) -> Self {
        Self {
            left: value.left,
            top: value.top,
            right: value.right,
            bottom: value.bottom,
        }
    }
}

impl Bounds {
    fn covers(self, monitor: Self) -> bool {
        monitor.right > monitor.left
            && monitor.bottom > monitor.top
            && self.right > self.left
            && self.bottom > self.top
            && self.left as i64 <= monitor.left as i64 + 1
            && self.top as i64 <= monitor.top as i64 + 1
            && self.right as i64 >= monitor.right as i64 - 1
            && self.bottom as i64 >= monitor.bottom as i64 - 1
    }
}

fn desktop_class(name: &str) -> bool {
    matches!(
        name,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}

fn classify(bounds: Bounds, monitor: Bounds, maximized: bool, decorated: bool) -> Foreground {
    Foreground {
        other_app: true,
        // A maximized regular window may fill the monitor with an auto-hidden
        // taskbar. Keep that distinct from a borderless/F11/exclusive window.
        fullscreen: bounds.covers(monitor) && !(maximized && decorated),
    }
}

/// Read-only foreground classification for the monitor containing Horizon.
/// A fullscreen app on another monitor does not hide this notch. No focus,
/// activation, input injection, hook, or window mutation occurs here.
pub unsafe fn foreground(root: HWND, settings: HWND) -> Foreground {
    unsafe {
        let current = GetForegroundWindow();
        if current.is_invalid()
            || current == root
            || current == settings
            || !IsWindowVisible(current).as_bool()
            || IsIconic(current).as_bool()
        {
            return Foreground::default();
        }
        let mut root_process = 0;
        let mut current_process = 0;
        GetWindowThreadProcessId(root, Some(&mut root_process));
        GetWindowThreadProcessId(current, Some(&mut current_process));
        if root_process != 0 && current_process == root_process {
            return Foreground::default();
        }
        let mut name = [0u16; 256];
        let length = GetClassNameW(current, &mut name);
        if length > 0 && desktop_class(&String::from_utf16_lossy(&name[..length as usize])) {
            return Foreground::default();
        }

        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let monitor = MonitorFromWindow(root, MONITOR_DEFAULTTOPRIMARY);
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Foreground {
                other_app: true,
                fullscreen: false,
            };
        }
        let mut bounds = RECT::default();
        if DwmGetWindowAttribute(
            current,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut bounds as *mut RECT as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
            && GetWindowRect(current, &mut bounds).is_err()
        {
            return Foreground {
                other_app: true,
                fullscreen: false,
            };
        }
        let style = GetWindowLongPtrW(current, GWL_STYLE) as u32;
        let decorated = style & (WS_CAPTION.0 | WS_THICKFRAME.0) != 0;
        classify(
            bounds.into(),
            info.rcMonitor.into(),
            IsZoomed(current).as_bool(),
            decorated,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicator_opens_directly_to_peek_and_reverses_from_visible_bounds() {
        let now = std::time::Instant::now();
        let indicator = Mode::Indicator.idle_geometry(680.);
        let peek = Mode::Peek.idle_geometry(680.);
        let mut opening = Transition::new(indicator, peek, now);
        assert_eq!(opening.current, indicator);
        assert!(opening.step(now + std::time::Duration::from_millis(30)));
        let visible = opening.current;
        assert!(visible.height > indicator.height && visible.height < peek.height);
        let mut closing = Transition::new(visible, indicator, now);
        assert_eq!(closing.current, visible);
        assert!(!closing.step(now + std::time::Duration::from_millis(300)));
        assert_eq!(closing.current, indicator);
        assert!(!opening.step(now + std::time::Duration::from_millis(300)));
        assert_eq!(opening.current, peek);
    }

    #[test]
    fn lyrics_pin_keeps_compact_above_normal_apps_but_not_fullscreen() {
        let app = Inputs {
            other_app: true,
            ..Default::default()
        };
        assert_eq!(decide(app), Mode::Indicator);
        let pinned = Inputs {
            lyrics_pinned: true,
            ..app
        };
        assert_eq!(decide(pinned), Mode::Compact);
        assert_eq!(
            decide(Inputs {
                fullscreen: true,
                ..pinned
            }),
            Mode::Hidden
        );
        assert_eq!(
            decide(Inputs {
                fullscreen: true,
                hovered: true,
                ..pinned
            }),
            Mode::Hidden
        );
    }

    #[test]
    fn hover_reveals_indicator_and_desktop_restores_compact() {
        assert_eq!(
            decide(Inputs {
                other_app: true,
                hovered: true,
                ..Default::default()
            }),
            Mode::Peek
        );
        assert_eq!(decide(Inputs::default()), Mode::Compact);
        assert_eq!(
            decide(Inputs {
                has_alert: true,
                other_app: true,
                ..Default::default()
            }),
            Mode::Compact
        );
        assert_eq!(
            decide(Inputs {
                expanded: true,
                fullscreen: true,
                ..Default::default()
            }),
            Mode::Expanded
        );
        assert_eq!(
            decide(Inputs {
                fullscreen: true,
                hide_fullscreen: false,
                other_app: true,
                ..Default::default()
            }),
            Mode::Indicator
        );
    }

    #[test]
    fn dimensions_match_approved_figma_layout() {
        assert_eq!(
            Mode::Compact.size(560.),
            Size {
                width: 310.,
                height: 50.,
                shape: 0.
            }
        );
        assert_eq!(
            Mode::Peek.size(560.),
            Size {
                width: 560.,
                height: 101.,
                shape: 0.
            }
        );
        assert_eq!(
            Mode::Indicator.size(560.),
            Size {
                width: 54.,
                height: 6.,
                shape: 0.
            }
        );
        assert_eq!(Mode::Expanded.size(591.).height, 280.);
        assert_eq!(Mode::Expanded.size(480.).height, 280.);
        assert_eq!(Mode::Hidden.size(560.).width, 0.);
    }

    #[test]
    fn maximized_workarea_and_decorated_auto_hide_taskbar_are_not_fullscreen() {
        let monitor = Bounds {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let workarea = Bounds {
            bottom: 1040,
            ..monitor
        };
        assert!(!classify(workarea, monitor, true, true).fullscreen);
        assert!(!classify(monitor, monitor, true, true).fullscreen);
        assert!(classify(monitor, monitor, false, false).fullscreen);
        assert!(classify(monitor, monitor, true, false).fullscreen);
    }

    #[test]
    fn secondary_monitor_and_desktop_classes_are_handled() {
        let primary = Bounds {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let secondary = Bounds {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        assert!(!classify(secondary, primary, false, false).fullscreen);
        assert!(classify(secondary, secondary, false, false).fullscreen);
        for name in [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
        ] {
            assert!(desktop_class(name));
        }
        assert!(!desktop_class("Chrome_WidgetWin_1"));
    }
}
