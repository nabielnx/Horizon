use crate::*;
use std::time::Duration;

pub(super) unsafe fn prepare_first_frame(hwnd: HWND, w: &mut Window) -> Result<()> {
    unsafe {
        if !w.settings {
            // Hidden startup only: initialize the full retained surface, including
            // pixels outside the compact contour that the first hover will reveal.
            if IsWindowVisible(hwnd).as_bool() {
                return Err(Error::new(
                    E_UNEXPECTED,
                    "Initial frame must precede window visibility",
                ));
            }
            if SetWindowRgn(hwnd, None, false) == 0 {
                return Err(Error::from_thread());
            }
        }
        let mut bounds = RECT::default();
        GetClientRect(hwnd, &mut bounds)?;
        w.canvas = Some(Canvas::new(
            hwnd,
            bounds.right as u32,
            bounds.bottom as u32,
        )?);
        w.paint()?;
        if !w.canvas.as_ref().is_some_and(Canvas::has_frame) {
            return Err(Error::new(E_UNEXPECTED, "Initial frame was not completed"));
        }
        if !w.settings {
            region(hwnd, w);
        }
        trace("create: first frame ready before show");
        Ok(())
    }
}

pub(super) unsafe fn begin_transition(w: &mut Window, shell: bool, duration: f32) {
    if w.shared.borrow().data.settings.motion {
        w.overlay = w
            .canvas
            .as_ref()
            .and_then(|c| unsafe { c.snapshot().ok() })
            .map(|bitmap| Overlay {
                bitmap,
                started: Instant::now(),
                duration,
                shell,
            });
    } else {
        w.overlay = None;
    }
}
pub(super) unsafe fn animate(_hwnd: HWND, w: &mut Window) {
    unsafe {
        begin_transition(w, true, 0.28);
    }
    if !w.expanded {
        w.preview_worker = None;
        if let Some(canvas) = &w.canvas {
            canvas.release_preview();
        }
    }
    if !w.shared.borrow().data.settings.motion {
        w.expansion = if w.expanded { 1. } else { 0. };
        w.peek = if w.expanded || w.mode == shell_policy::Mode::Peek {
            1.
        } else {
            0.
        };
        w.expansion_velocity = 0.;
        w.peek_velocity = 0.;
    }
}
pub(super) fn frame_interval(w: &Window) -> Option<Duration> {
    let state = w.shared.borrow();
    let now = Instant::now();
    let target = if w.expanded { 1. } else { 0. };
    let target_peek = if w.expanded || w.mode == shell_policy::Mode::Peek {
        1.
    } else {
        0.
    };
    if (w.expansion - target).abs() > 0.001
        || (w.peek - target_peek).abs() > 0.001
        || w.expansion_velocity.abs() > 0.01
        || w.peek_velocity.abs() > 0.01
        || w.overlay.is_some()
        || w.idle_transition.is_some()
        || w.canvas.as_ref().is_some_and(Canvas::glass_animating)
        || w.palette.active(now)
        || w.expanded && w.page == 2 && w.calendar.active()
        || (w.sidebar_scroll - w.sidebar_target_scroll).abs() > 0.1
        || w.hover_tick.is_some()
        || state.data.settings.motion
            && w.feedback
                .is_some_and(|(_, start)| start.elapsed().as_millis() < 350)
        || state.data.settings.motion && w.expanded && w.page == 3
        || state.data.settings.motion && w.expanded && w.page == 1 && state.running.is_some()
    {
        return Some(motion::FRAME);
    }
    let playing = state
        .media
        .as_ref()
        .is_some_and(|m| m.snapshot.lock().is_ok_and(|s| s.playing));
    if playing && (w.expanded && w.page == 0 || !w.expanded) {
        Some(if state.data.settings.motion {
            motion::FRAME
        } else {
            Duration::from_millis(250)
        })
    } else if state.data.settings.motion
        && !w.expanded
        && w.mode == shell_policy::Mode::Compact
        && w.battery.charging
    {
        Some(Duration::from_millis(33))
    } else {
        None
    }
}
pub(super) unsafe fn frame_step(hwnd: HWND, w: &mut Window) {
    let now = Instant::now();
    let dt = w.frame_tick.elapsed().as_secs_f32().min(0.05);
    w.frame_tick = now;
    let motion = w.shared.borrow().data.settings.motion;
    if w.feedback
        .is_some_and(|(_, start)| !motion || start.elapsed().as_millis() >= 350)
    {
        w.feedback = None;
    }
    if w.water_fill >= 0. {
        let state = w.shared.borrow();
        let remaining_ml = state
            .data
            .settings
            .water_goal
            .saturating_sub(state.data.water);
        let target = (remaining_ml as f32 / state.data.settings.water_goal as f32).clamp(0., 1.);
        if !motion || (w.water_fill - target).abs() < 0.001 {
            w.water_fill = target;
        } else {
            w.water_fill += (target - w.water_fill) * (1. - (-14. * dt).exp());
        }
    }
    let mut needs_region = false;
    if let Some(transition) = &mut w.idle_transition {
        let active = motion && transition.step(now);
        if !active {
            w.idle_transition = None;
        }
        needs_region = true;
    }

    let target = if w.expanded { 1. } else { 0. };
    if w.expansion != target || w.expansion_velocity != 0. {
        if motion {
            motion::settle(&mut w.expansion, &mut w.expansion_velocity, target, dt);
        } else {
            w.expansion = target;
            w.expansion_velocity = 0.;
        }
        needs_region = true;
    }

    let target_peek = if w.expanded || w.mode == shell_policy::Mode::Peek {
        1.
    } else {
        0.
    };
    if w.peek != target_peek || w.peek_velocity != 0. {
        if motion {
            motion::settle(&mut w.peek, &mut w.peek_velocity, target_peek, dt);
        } else {
            w.peek = target_peek;
            w.peek_velocity = 0.;
        }
        needs_region = true;
    }

    if needs_region {
        unsafe {
            region(hwnd, w);
        }
    }
    if let Some(hover_tick) = w.hover_tick
        && hover_tick.elapsed().as_millis() >= shell_policy::HOVER_LEAVE_MS as u128
    {
        w.hover_tick = None;
        w.hovered = false;
        unsafe {
            update_policy(hwnd, w);
        }
    }
    if w.overlay
        .as_ref()
        .is_some_and(|s| !motion || s.started.elapsed().as_secs_f32() >= s.duration)
    {
        w.overlay = None;
    }
    if w.calendar.active() {
        if motion {
            w.calendar.step(dt);
        } else {
            w.calendar.scroll = w.calendar.target_scroll;
        }
    }
    if (w.sidebar_scroll - w.sidebar_target_scroll).abs() > 0.1 {
        if motion {
            let t = (dt * 18.).clamp(0., 1.);
            w.sidebar_scroll += (w.sidebar_target_scroll - w.sidebar_scroll) * t;
        } else {
            w.sidebar_scroll = w.sidebar_target_scroll;
        }
    }
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
        let _ = UpdateWindow(hwnd);
    }
}
pub(super) unsafe fn sync_inputs(hwnd: HWND, w: &mut Window) {
    unsafe {
        if w.settings {
            return;
        }
        let s = w.shared.borrow().data.settings.clone();
        let show = w.expanded
            && w.expansion >= 0.98
            && w.overlay.as_ref().is_none_or(|overlay| !overlay.shell);
        let key = [
            w.page as i32,
            show as i32,
            s.width,
            w.calendar.form as i32,
            w.calendar.scroll.round() as i32,
            w.calendar.form_y as i32,
            panel_height(&s),
            0,
        ];
        if w.input_layout == Some(key) {
            return;
        }
        w.input_layout = Some(key);
        if !w.input.is_invalid() {
            input::place(w.input, input::task_bounds(s.width), show && w.page == 4);
        }
        for (i, edit) in w.event_inputs.into_iter().enumerate() {
            if edit.is_invalid() {
                continue;
            }
            let bounds = input::figma_event_bounds(s.width, i);
            let visible =
                show && w.page == 2 && w.calendar.form && input::in_view(bounds, host_height());
            input::place(edit, bounds, visible);
        }
        let _ = hwnd;
    }
}
pub(super) unsafe fn create_event_inputs(hwnd: HWND, w: &mut Window) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        input::configure(w.input, w.input_font, "Tambah tugas", 500);
        for (i, text) in ["", "09:00", "5", ""].into_iter().enumerate() {
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                &HSTRING::from(text),
                input::style(),
                0,
                0,
                100,
                36,
                Some(hwnd),
                None,
                Some(instance.into()),
                None,
            )?;
            input::configure(
                edit,
                w.input_font,
                ["Nama acara", "09:00", "5", "https://… (opsional)"][i],
                if i == 3 { 2048 } else { 200 },
            );
            SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
            w.event_inputs[i] = edit;
        }
        Ok(())
    }
}

pub(super) struct PrivateFont(windows::core::HSTRING);
impl PrivateFont {
    pub fn load() -> Result<Self> {
        let root = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("fonts")))
            .filter(|p| p.exists())
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")
            });
        let path = HSTRING::from(
            root.join("PlusJakartaSans-Medium.ttf")
                .to_string_lossy()
                .as_ref(),
        );
        if unsafe { AddFontResourceExW(&path, FR_PRIVATE, None) } == 0 {
            return Err(Error::new(
                E_FAIL,
                "Font Plus Jakarta Sans belum tersedia. Simpan folder fonts di samping Horizon.exe.",
            ));
        }
        Ok(Self(path))
    }
}
impl Drop for PrivateFont {
    fn drop(&mut self) {
        unsafe {
            let _ = RemoveFontResourceExW(&self.0, FR_PRIVATE.0, None);
        }
    }
}
