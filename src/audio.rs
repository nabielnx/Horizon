//! Spotify-only Windows audio session volume. Never controls endpoint/master volume.
use std::{
    sync::{Arc, Mutex, atomic::AtomicUsize, mpsc},
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, S_OK},
        Media::Audio::*,
        System::{
            Com::*,
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    },
    core::*,
};

// Hit bounds include the thumb's radius at both ends of the 210px rail.
pub fn slider_level(mouse_x: i32, hit_x: i32, hit_width: i32) -> Option<f32> {
    if hit_width <= 0 {
        return None;
    }
    let padding = hit_width as f32 * 5. / 220.;
    Some(
        ((mouse_x as f32 - hit_x as f32 - padding) / (hit_width as f32 - padding * 2.))
            .clamp(0., 1.),
    )
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub level: Option<f32>,
    pub muted: bool,
    pub sessions: usize,
    pub error: String,
}
#[derive(Clone, Copy)]
enum Command {
    Level(f32),
    Mute(bool),
}
#[derive(Default)]
struct Pending {
    level: Option<f32>,
    muted: Option<bool>,
}
impl Pending {
    fn push(&mut self, command: Command) {
        match command {
            Command::Level(level) if level.is_finite() => {
                self.level = Some(level.clamp(0., 1.));
                if level > 0. {
                    self.muted = Some(false);
                }
            }
            Command::Mute(muted) => self.muted = Some(muted),
            _ => {}
        }
    }
}
pub struct SpotifyVolume {
    pub snapshot: Arc<Mutex<Snapshot>>,
    send: mpsc::Sender<Command>,
}
fn allowed_session(status: HRESULT, pid: u32, system: bool, path: &str) -> bool {
    status == S_OK
        && pid != 0
        && !system
        && path
            .rsplit(['\\', '/'])
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("Spotify.exe"))
}
unsafe fn process_path(pid: u32) -> Result<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        result?;
        Ok(String::from_utf16_lossy(&buffer[..size as usize]))
    }
}
unsafe fn sessions() -> Result<Vec<ISimpleAudioVolume>> {
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let endpoints = devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut matches = Vec::new();
        for index in 0..endpoints.GetCount()? {
            let Ok(endpoint) = endpoints.Item(index) else {
                continue;
            };
            let Ok(manager) = endpoint.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                continue;
            };
            let Ok(list) = manager.GetSessionEnumerator() else {
                continue;
            };
            for index in 0..list.GetCount()? {
                let Ok(session) = list
                    .GetSession(index)
                    .and_then(|s| s.cast::<IAudioSessionControl2>())
                else {
                    continue;
                };
                if session
                    .GetState()
                    .is_ok_and(|state| state == AudioSessionStateExpired)
                {
                    continue;
                }
                let mut pid = 0;
                // Keep the HRESULT: AUDCLNT_S_NO_SINGLE_PROCESS must be rejected,
                // since a shared session could also affect another application.
                let status = (session.vtable().GetProcessId)(session.as_raw(), &mut pid);
                if status != S_OK || pid == 0 || session.IsSystemSoundsSession() == S_OK {
                    continue;
                }
                let Ok(path) = process_path(pid) else {
                    continue;
                };
                if allowed_session(status, pid, false, &path)
                    && let Ok(volume) = session.cast::<ISimpleAudioVolume>()
                {
                    matches.push(volume);
                }
            }
        }
        Ok(matches)
    }
}
unsafe fn refresh(pending: &Pending) -> Result<Snapshot> {
    unsafe {
        let sessions = sessions()?;
        let mut snapshot = Snapshot {
            muted: !sessions.is_empty(),
            ..Snapshot::default()
        };
        for volume in sessions {
            let result = (|| -> Result<(f32, bool)> {
                if let Some(level) = pending.level {
                    volume.SetMasterVolume(level, std::ptr::null())?;
                }
                if let Some(muted) = pending.muted {
                    volume.SetMute(muted, std::ptr::null())?;
                }
                Ok((volume.GetMasterVolume()?, volume.GetMute()?.as_bool()))
            })();
            match result {
                Ok((level, muted)) => {
                    snapshot.level = Some(snapshot.level.map_or(level, |old| old.max(level)));
                    snapshot.muted &= muted;
                    snapshot.sessions += 1;
                }
                Err(error) => snapshot.error = error.to_string(),
            }
        }
        Ok(snapshot)
    }
}
impl SpotifyVolume {
    // Only used by the explicit offscreen renderer. Never touches audio devices.
    pub fn fixture(level: f32) -> Self {
        let (send, _) = mpsc::channel();
        Self {
            snapshot: Arc::new(Mutex::new(Snapshot {
                level: Some(level),
                sessions: 1,
                ..Snapshot::default()
            })),
            send,
        }
    }
    pub fn start(window: usize) -> Self {
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let state = snapshot.clone();
        let (send, commands) = mpsc::channel();
        std::thread::spawn(move || unsafe {
            if let Err(error) = CoInitializeEx(None, COINIT_MULTITHREADED).ok() {
                if let Ok(mut data) = state.lock() {
                    data.error = error.to_string();
                }
                crate::media::notify(&AtomicUsize::new(window));
                return;
            }
            let mut pending = Pending::default();
            loop {
                let data = refresh(&pending).unwrap_or_else(|error| Snapshot {
                    error: error.to_string(),
                    ..Snapshot::default()
                });
                if let Ok(mut current) = state.lock()
                    && *current != data
                {
                    *current = data;
                    crate::media::notify(&AtomicUsize::new(window));
                }
                pending = Pending::default();
                match commands.recv_timeout(Duration::from_secs(2)) {
                    Ok(command) => pending.push(command),
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                for command in commands.try_iter() {
                    pending.push(command);
                }
            }
            CoUninitialize();
        });
        Self { snapshot, send }
    }
    pub fn set_level(&self, level: f32) {
        if !level.is_finite() {
            return;
        }
        if let Ok(mut data) = self.snapshot.lock()
            && data.level.is_some()
        {
            let level = level.clamp(0., 1.);
            if self.send.send(Command::Level(level)).is_ok() {
                data.level = Some(level);
                if level > 0. {
                    data.muted = false;
                }
            }
        }
    }
    pub fn toggle_mute(&self) {
        if let Ok(mut data) = self.snapshot.lock()
            && data.level.is_some()
            && self.send.send(Command::Mute(!data.muted)).is_ok()
        {
            data.muted = !data.muted;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn targeting_rejects_other_apps_system_sounds_and_cross_process_sessions() {
        assert!(allowed_session(S_OK, 42, false, r"C:\Apps\Spotify.exe"));
        assert!(allowed_session(S_OK, 42, false, r"C:\Apps\SPOTIFY.EXE"));
        for path in [
            r"C:\Apps\chrome.exe",
            r"C:\Spotify.exe\other.exe",
            "SpotifyHelper.exe",
            "",
        ] {
            assert!(!allowed_session(S_OK, 42, false, path));
        }
        assert!(!allowed_session(S_OK, 0, false, "Spotify.exe"));
        assert!(!allowed_session(S_OK, 42, true, "Spotify.exe"));
        assert!(!allowed_session(
            HRESULT(1), // Any non-S_OK success status, including shared sessions.
            42,
            false,
            "Spotify.exe"
        ));
    }
    #[test]
    #[ignore = "Explicit live Spotify volume check; changes by 1% then restores all Spotify sessions"]
    fn live_spotify_volume_changes_only_spotify_sessions_and_restores_them() {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();
            let endpoints = devices
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .unwrap();
            let mut controls = Vec::new();
            for index in 0..endpoints.GetCount().unwrap() {
                let endpoint = endpoints.Item(index).unwrap();
                let Ok(manager) = endpoint.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)
                else {
                    continue;
                };
                let list = manager.GetSessionEnumerator().unwrap();
                for index in 0..list.GetCount().unwrap() {
                    let Ok(session) = list
                        .GetSession(index)
                        .and_then(|s| s.cast::<IAudioSessionControl2>())
                    else {
                        continue;
                    };
                    let Ok(volume) = session.cast::<ISimpleAudioVolume>() else {
                        continue;
                    };
                    let mut pid = 0;
                    let status = (session.vtable().GetProcessId)(session.as_raw(), &mut pid);
                    let path = if pid == 0 {
                        String::new()
                    } else {
                        process_path(pid).unwrap_or_default()
                    };
                    let spotify = allowed_session(
                        status,
                        pid,
                        session.IsSystemSoundsSession() == S_OK,
                        &path,
                    );
                    if let (Ok(level), Ok(muted)) = (volume.GetMasterVolume(), volume.GetMute()) {
                        controls.push((volume, level, muted.as_bool(), spotify));
                    }
                }
            }
            let original = controls
                .iter()
                .find(|r| r.3)
                .expect("Spotify must have an audio session")
                .1;
            struct Restore(Vec<(ISimpleAudioVolume, f32, bool)>);
            impl Drop for Restore {
                fn drop(&mut self) {
                    unsafe {
                        for (volume, level, muted) in &self.0 {
                            let _ = volume.SetMasterVolume(*level, std::ptr::null());
                            let _ = volume.SetMute(*muted, std::ptr::null());
                        }
                    }
                }
            }
            let restore = Restore(
                controls
                    .iter()
                    .filter(|r| r.3)
                    .map(|r| (r.0.clone(), r.1, r.2))
                    .collect(),
            );
            let target = if original >= 0.01 {
                original - 0.01
            } else {
                original + 0.01
            };
            let after = refresh(&Pending {
                level: Some(target),
                muted: None,
            })
            .unwrap();
            assert!((after.level.unwrap() - target).abs() < 0.0001);
            for (volume, level, muted, spotify) in &controls {
                if !spotify {
                    assert!((volume.GetMasterVolume().unwrap() - level).abs() < 0.0001);
                    assert_eq!(volume.GetMute().unwrap().as_bool(), *muted);
                }
            }
            drop(restore);
            for (volume, level, muted, spotify) in &controls {
                if *spotify {
                    assert!((volume.GetMasterVolume().unwrap() - level).abs() < 0.0001);
                    assert_eq!(volume.GetMute().unwrap().as_bool(), *muted);
                }
            }
            // Interfaces are released before uninitializing COM.
            drop(controls);
            drop(endpoints);
            drop(devices);
            CoUninitialize();
        }
    }
    #[test]
    fn slider_reaches_both_endpoints_with_thumb_padding_and_scaled_widths() {
        for scale in [1, 2] {
            let left = 96 * scale;
            let width = 220 * scale;
            assert_eq!(slider_level(101 * scale, left, width), Some(0.));
            assert_eq!(slider_level(206 * scale, left, width), Some(0.5));
            assert_eq!(slider_level(311 * scale, left, width), Some(1.));
            assert_eq!(slider_level(left - 100, left, width), Some(0.));
            assert_eq!(slider_level(left + width + 100, left, width), Some(1.));
        }
        assert_eq!(slider_level(0, 0, 0), None);
    }
    #[test]
    fn queued_slider_changes_preserve_final_level_and_mute_intent() {
        let mut pending = Pending::default();
        pending.push(Command::Level(0.8));
        pending.push(Command::Level(0.2));
        pending.push(Command::Mute(true));
        assert_eq!(pending.level, Some(0.2));
        assert_eq!(pending.muted, Some(true));
        pending.push(Command::Level(0.4));
        assert_eq!(pending.muted, Some(false));
        pending.push(Command::Level(f32::NAN));
        assert_eq!(pending.level, Some(0.4));
        pending.push(Command::Level(2.));
        assert_eq!(pending.level, Some(1.));
        pending.push(Command::Level(-1.));
        assert_eq!(pending.level, Some(0.));
    }
}
