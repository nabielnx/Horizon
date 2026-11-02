//! Opt-in native fixture checks. They only run with --offscreen and never
//! control the user's media player or desktop.
use crate::*;

pub(super) fn running_media_diagnostics(window: &Window) -> Result<()> {
    let state = window.shared.borrow();
    let track = state
        .media
        .as_ref()
        .and_then(|media| media.snapshot.lock().ok().map(|track| track.clone()))
        .unwrap_or_default();
    let report = serde_json::json!({
        "pid": std::process::id(), "readOnly": true, "liveInstance": true,
        "source": track.source_app, "title": track.title, "artist": track.artist,
        "position": track.position_now(), "duration": track.duration,
        "coverBytes": track.cover.as_ref().map_or(0, |bytes| bytes.len()),
        "coverError": track.cover_error, "coverAttempts": track.cover_attempts,
        "coverRevision": track.cover_revision, "error": track.error,
        "coverRenderError": window.cover_render_error.borrow().as_str(),
        "batteryPercent": window.battery.percent, "batteryCharging": window.battery.charging,
        "lyricLines": track.lyrics.len(), "wordTimedLines": track.lyrics.iter().filter(|line| !line.words.is_empty()).count()
    });
    std::fs::create_dir_all("artifacts").map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    std::fs::write(
        "artifacts/running-media-diagnostics.json",
        serde_json::to_vec_pretty(&report).map_err(|e| Error::new(E_FAIL, e.to_string()))?,
    )
    .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    Ok(())
}

pub(super) fn media_diagnostics() -> Result<()> {
    let lyrics_network = std::env::args().any(|a| a == "--lyrics-network");
    let media = media::Media::start(lyrics_network);
    let mut samples = Vec::new();
    for second in 1..=6 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Ok(track) = media.snapshot.lock() {
            samples.push(serde_json::json!({"second":second,"source":track.source_app,"title":track.title,"artist":track.artist,"duration":track.duration,"playing":track.playing,"coverBytes":track.cover.as_ref().map_or(0, |b| b.len()),"coverRevision":track.cover_revision,"coverAttempts":track.cover_attempts,"coverError":track.cover_error,"lyricLines":track.lyrics.len(),"lyricsLoaded":track.lyrics_loaded,"lyricsRetryMs":track.lyrics_retry.map(|at| at.saturating_duration_since(Instant::now()).as_millis() as u64),"error":track.error}));
        }
    }
    std::fs::create_dir_all("artifacts").map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    let report = serde_json::json!({"readOnly":!lyrics_network,"mediaControlsUsed":false,"lyricsNetworkEnabled":lyrics_network,"lyricCacheMayBeUpdated":lyrics_network,"samples":samples});
    std::fs::write(
        "artifacts/media-diagnostics.json",
        serde_json::to_vec_pretty(&report).map_err(|e| Error::new(E_FAIL, e.to_string()))?,
    )
    .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
    Ok(())
}

pub(super) unsafe fn startup_checks(hwnd: HWND, w: &mut Window) -> Result<()> {
    unsafe {
        if !w.canvas.as_ref().is_some_and(Canvas::has_frame) {
            return Err(Error::new(
                E_FAIL,
                "Creation exposed an unfinished first frame",
            ));
        }
        use windows::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DwmGetWindowAttribute};
        let mut border = COLORREF(0);
        let border_supported = DwmGetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &mut border as *mut _ as *mut _,
            std::mem::size_of::<COLORREF>() as u32,
        )
        .is_ok();
        if border_supported && border.0 != 0xfffffffe {
            return Err(Error::new(E_FAIL, "System border was not suppressed"));
        }
        // Exercise the first hover from the actual hidden indicator, including
        // the first content frame before any animation time has elapsed.
        w.mode = shell_policy::Mode::Indicator;
        w.idle_transition = None;
        w.peek = 0.;
        set_mode(w, shell_policy::Mode::Peek);
        if w.idle_geometry()
            != shell_policy::Mode::Indicator
                .idle_geometry(w.shared.borrow().data.settings.width as f32)
        {
            return Err(Error::new(E_FAIL, "First hover jumped to compact bounds"));
        }
        w.paint()?;
        if !w
            .hits
            .iter()
            .any(|hit| matches!(hit.action, Action::Media(0)))
        {
            return Err(Error::new(
                E_FAIL,
                "First hover rendered compact content instead of peek",
            ));
        }
        w.idle_transition = None;
        let mut frames = 0;
        for _ in 0..3 {
            for mode in [shell_policy::Mode::Peek, shell_policy::Mode::Compact] {
                // Drive the same region/repaint path as hover, using a virtual
                // animation age so the probe needs no mouse input or sleeps.
                w.mode = mode;
                w.hovered = mode == shell_policy::Mode::Peek;
                for _ in 0..=36 {
                    w.frame_tick = Instant::now() - std::time::Duration::from_millis(17);
                    frame_step(hwnd, w);
                    let canvas = w
                        .canvas
                        .as_ref()
                        .ok_or_else(|| Error::new(E_FAIL, "Hover lost its render target"))?;
                    let _ = canvas.snapshot()?;
                    frames += 1;
                }
            }
        }
        let result = serde_json::json!({
            "offscreen": true,
            "desktopInputUsed": false,
            "desktopVisualVerified": false,
            "completedFrameOnCreation": true,
            "systemBorderSuppressed": if border_supported { Some(true) } else { None },
            "hoverCycles": 3,
            "indicatorOpensDirectlyToPeek": true,
            "hoverFramesRendered": frames,
            "hoverSnapshotsReadable": true
        });
        std::fs::write(
            "artifacts/startup-checks-0.3.json",
            serde_json::to_vec_pretty(&result).map_err(|e| Error::new(E_FAIL, e.to_string()))?,
        )
        .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        Ok(())
    }
}

