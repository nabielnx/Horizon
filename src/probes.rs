//! Opt-in native fixture checks. They only run with --offscreen and never
//! control the user's media player or desktop.
use crate::*;
pub(super) unsafe fn mascot_checks(hwnd: HWND, w: &mut Window) -> Result<()> {
    unsafe {
        let (media, running, alert, motion) = {
            let mut state = w.shared.borrow_mut();
            let saved = (
                state.media.take(),
                state.running.take(),
                state.alert.take(),
                state.data.settings.motion,
            );
            state.data.settings.motion = true;
            saved
        };
        let saved = (
            w.expanded,
            w.mode,
            w.page,
            w.expansion,
            w.peek,
            w.battery,
            w.offscreen,
        );
        w.expanded = false;
        w.mode = shell_policy::Mode::Compact;
        w.page = 0;
        w.expansion = 0.;
        w.peek = 0.;
        w.expansion_velocity = 0.;
        w.peek_velocity = 0.;
        w.overlay = None;
        w.idle_transition = None;
        w.hover_tick = None;
        w.battery = power::Battery::default();
        w.power_notice = None;
        w.companion = mascot::Companion::default();
        w.lyric_key = None;
        // Exercise the real clock decision on an image target; the owned host
        // remains offscreen throughout, with no desktop input or live media.
        w.offscreen = false;
        let width = w.shared.borrow().data.settings.width;
        w.canvas = Some(Canvas::image(width as u32, host_height() as u32)?);
        w.paint()?;
        w.palette
            .retarget(render::palette(0), false, Instant::now());
        if !w.companion_visible()
            || frame_interval(w).is_none()
            || !w.hits.iter().any(|h| h.action == Action::PetMascot)
        {
            return Err(Error::new(
                E_FAIL,
                "Idle companion or animation clock is missing",
            ));
        }
        let mascot_hit = w
            .hits
            .iter()
            .find(|h| h.action == Action::PetMascot)
            .unwrap();
        let compact_width = w
            .compact_width(w.island_style.sample(Instant::now()))
            .round() as i32;
        if mascot_hit.x != (width - compact_width) / 2
            || mascot_hit.y != 0
            || mascot_hit.h
                != layout::compact_height(w.island_style.sample(Instant::now())).round() as i32
            || w.hits
                .iter()
                .any(|h| !matches!(h.action, Action::PetMascot | Action::OpenAlert))
        {
            return Err(Error::new(
                E_FAIL,
                "Quiet compact scene must contain mascot and clock",
            ));
        }
        action(hwnd, w, Action::PetMascot);
        if !w.companion.reacting() || w.expanded {
            return Err(Error::new(
                E_FAIL,
                "Petting did not react independently of shell navigation",
            ));
        }
        w.shared.borrow_mut().media = Some(media::Media::fixture(media::Track {
            title: "Companion activity fixture".into(),
            playing: true,
            ..Default::default()
        }));
        if w.companion_visible() {
            return Err(Error::new(E_FAIL, "Companion displaced active music"));
        }
        w.shared.borrow_mut().media = None;
        w.shared.borrow_mut().running = Some(Instant::now());
        if w.companion_visible() {
            return Err(Error::new(E_FAIL, "Companion displaced active focus"));
        }
        w.shared.borrow_mut().running = None;
        for mode in [shell_policy::Mode::Hidden, shell_policy::Mode::Indicator] {
            w.mode = mode;
            if w.companion_visible() || frame_interval(w).is_some() {
                return Err(Error::new(E_FAIL, "Companion keeps hidden shell animating"));
            }
        }
        w.mode = shell_policy::Mode::Peek;
        w.peek = 1.;
        w.lyric_key = None;
        w.paint()?;
        if !w.hits.iter().any(|h| h.action == Action::PetMascot)
            || w.hits.iter().any(|h| matches!(h.action, Action::Media(_)))
        {
            return Err(Error::new(
                E_FAIL,
                "Empty medium companion overlaps playback action",
            ));
        }
        // Connection edges may overlay status, but must not activate the last
        // visited page, replace the mascot, or expose compact playback controls.
        w.mode = shell_policy::Mode::Compact;
        w.peek = 0.;
        w.page = 1; // Reproduce the user's last-opened focus page.
        for context in 0..3 {
            {
                let mut state = w.shared.borrow_mut();
                state.media = (context == 1).then(|| {
                    media::Media::fixture(media::Track {
                        title: "Power-edge fixture".into(),
                        playing: true,
                        updated: Some(Instant::now()),
                        ..Default::default()
                    })
                });
                state.running = (context == 2).then(Instant::now);
            }
            sync_compact_activity(w);
            w.battery = power::Battery {
                percent: Some(80),
                plugged: Some(true),
                charging: false,
            };
            for plugged in [false, true, false, true] {
                let before = {
                    let state = w.shared.borrow();
                    w.compact_activity
                        .page(None, state.running.is_some(), context == 1, w.page)
                };
                if !update_battery(
                    w,
                    power::Battery {
                        plugged: Some(plugged),
                        ..w.battery
                    },
                ) {
                    return Err(Error::new(E_FAIL, "Simulated charger edge was ignored"));
                }
                sync_compact_activity(w);
                w.canvas = Some(Canvas::image(width as u32, host_height() as u32)?);
                w.paint()?;
                let state = w.shared.borrow();
                let after =
                    w.compact_activity
                        .page(None, state.running.is_some(), context == 1, w.page);
                drop(state);
                if before != after
                    || w.companion_visible() != (context == 0)
                    || w.hits
                        .iter()
                        .any(|h| !matches!(h.action, Action::PetMascot | Action::OpenAlert))
                {
                    return Err(Error::new(
                        E_FAIL,
                        format!(
                            "Charger edge changed compact identity or exposed a control: context={context}, plugged={plugged}, page={before}->{after}, mascot={}, mode={:?}, expanded={}, hits={:?}",
                            w.companion_visible(),
                            w.mode,
                            w.expanded,
                            w.hits
                                .iter()
                                .filter(|h| !matches!(
                                    h.action,
                                    Action::PetMascot | Action::OpenAlert
                                ))
                                .count()
                        ),
                    ));
                }
            }
        }
        w.shared.borrow_mut().media = None;
        w.shared.borrow_mut().running = None;
        w.power_notice = None;
        w.mode = shell_policy::Mode::Compact;
        w.peek = 0.;
        w.shared.borrow_mut().data.settings.motion = false;
        sync_compact_activity(w);
        w.compact_activity_progress = w.compact_activity_size.sample(Instant::now());
        w.overlay = None;
        w.paint()?;
        if frame_interval(w).is_some() {
            return Err(Error::new(
                E_FAIL,
                "Disabled motion keeps companion clock running",
            ));
        }
        if GetClassLongPtrW(hwnd, GCLP_HICON) == 0 {
            return Err(Error::new(
                E_FAIL,
                "Embedded Horizon application icon is missing",
            ));
        }
        {
            let mut state = w.shared.borrow_mut();
            state.media = media;
            state.running = running;
            state.alert = alert;
            state.data.settings.motion = motion;
        }
        (
            w.expanded,
            w.mode,
            w.page,
            w.expansion,
            w.peek,
            w.battery,
            w.offscreen,
        ) = saved;
        w.overlay = None;
        w.canvas = None;
        apply_frame(hwnd, w);
        let report = serde_json::json!({"offscreen":true,"desktopInputUsed":false,"liveMediaUsed":false,"idleMascotVisible":true,"quietCompactMascotClockAndBattery":true,"groundedCompactBounds":true,"petReaction":true,"compactHasNoPlaybackControls":true,"chargerEdgesPreserveMusicFocusAndMascot":true,"activeMusicAndFocusKeepTheirIcons":true,"mediumEmptyStateHasNoDeadPlaybackAction":true,"hiddenClockStops":true,"reducedMotionClockStops":true,"embeddedWindowIcon":true});
        std::fs::write(
            "artifacts/mascot-checks-0.3.json",
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        Ok(())
    }
}

pub(super) unsafe fn volume_input_checks(hwnd: HWND) -> Result<()> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Window;
        if !(*ptr).offscreen {
            return Err(Error::new(
                E_FAIL,
                "Volume fixture requires an offscreen window",
            ));
        }
        let shared = (*ptr).shared.clone();
        let (media, volume) = {
            let mut state = shared.borrow_mut();
            let saved = (state.media.take(), state.spotify_volume.take());
            state.media = Some(media::Media::fixture(media::Track {
                source_app: "Spotify.exe".into(),
                title: "Volume input fixture".into(),
                ..Default::default()
            }));
            state.spotify_volume = Some(audio::SpotifyVolume::fixture(0.65));
            saved
        };
        let saved = (
            (*ptr).expanded,
            (*ptr).mode,
            (*ptr).page,
            (*ptr).expansion,
            (*ptr).peek,
            (*ptr).focused,
            (*ptr).pointer,
        );
        (*ptr).expanded = true;
        (*ptr).mode = shell_policy::Mode::Expanded;
        (*ptr).page = 0;
        (*ptr).expansion = 1.;
        (*ptr).peek = 1.;
        (*ptr).overlay = None;
        let width = shared.borrow().data.settings.width;
        (*ptr).canvas = Some(Canvas::image(width as u32, host_height() as u32)?);
        let saved_layout = {
            let mut state = shared.borrow_mut();
            let settings = &mut state.data.settings;
            let saved = (settings.show_lyrics, settings.motion);
            settings.show_lyrics = true;
            settings.motion = true;
            saved
        };
        let volume_hit = |window: &Window| {
            window
                .hits
                .iter()
                .find(|hit| hit.action == Action::SpotifyVolume)
                .map(|hit| hit.w)
                .ok_or_else(|| Error::new(E_FAIL, "Volume slider missing from fixture"))
        };
        (*ptr).paint()?;
        let split_width = volume_hit(&*ptr)?;
        shared.borrow_mut().data.settings.show_lyrics = false;
        (*ptr).paint()?;
        if !(*ptr).canvas.as_ref().is_some_and(Canvas::glass_animating) {
            return Err(Error::new(E_FAIL, "Music columns did not start animating"));
        }
        shared.borrow_mut().data.settings.motion = false;
        (*ptr).paint()?;
        if volume_hit(&*ptr)? <= split_width {
            return Err(Error::new(E_FAIL, "Player did not widen with lyrics off"));
        }
        shared.borrow_mut().data.settings.motion = true;
        action(hwnd, &mut *ptr, Action::SpotifySearchPanel);
        // Sample a frame during the reveal without waiting on the system clock.
        let halfway = Instant::now() - std::time::Duration::from_millis(65);
        (*ptr).spotify_search_reveal = motion::Tween::new(0., halfway);
        (*ptr)
            .spotify_search_reveal
            .retarget(1., motion::PAGE, true, halfway);
        (*ptr).paint()?;
        if !(*ptr).spotify_search_reveal.active(Instant::now())
            || (*ptr).spotify_search_input_ready(true)
            || (*ptr)
                .hits
                .iter()
                .any(|hit| hit.action == Action::SpotifySearchSubmit)
        {
            return Err(Error::new(
                E_FAIL,
                "Spotify search did not animate before enabling input",
            ));
        }
        shared.borrow_mut().data.settings.motion = false;
        action(hwnd, &mut *ptr, Action::SpotifySearchPanel);
        (*ptr).paint()?;
        action(hwnd, &mut *ptr, Action::SpotifySearchPanel);
        (*ptr).paint()?;
        let search = &(*ptr).spotify_search;
        let has_action =
            |window: &Window, action| window.hits.iter().any(|hit| hit.action == action);
        if search.edit.is_invalid()
            || !has_action(&*ptr, Action::SpotifySearchSubmit)
            || !has_action(&*ptr, Action::SpotifySearchWeb)
            || !has_action(&*ptr, Action::SpotifySearchSetup)
            || has_action(&*ptr, Action::SpotifySearchCopyRedirect)
            || volume_hit(&*ptr)? != split_width
        {
            return Err(Error::new(
                E_FAIL,
                "Spotify search did not reserve the lyrics lane",
            ));
        }
        let field = spotify_ui::edit_bounds(width, spotify::SearchMode::Free);
        let search_hit = |action| {
            (*ptr)
                .hits
                .iter()
                .find(|hit| hit.action == action)
                .ok_or_else(|| Error::new(E_FAIL, "Missing Spotify search control"))
        };
        let desktop = search_hit(Action::SpotifySearchSubmit)?;
        let browser = search_hit(Action::SpotifySearchWeb)?;
        let premium = search_hit(Action::SpotifySearchSetup)?;
        if desktop.y < field.y + field.height + 8
            || desktop.y + desktop.h >= browser.y
            || browser.x + browser.w + 4 > premium.x
            || browser.y != premium.y
            || browser.y + browser.h > host_height() - 20
        {
            return Err(Error::new(E_FAIL, "Spotify search controls overlap"));
        }
        action(hwnd, &mut *ptr, Action::SpotifySearchSetup);
        (*ptr).paint()?;
        if !has_action(&*ptr, Action::SpotifySearchCopyRedirect) {
            return Err(Error::new(
                E_FAIL,
                "Optional Spotify setup is missing its redirect URI",
            ));
        }
        (*ptr).spotify_search.mode = spotify::SearchMode::Connected;
        (*ptr).spotify_search.songs = vec![spotify::Song {
            id: "0123456789012345678901".into(),
            title: "Search result fixture".into(),
            artist: "Fixture artist".into(),
        }];
        (*ptr).paint()?;
        if !has_action(&*ptr, Action::SpotifySearchPlay(0))
            || !has_action(&*ptr, Action::SpotifySearchLink(0))
        {
            return Err(Error::new(
                E_FAIL,
                "Spotify result is missing playback or its source link",
            ));
        }
        action(hwnd, &mut *ptr, Action::SpotifySearchPanel);
        (*ptr).spotify_search.songs.clear();
        shared.borrow_mut().data.settings.show_lyrics = true;
        (*ptr).paint()?;
        if volume_hit(&*ptr)? != split_width {
            return Err(Error::new(
                E_FAIL,
                "Player did not return to its split width",
            ));
        }
        {
            let mut state = shared.borrow_mut();
            (state.data.settings.show_lyrics, state.data.settings.motion) = saved_layout;
        }
        (*ptr).paint()?;
        let index = (*ptr)
            .hits
            .iter()
            .position(|h| h.action == Action::SpotifyVolume)
            .ok_or_else(|| Error::new(E_FAIL, "Volume slider missing from fixture"))?;
        let hit = (&*ptr).hits.get(index).unwrap();
        let point = LPARAM(((hit.y + hit.h / 2) as isize) << 16 | (hit.x + hit.w / 2) as isize);
        for end in [WM_LBUTTONUP, WM_CAPTURECHANGED] {
            (*ptr).focused = Some(index); // Existing keyboard focus switches to pointer input.
            SendMessageW(hwnd, WM_LBUTTONDOWN, Some(WPARAM(1)), Some(point));
            let dragging = (*ptr).volume_drag.is_some();
            let pointer_outline = (*ptr).focused.is_some();
            SendMessageW(hwnd, end, Some(WPARAM(0)), Some(point));
            let _ = ReleaseCapture();
            if !dragging
                || pointer_outline
                || (*ptr).focused.is_some()
                || (*ptr).volume_drag.is_some()
                || (*ptr).pressed
            {
                return Err(Error::new(
                    E_FAIL,
                    "Volume pointer input left a focus outline or stuck drag",
                ));
            }
        }
        for _ in 0..=index {
            SendMessageW(hwnd, WM_KEYDOWN, Some(WPARAM(9)), Some(LPARAM(0)));
        }
        if (*ptr).focused != Some(index) {
            return Err(Error::new(
                E_FAIL,
                "Volume slider lost keyboard Tab navigation",
            ));
        }
        SendMessageW(hwnd, WM_KILLFOCUS, Some(WPARAM(0)), Some(LPARAM(0)));
        if (*ptr).focused.is_some() {
            return Err(Error::new(
                E_FAIL,
                "Inactive window retained its keyboard outline",
            ));
        }
        {
            let mut state = shared.borrow_mut();
            state.media = media;
            state.spotify_volume = volume;
        }
        (
            (*ptr).expanded,
            (*ptr).mode,
            (*ptr).page,
            (*ptr).expansion,
            (*ptr).peek,
            (*ptr).focused,
            (*ptr).pointer,
        ) = saved;
        (*ptr).canvas = None;
        (*ptr).overlay = None;
        apply_frame(hwnd, &mut *ptr);
        std::fs::write("artifacts/volume-input-checks.json", r#"{"offscreen":true,"liveAudioUsed":false,"desktopInputUsed":false,"mouseDragHasNoFocusOutline":true,"releaseAndCaptureLossClearDrag":true,"keyboardTabRetainsFocusOutline":true,"windowFocusLossClearsOutline":true}"#)
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        Ok(())
    }
}

