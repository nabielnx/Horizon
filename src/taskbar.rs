//! Read the primary taskbar; never move, resize or reparent Explorer windows.
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::UI::Accessibility::*;
use windows::{
    Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*},
    core::*,
};
static ROOT: AtomicUsize = AtomicUsize::new(0);
unsafe extern "system" fn foreground_changed(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    let root = ROOT.load(Ordering::Relaxed);
    if root != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(root as _)), WM_APP + 14, WPARAM(0), LPARAM(0));
        }
    }
}
pub struct ForegroundWatch(HWINEVENTHOOK);
impl ForegroundWatch {
    pub unsafe fn register(root: HWND) -> Option<Self> {
        unsafe {
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(foreground_changed),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            if hook.is_invalid() {
                return None;
            }
            ROOT.store(root.0 as usize, Ordering::Relaxed);
            Some(Self(hook))
        }
    }
}
impl Drop for ForegroundWatch {
    fn drop(&mut self) {
        ROOT.store(0, Ordering::Relaxed);
        unsafe {
            let _ = UnhookWinEvent(self.0);
        }
    }
}

pub unsafe fn primary() -> Option<(HWND, i32)> {
    unsafe {
        let bar = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
        let mut bounds = RECT::default();
        GetWindowRect(bar, &mut bounds).ok()?;
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(
            MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY),
            &mut info,
        )
        .as_bool()
        {
            return None;
        }
        top_height(bounds, info.rcMonitor).map(|height| (bar, height))
    }
}
fn top_height(bar: RECT, monitor: RECT) -> Option<i32> {
    let height = bar.bottom - bar.top;
    // Recognize an auto-hidden top taskbar even when translated above the edge.
    (height > 0
        && height < (monitor.bottom - monitor.top) / 3
        && bar.right - bar.left >= (monitor.right - monitor.left) / 2
        && bar.top <= monitor.top + 1
        && bar.bottom >= monitor.top)
        .then_some(height)
}
pub unsafe fn above(bar: HWND, root: HWND) -> bool {
    unsafe {
        let mut current = root;
        for _ in 0..512 {
            let Ok(previous) = GetWindow(current, GW_HWNDPREV) else {
                break;
            };
            if previous.is_invalid() || previous == current {
                break;
            }
            if previous == bar {
                return true;
            }
            current = previous;
        }
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn top_and_hidden_taskbars_share_the_anchor_other_edges_do_not() {
        let monitor = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        for top in [0, -46, -48] {
            assert_eq!(
                top_height(
                    RECT {
                        left: 0,
                        top,
                        right: 1920,
                        bottom: top + 48
                    },
                    monitor
                ),
                Some(48)
            );
        }
        assert_eq!(
            top_height(
                RECT {
                    left: 0,
                    top: 1032,
                    right: 1920,
                    bottom: 1080
                },
                monitor
            ),
            None
        );
        assert_eq!(
            top_height(
                RECT {
                    left: 0,
                    top: 0,
                    right: 48,
                    bottom: 1080
                },
                monitor
            ),
            None
        );
    }
}