pub(super) struct MotionProbe {
    last: Instant,
    samples: Vec<f64>,
    width: i32,
    height: i32,
    snapshots: usize,
}
impl MotionProbe {
    pub unsafe fn start(hwnd: HWND, w: &mut Window) -> Result<Self> {
        unsafe {
            w.shared.borrow_mut().media = Some(media::Media::fixture(media::Track {
                title: "Native motion fixture".into(),
                playing: true,
                duration: 300.,
                position: 120.,
                updated: Some(Instant::now()),
                palette: Some([0x442833, 0x302429, 0x19171a]),
                cover: Some(std::sync::Arc::new(
                    include_bytes!("../assets/fixtures/cover.png").to_vec(),
                )),
                ..Default::default()
            }));
            w.shared.borrow_mut().data.settings.motion = true;
            w.shared.borrow_mut().data.tasks.push(Task {
                text: "Motion fixture".into(),
                done: false,
                due: None,
                notified: false,
                priority: 0,
            });
            w.expanded = true;
            w.expansion = 1.;
            w.peek = 1.;
            w.page = 0;
            apply_frame(hwnd, w);
            let _ = UpdateWindow(hwnd);
            let mut bounds = RECT::default();
            GetClientRect(hwnd, &mut bounds)?;
            Ok(Self {
                last: Instant::now(),
                samples: Vec::with_capacity(120),
                width: bounds.right,
                height: bounds.bottom,
                snapshots: 0,
            })
        }
    }
    pub unsafe fn frame(&mut self, hwnd: HWND, w: &mut Window) -> Result<bool> {
        unsafe {
            let now = Instant::now();
            self.samples
                .push(now.duration_since(self.last).as_secs_f64() * 1000.);
            self.last = now;
            let count = self.samples.len();
            if count == 4
                && let Some(hit) = w
                    .hits
                    .iter()
                    .find(|h| h.action == Action::Media(0) && h.w <= 52 && h.h >= 32)
            {
                w.pointer = (hit.x + hit.w / 2, hit.y + hit.h / 2);
                w.paint()?;
                if !w.canvas.as_ref().is_some_and(Canvas::glass_animating) {
                    return Err(Error::new(
                        E_FAIL,
                        "Hover did not schedule interaction frames",
                    ));
                }
            }
            if count == 5 {
                w.pressed = true;
                w.paint()?;
            }
            if count == 6 {
                w.pressed = false;
                w.paint()?;
            }
            if count == 7 {
                w.pointer = (-1, -1);
                w.paint()?;
            }
            let mut bounds = RECT::default();
            GetClientRect(hwnd, &mut bounds)?;
            if bounds.right != self.width || bounds.bottom != self.height {
                return Err(Error::new(E_FAIL, "Host size changed during motion"));
            }
            if matches!(count, 8 | 16 | 24 | 32) {
                action(hwnd, w, Action::Page((count / 8) % 4));
                self.check_snapshot(w)?;
            }
            if matches!(count, 40 | 44 | 48 | 52) {
                w.expanded = !w.expanded;
                animate(hwnd, w);
                apply_frame(hwnd, w);
                self.check_snapshot(w)?;
            }
            if matches!(count, 60 | 80)
                && let Some(media) = &w.shared.borrow().media
                && let Ok(mut track) = media.snapshot.lock()
            {
                track.palette = Some(if count == 60 {
                    [0x542933, 0x342329, 0x20151a]
                } else {
                    [0x283b48, 0x222a32, 0x161c21]
                });
            }
            if count == 60 {
                action(hwnd, w, Action::Page(4));
                self.check_snapshot(w)?;
            }
            if matches!(count, 64 | 70) {
                action(hwnd, w, Action::ToggleTask(0));
            }
            if count == 82 {
                action(hwnd, w, Action::Page(0));
                self.check_snapshot(w)?;
            }
            if count < 120 {
                return Ok(false);
            }
            w.shared.borrow_mut().media = None;
            w.overlay = None;
            w.expanded = false;
            w.expansion = 0.;
            w.peek = 0.;
            w.peek_velocity = 0.;
            w.expansion_velocity = 0.;
            w.calendar.target_scroll = w.calendar.scroll;
            w.palette.retarget(render::palette(0), false, now);
            if frame_interval(w).is_some() {
                return Err(Error::new(E_FAIL, "Animation clock did not stop at idle"));
            }
            let mut sorted = self.samples.clone();
            sorted.sort_by(f64::total_cmp);
            let result = serde_json::json!({"offscreen":true,"desktopInputUsed":false,"realMediaUsed":false,"renderedFrames":count,"frameIntervalMedianMs":sorted[count/2],"frameIntervalP95Ms":sorted[(count*95/100).min(count-1)],"frameIntervalMaxMs":sorted[count-1],"snapshotsPreserved":self.snapshots,"hoverPressReleaseExercised":true,"taskCompletionReversed":true,"hostSizeStable":true,"clockStopsAtIdle":true,"samplesMs":self.samples});
            std::fs::write(
                "artifacts/motion-checks-0.3.json",
                serde_json::to_vec_pretty(&result)
                    .map_err(|e| Error::new(E_FAIL, e.to_string()))?,
            )
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
            Ok(true)
        }
    }
    fn check_snapshot(&mut self, w: &Window) -> Result<()> {
        if w.overlay.is_none() {
            return Err(Error::new(E_FAIL, "Transition lost its previous frame"));
        }
        self.snapshots += 1;
        Ok(())
    }
}
pub(super) unsafe fn calendar_checks(hwnd: HWND, w: &mut Window) -> Result<()> {
    unsafe {
        let tomorrow = Local::now().date_naive() + chrono::Duration::days(1);
        w.expanded = true;
        w.expansion = 1.;
        w.peek = 1.;
        w.page = 2;
        w.calendar.select(tomorrow);
        w.calendar.form = true;
        let _ = UpdateWindow(hwnd);
        for (h, text) in w.event_inputs.into_iter().zip([
            "Agenda pengujian",
            "09:30",
            "12",
            "https://example.com/meeting",
        ]) {
            SetWindowTextW(h, &HSTRING::from(text))?;
        }
        action(hwnd, w, Action::AddEvent);
        let state = w.shared.borrow();
        let Some(event) = state.data.events.first() else {
            return Err(Error::new(E_FAIL, "Calendar event was not added"));
        };
        if event.lead_minutes != Some(12) || event.url != "https://example.com/meeting" {
            return Err(Error::new(E_FAIL, "Calendar form fields were not saved"));
        }
        let loaded = Data::load(&state.store).map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        if loaded.events.len() != 1 || loaded.events[0].lead_minutes != Some(12) {
            return Err(Error::new(E_FAIL, "Calendar event did not survive reload"));
        }
        drop(state);
        w.paint()?;
        if w.hits
            .iter()
            .filter(|h| matches!(h.action, Action::Date(_)))
            .count()
            != 42
        {
            return Err(Error::new(
                E_FAIL,
                "Calendar does not show all 42 date cells",
            ));
        }
        w.calendar.scroll = w.calendar.form_y - 26.;
        w.calendar.target_scroll = w.calendar.scroll;
        w.paint()?;
        sync_inputs(hwnd, w);
        if !IsWindowVisible(w.event_inputs[0]).as_bool() {
            return Err(Error::new(
                E_FAIL,
                "Calendar title field is not accessible after scrolling",
            ));
        }
        action(hwnd, w, Action::DeleteEvent(0));
        if !w.shared.borrow().data.events.is_empty() {
            return Err(Error::new(E_FAIL, "Calendar event was not deleted"));
        }
        action(hwnd, w, Action::Today);
        if w.calendar.selected != Local::now().date_naive() {
            return Err(Error::new(E_FAIL, "Today button did not reset date"));
        }
        Ok(())
    }
}