pub(super) fn running_media_diagnostics(root: HWND, window: &Window) -> Result<()> {
    let state = window.shared.borrow();
    let track = state
        .media
        .as_ref()
        .and_then(|media| media.snapshot.lock().ok().map(|track| track.clone()))
        .unwrap_or_default();
    let spotify_volume = state.spotify_volume.as_ref().and_then(|v| v.snapshot.lock().ok().map(|s| serde_json::json!({"level":s.level,"muted":s.muted,"sessions":s.sessions,"error":s.error})));
    let mut report = serde_json::json!({
        "pid": std::process::id(), "readOnly": true, "liveInstance": true,
        "firstFrameMs": window.first_frame_ms, "entranceActive": window.entrance.is_some(),
        "shellMode": format!("{:?}", window.mode), "shellExpanded": window.expanded,
        "shellHovered": window.hovered, "taskbarOverlay": state.data.settings.taskbar_layout,
        "shellVisible": unsafe { IsWindowVisible(root).as_bool() },
        "shellForeground": unsafe { let fg = shell_policy::foreground(root, state.settings_window); serde_json::json!({"otherApp":fg.other_app,"fullscreen":fg.fullscreen}) },
        "source": track.source_app, "title": track.title, "artist": track.artist, "album": track.album,
        "position": track.position_now(), "duration": track.duration,
        "coverBytes": track.cover.as_ref().map_or(0, |bytes| bytes.len()),
        "coverError": track.cover_error, "coverAttempts": track.cover_attempts,
        "coverRevision": track.cover_revision, "error": track.error,
        "recentCoverFailures": track.cover_failures,
        "coverProvider": if track.cover_source_url.is_some() { "Apple Music" } else { "Windows" },
        "coverRenderError": window.cover_render_error.borrow().as_str(),
        "batteryPercent": window.battery.percent, "batteryCharging": window.battery.charging,
        "chargerConnected": window.battery.plugged,
        "powerConnectionChanges": window.power_changes, "lastPowerConnectionChange": window.last_power_change,
        "powerNoticeVisible": window.power_notice.is_some(),
        "powerEventsRegistered": window.power_notification.is_some(),
        "compactContextPage": window.compact_activity.page(state.alert.as_ref().map(|alert| alert.page), state.running.is_some(), track.playing, window.page),
        "compactIndicator": format!("{:?}",window.compact_activity.indicator(state.running.is_some(),&track,state.alert.as_ref().map(|a|a.page),window.page)),
        "focusRunning": state.running.is_some(),
        "lyricLines": track.lyrics.len(), "wordTimedLines": track.lyrics.iter().filter(|line| !line.words.is_empty()).count(),
        "lyricsStatus": track.lyrics_status, "lyricsAttempts": track.lyrics_attempts,
        "lyricsRetryMs": track.lyrics_retry.map(|at| at.saturating_duration_since(Instant::now()).as_millis() as u64)
    });
    report["spotifyVolume"] = spotify_volume.unwrap_or(serde_json::Value::Null);
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
    let media = media::Media::start(lyrics_network, false);
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

pub(super) fn cover_fallback_diagnostics() -> Result<()> {
    unsafe {
        let _ = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
        );
    }
    let media = media::Media::start(false, false);
    std::thread::sleep(std::time::Duration::from_secs(2));
    let track = media
        .snapshot
        .lock()
        .map_err(|_| Error::new(E_FAIL, "Media snapshot unavailable"))?
        .clone();
    let job = crate::cover_fallback::Job {
        key: media::track_cover_key(&track),
        generation: track.generation,
        title: track.title,
        artist: track.artist,
        album: track.album,
        duration: track.duration,
    };
    let result = crate::cover_fallback::lookup(&job, &|| true);
    let report = match result {
        Ok(art) => {
            serde_json::json!({"matched":true, "bytes":art.bytes.len(), "sourceLink":art.link})
        }
        Err(error) => {
            serde_json::json!({"matched":false, "error":error, "title":job.title,"artist":job.artist,"album":job.album,"duration":job.duration})
        }
    };
    std::fs::write(
        "artifacts/cover-fallback-probe.json",
        serde_json::to_vec_pretty(&report).unwrap(),
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
        if w.canvas.as_ref().map(Canvas::height) != Some(layout::COMPACT_BUFFER_HEIGHT) {
            return Err(Error::new(
                E_FAIL,
                "Compact startup retained a full-size buffer",
            ));
        }
        let floating = w.shared.borrow().data.settings.dynamic_island;
        let expected_width = layout::compact_activity_width(f32::from(floating), 0.);
        if (shell_outline(w).width - expected_width).abs() > 0.01 {
            return Err(Error::new(
                E_FAIL,
                "First compact frame has a different width",
            ));
        }
        if (shell_outline(w).height - layout::compact_height(f32::from(floating))).abs() > 0.01 {
            return Err(Error::new(
                E_FAIL,
                "First compact frame has a different height",
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
        apply_frame(hwnd, w);
        let _ = UpdateWindow(hwnd);
        if !w
            .canvas
            .as_ref()
            .is_some_and(|canvas| canvas.height() == host_height() as u32 && canvas.has_frame())
        {
            return Err(Error::new(
                E_FAIL,
                "Peek did not repaint after growing its buffer",
            ));
        }
        if !w.hits.iter().any(|hit| hit.action == Action::Focus)
            || !w.hits.iter().any(|hit| hit.action == Action::PetMascot)
        {
            return Err(Error::new(
                E_FAIL,
                "First hover rendered compact content instead of peek",
            ));
        }
        w.idle_transition = None;
        // Simulate AC edges at the window boundary; this does not change the
        // machine's battery state or control another application.
        let saved_battery = w.battery;
        w.battery = power::Battery {
            percent: Some(80),
            plugged: Some(false),
            charging: false,
        };
        let connected = power::Battery {
            plugged: Some(true),
            ..w.battery
        };
        if !update_battery(w, connected) || !w.battery.energized() || w.power_notice.is_none() {
            return Err(Error::new(E_FAIL, "Plug-in did not show a power notice"));
        }
        w.mode = shell_policy::Mode::Compact;
        w.paint()?;
        if !update_battery(
            w,
            power::Battery {
                plugged: Some(false),
                ..connected
            },
        ) || w.battery.energized()
            || w.power_changes != 2
            || w.power_notice.is_none()
        {
            return Err(Error::new(E_FAIL, "Unplug did not show a power notice"));
        }
        w.paint()?;
        w.battery = saved_battery;
        w.power_notice = None;
        w.power_changes = 0;
        w.last_power_change = None;
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
        let saved_motion = w.shared.borrow().data.settings.motion;
        w.shared.borrow_mut().data.settings.motion = true;
        let mut previous = 0;
        for age in [0., 0.15, 0.6] {
            w.entrance = Instant::now().checked_sub(std::time::Duration::from_secs_f32(age));
            frame_step(hwnd, w);
            if (shell_outline(w).width - expected_width).abs() > 0.01 {
                return Err(Error::new(E_FAIL, "Compact width changed during entrance"));
            }
            let mut alpha = 0;
            GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), None)?;
            if alpha < previous {
                return Err(Error::new(E_FAIL, "Entrance opacity moved backwards"));
            }
            previous = alpha;
        }
        if w.entrance.is_some() || previous != surface_opacity(w, &w.shared.borrow().data.settings)
        {
            return Err(Error::new(
                E_FAIL,
                "Entrance did not restore the configured opacity",
            ));
        }
        let active_width = layout::compact_width(f32::from(floating));
        let mut fixture = media::Track {
            title: "Compact width fixture".into(),
            playing: true,
            ..Default::default()
        };
        let mut last_width = expected_width;
        for playing in [true, false] {
            fixture.playing = playing;
            w.shared.borrow_mut().media = Some(media::Media::fixture(fixture.clone()));
            sync_compact_activity(w);
            let frame = Instant::now();
            let middle = w
                .compact_activity_size
                .sample(frame + std::time::Duration::from_millis(70));
            let middle_width = layout::compact_activity_width(f32::from(floating), middle);
            if !(expected_width < middle_width && middle_width < active_width) {
                return Err(Error::new(
                    E_FAIL,
                    "Compact activity skipped its intermediate width",
                ));
            }
            // Retarget from the visible endpoint before exercising the reverse direction.
            let end = if playing { 1. } else { 0. };
            if (shell_outline(w).width - last_width).abs() > 0.01 {
                return Err(Error::new(E_FAIL, "Compact width jumped before animation"));
            }
            w.compact_activity_size
                .retarget(end, motion::SHELL, false, frame);
            frame_step(hwnd, w);
            last_width = if playing {
                active_width
            } else {
                expected_width
            };
            if (shell_outline(w).width - last_width).abs() > 0.01 {
                return Err(Error::new(E_FAIL, "Compact width did not reach its target"));
            }
        }
        w.shared.borrow_mut().media = None;
        w.shared.borrow_mut().data.settings.motion = false;
        w.entrance = Some(Instant::now());
        frame_step(hwnd, w);
        if w.entrance.is_some() {
            return Err(Error::new(E_FAIL, "Reduced motion did not skip entrance"));
        }
        w.shared.borrow_mut().data.settings.motion = saved_motion;
        let result = serde_json::json!({
            "offscreen": true,
            "firstFrameMs": w.first_frame_ms,
            "firstCompactWidth": expected_width,
            "activeCompactWidth": active_width,
            "dynamicIsland": floating,
            "entranceOpacityAndReducedMotionVerified": true,
            "desktopInputUsed": false,
            "desktopVisualVerified": false,
            "completedFrameOnCreation": true,
            "systemBorderSuppressed": if border_supported { Some(true) } else { None },
            "hoverCycles": 3,
            "indicatorOpensDirectlyToPeek": true,
            "plugUnplugIndicatorAndNoticesVerified": true,
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
                samples: Vec::with_capacity(180),
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
            // Include the spring's settling tail after the final page change.
            // Do not clear interaction state to manufacture an idle result.
            if count < 180 {
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
