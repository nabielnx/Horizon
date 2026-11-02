use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};
use windows::{
    Foundation::TypedEventHandler,
    Media::Control::*,
    Storage::Streams::DataReader,
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::WinRT::*,
        UI::WindowsAndMessaging::{PostMessageW, WM_APP},
    },
};
#[derive(Clone, Default)]
pub struct Track {
    pub source_app: String,
    pub album: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub updated: Option<Instant>,
    pub cover: Option<Arc<Vec<u8>>>,
    pub cover_revision: u64,
    pub palette: Option<[u32; 3]>,
    pub lyrics: Arc<Vec<crate::lyrics::Line>>,
    pub lyrics_loaded: bool,
    pub lyrics_retry: Option<Instant>,
    pub error: String,
    pub seek_guard: Option<(f64, Instant)>,
}
impl Track {
    pub fn position_now(&self) -> f64 {
        let extra = if self.playing {
            self.updated
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.)
        } else {
            0.
        };
        (self.position + extra).max(0.).min(self.duration.max(0.))
    }
    pub fn lyric_index(&self) -> Option<usize> {
        if self.lyrics.is_empty() {
            return None;
        }
        let at = self.position_now() + 0.1;
        let idx = self.lyrics.partition_point(|line| line.at <= at);
        Some(if idx == 0 {
            0
        } else {
            (idx - 1).min(self.lyrics.len() - 1)
        })
    }
}
pub enum Command {
    Refresh,
    Playback,
    LyricsChanged,
    Toggle,
    Previous,
    Next,
    Seek(f64),
}
pub struct Media {
    pub snapshot: Arc<Mutex<Track>>,
    pub send: mpsc::Sender<Command>,
    pub lyrics_enabled: Arc<AtomicBool>,
    pub notify: Arc<AtomicUsize>,
}
pub fn notify(target: &AtomicUsize) {
    let handle = target.load(Ordering::Relaxed);
    if handle != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(handle as *mut _)),
                WM_APP + 9,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}
fn read_thumbnail(
    props: &GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<(Arc<Vec<u8>>, [u32; 3])> {
    (|| -> windows::core::Result<Vec<u8>> {
        let stream = props.Thumbnail()?.OpenReadAsync()?.join()?;
        let size = stream.Size()?;
        if size == 0 || size > 25_000_000 {
            return Ok(vec![]);
        }
        let reader = DataReader::CreateDataReader(&stream)?;
        let mut bytes = vec![0u8; size as usize];
        let mut offset = 0;
        while offset < bytes.len() {
            let to_read = (bytes.len() - offset).min(1024 * 1024) as u32;
            let loaded = reader.LoadAsync(to_read)?.join()? as usize;
            if loaded == 0 {
                break;
            }
            reader.ReadBytes(&mut bytes[offset..offset + loaded])?;
            offset += loaded;
        }
        let _ = reader.Close();
        let _ = stream.Close();
        if offset != bytes.len() {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_FAIL,
                "Incomplete media thumbnail",
            ));
        }
        Ok(bytes)
    })()
    .ok()
    .filter(|b| !b.is_empty())
    .and_then(validated_thumbnail)
}

fn validated_thumbnail(bytes: Vec<u8>) -> Option<(Arc<Vec<u8>>, [u32; 3])> {
    crate::artwork::palette(&bytes).map(|palette| (Arc::new(bytes), palette))
}

fn cover_retry_delay(attempt: u8) -> std::time::Duration {
    std::time::Duration::from_millis(match attempt {
        0..=10 => 250,
        11..=20 => 1000,
        21..=35 => 2500,
        _ => 30000,
    })
}

fn lyric_request_due(data: &Track, requested: &Option<(String, f64)>, now: Instant) -> bool {
    let key = format!("{}|{}", data.title, data.artist);
    requested
        .as_ref()
        .is_none_or(|(old, duration)| old != &key || (data.duration - duration).abs() >= 2.)
        || data.lyrics_retry.is_some_and(|at| at <= now)
}

type CachedCover = (String, Arc<Vec<u8>>, Option<[u32; 3]>);

fn cache_cover(cache: &mut std::collections::VecDeque<CachedCover>, entry: CachedCover) {
    const LIMIT: usize = 4 * 1024 * 1024;
    cache.retain(|(key, _, _)| key != &entry.0);
    if entry.1.len() > LIMIT {
        return;
    }
    while cache.len() >= 4
        || cache.iter().map(|(_, bytes, _)| bytes.len()).sum::<usize>() + entry.1.len() > LIMIT
    {
        cache.pop_front();
    }
    cache.push_back(entry);
}

impl Media {
    pub fn fixture(track: Track) -> Self {
        let (send, _) = mpsc::channel();
        Self {
            snapshot: Arc::new(Mutex::new(track)),
            send,
            lyrics_enabled: Arc::new(AtomicBool::new(true)),
            notify: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn start(lyrics_enabled: bool) -> Self {
        let snapshot = Arc::new(Mutex::new(Track::default()));
        let enabled = Arc::new(AtomicBool::new(lyrics_enabled));
        let target = Arc::new(AtomicUsize::new(0));
        let lyric_jobs = crate::lyrics::worker(snapshot.clone(), enabled.clone(), target.clone());
        let (send, receive) = mpsc::channel();
        let state = snapshot.clone();
        let events = send.clone();
        let config = enabled.clone();
        let gui = target.clone();
        std::thread::spawn(move || {
            let result = (|| -> windows::core::Result<()> {
                unsafe {
                    RoInitialize(RO_INIT_MULTITHREADED)?;
                }
                let manager =
                    GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
                let tx = events.clone();
                let _token =
                    manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
                        let _ = tx.send(Command::Refresh);
                        Ok(())
                    }))?;
                let tx_sess = events.clone();
                let _token_sess =
                    manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
                        let _ = tx_sess.send(Command::Refresh);
                        Ok(())
                    }))?;
                let mut attached: Option<(GlobalSystemMediaTransportControlsSession, i64, i64)> =
                    None;
                let mut refresh = true;
                let mut requested: Option<(String, f64)> = None;
                let mut cover_retries = 0u8;
                let mut next_cover = Instant::now();
                let mut next_metadata = Instant::now();
                let mut cover_cache: std::collections::VecDeque<CachedCover> =
                    std::collections::VecDeque::with_capacity(4);
                loop {
                    if Instant::now() >= next_metadata {
                        refresh = true;
                    }
                    if refresh {
                        next_metadata = Instant::now() + std::time::Duration::from_secs(15);
                        if let Some((old, media, play)) = attached.take() {
                            let _ = old.RemoveMediaPropertiesChanged(media);
                            let _ = old.RemovePlaybackInfoChanged(play);
                        }
                        let current_session = manager.GetCurrentSession().ok();
                        let mut valid_session = None;
                        if let Some(session) = current_session
                            && let Ok(info) = session.GetPlaybackInfo()
                            && let Ok(status) = info.PlaybackStatus()
                            && status
                                != GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed
                            && status
                                != GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped
                        {
                            valid_session = Some(session);
                        }

                        if let Some(session) = valid_session {
                            let source_app = session
                                .SourceAppUserModelId()
                                .map(|s| s.to_string())
                                .unwrap_or_default();
                            let tx = events.clone();
                            let media = session.MediaPropertiesChanged(&TypedEventHandler::new(
                                move |_, _| {
                                    let _ = tx.send(Command::Refresh);
                                    Ok(())
                                },
                            ))?;
                            let tx = events.clone();
                            let play = session.PlaybackInfoChanged(&TypedEventHandler::new(
                                move |_, _| {
                                    let _ = tx.send(Command::Playback);
                                    Ok(())
                                },
                            ))?;
                            if let Ok(op) = session.TryGetMediaPropertiesAsync()
                                && let Ok(props) = op.join()
                            {
                                let title =
                                    props.Title().map(|s| s.to_string()).unwrap_or_default();
                                let artist =
                                    props.Artist().map(|s| s.to_string()).unwrap_or_default();
                                if title.is_empty() && artist.is_empty() {
                                    let _ = session.RemoveMediaPropertiesChanged(media);
                                    let _ = session.RemovePlaybackInfoChanged(play);
                                    attached = None;
                                    if let Ok(mut data) = state.lock() {
                                        *data = Track::default();
                                    }
                                    requested = None;
                                    notify(&gui);
                                } else {
                                    let album = props
                                        .AlbumTitle()
                                        .map(|s| s.to_string())
                                        .unwrap_or_default();
                                    let track_key =
                                        format!("{}|{}|{}|{}", source_app, title, artist, album);
                                    let (changed, needs_cover) = state
                                        .lock()
                                        .map(|data| {
                                            (
                                                data.source_app != source_app
                                                    || data.album != album
                                                    || data.title != title
                                                    || data.artist != artist,
                                                data.cover.is_none(),
                                            )
                                        })
                                        .unwrap_or((true, true));
                                    if changed || needs_cover && Instant::now() >= next_cover {
                                        if changed {
                                            cover_retries = 0;
                                        }
                                        cover_retries = cover_retries.saturating_add(1);
                                        next_cover =
                                            Instant::now() + cover_retry_delay(cover_retries);
                                        let cached_entry = cover_cache
                                            .iter()
                                            .find(|(k, _, _)| k == &track_key)
                                            .map(|(_, c, p)| (c.clone(), *p));
                                        let (cover, palette) = if let Some((c, p)) = cached_entry {
                                            cover_retries = 35;
                                            (Some(c), p)
                                        } else {
                                            let (fetched, pal) = read_thumbnail(&props)
                                                .map(|(bytes, palette)| {
                                                    (Some(bytes), Some(palette))
                                                })
                                                .unwrap_or((None, None));
                                            if let Some(ref c) = fetched {
                                                cache_cover(
                                                    &mut cover_cache,
                                                    (track_key.clone(), c.clone(), pal),
                                                );
                                            }
                                            (fetched, pal)
                                        };
                                        let mut should_notify = false;
                                        if let Ok(mut data) = state.lock() {
                                            if changed {
                                                data.source_app = source_app;
                                                data.album = album;
                                                data.title = title;
                                                data.artist = artist;
                                                data.duration = 0.;
                                                data.position = 0.;
                                                data.updated = None;
                                                data.seek_guard = None;
                                                data.cover = cover;
                                                data.cover_revision =
                                                    data.cover_revision.wrapping_add(1);
                                                data.palette = palette;
                                                data.lyrics = Arc::new(vec![]);
                                                data.lyrics_loaded = false;
                                                data.lyrics_retry = None;
                                                data.error.clear();
                                                should_notify = true;
                                            } else if cover.is_some() {
                                                data.cover = cover;
                                                data.cover_revision =
                                                    data.cover_revision.wrapping_add(1);
                                                data.palette = palette;
                                                should_notify = true;
                                            }
                                        }
                                        if changed {
                                            requested = None;
                                        }
                                        if should_notify {
                                            notify(&gui);
                                        }
                                    }
                                    attached = Some((session, media, play));
                                }
                            } else {
                                let _ = session.RemoveMediaPropertiesChanged(media);
                                let _ = session.RemovePlaybackInfoChanged(play);
                                attached = None;
                                if let Ok(mut data) = state.lock() {
                                    *data = Track::default();
                                }
                                requested = None;
                                notify(&gui);
                            }
                        } else {
                            attached = None;
                            if let Ok(mut data) = state.lock() {
                                *data = Track::default();
                            }
                            requested = None;
                            notify(&gui);
                        }
                        refresh = false;
                    }
                    if let Some((session, _, _)) = &attached {
                        let mut alive = false;
                        let needs_cover = state
                            .lock()
                            .map(|data| !data.title.is_empty() && data.cover.is_none())
                            .unwrap_or(false);
                        if needs_cover && Instant::now() >= next_cover {
                            cover_retries = cover_retries.saturating_add(1);
                            next_cover = Instant::now() + cover_retry_delay(cover_retries);
                            if let Ok(op) = session.TryGetMediaPropertiesAsync()
                                && let Ok(props) = op.join()
                                && let Some((cover, palette)) = read_thumbnail(&props)
                            {
                                let palette = Some(palette);
                                let title =
                                    props.Title().map(|s| s.to_string()).unwrap_or_default();
                                let artist =
                                    props.Artist().map(|s| s.to_string()).unwrap_or_default();
                                let album = props
                                    .AlbumTitle()
                                    .map(|s| s.to_string())
                                    .unwrap_or_default();
                                if let Ok(mut data) = state.lock() {
                                    if data.title == title && data.artist == artist {
                                        let current_key = format!(
                                            "{}|{}|{}|{}",
                                            data.source_app, title, artist, album
                                        );
                                        cache_cover(
                                            &mut cover_cache,
                                            (current_key, cover.clone(), palette),
                                        );
                                        data.cover = Some(cover);
                                        data.cover_revision = data.cover_revision.wrapping_add(1);
                                        data.palette = palette;
                                    } else {
                                        refresh = true;
                                    }
                                }
                                notify(&gui);
                            }
                        }
                        if let (Ok(play), Ok(time)) =
                            (session.GetPlaybackInfo(), session.GetTimelineProperties())
                            && let Ok(status) = play.PlaybackStatus()
                            && status
                                != GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed
                            && status
                                != GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped
                        {
                            alive = true;
                            if let Ok(mut data) = state.lock() {
                                let playing = status
                                    == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
                                let changed = data.playing != playing;
                                data.playing = playing;
                                if let (Ok(end), Ok(start)) = (time.EndTime(), time.StartTime()) {
                                    data.duration =
                                        (end.Duration - start.Duration) as f64 / 10_000_000.;
                                }
                                let raw_time =
                                    time.LastUpdatedTime().map(|t| t.UniversalTime).unwrap_or(0);
                                let since = if raw_time > 116_444_736_000_000_000 {
                                    let now_dur = std::time::SystemTime::now()
                                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                                        .unwrap_or_default();
                                    let now_ticks = 116_444_736_000_000_000i64
                                        + (now_dur.as_secs() as i64 * 10_000_000)
                                        + (now_dur.subsec_nanos() as i64 / 100);
                                    let diff_ticks = now_ticks - raw_time;
                                    let s = diff_ticks as f64 / 10_000_000.0;
                                    if (0.0..=3600.0).contains(&s) { s } else { 0.0 }
                                } else {
                                    0.0
                                };
                                if let Ok(pos) = time.Position() {
                                    let incoming = (pos.Duration as f64 / 10_000_000.
                                        + if playing { since } else { 0. })
                                    .clamp(0., data.duration.max(0.));

                                    let should_update = match data.seek_guard {
                                        Some((target, t)) => {
                                            if t.elapsed().as_millis() < 800 {
                                                let delta = (incoming - target).abs();
                                                if delta <= 2.0 {
                                                    data.seek_guard = None;
                                                    true
                                                } else {
                                                    false
                                                }
                                            } else {
                                                data.seek_guard = None;
                                                true
                                            }
                                        }
                                        None => true,
                                    };

                                    if should_update {
                                        data.position = incoming;
                                        data.updated = Some(Instant::now());
                                    }
                                }
                                if changed {
                                    notify(&gui);
                                }
                                let key = format!("{}|{}", data.title, data.artist);
                                if config.load(Ordering::Relaxed)
                                    && !data.title.is_empty()
                                    && lyric_request_due(&data, &requested, Instant::now())
                                {
                                    data.lyrics_retry = None;
                                    let _ = lyric_jobs.send(crate::lyrics::Job {
                                        title: data.title.clone(),
                                        artist: data.artist.clone(),
                                        duration: data.duration,
                                    });
                                    requested = Some((key, data.duration));
                                }
                            }
                        }
                        if !alive {
                            refresh = true;
                        }
                    }
                    let poll_timeout = if state
                        .lock()
                        .is_ok_and(|data| !data.title.is_empty() && data.cover.is_none())
                    {
                        next_cover.saturating_duration_since(Instant::now()).clamp(
                            std::time::Duration::from_millis(50),
                            std::time::Duration::from_secs(1),
                        )
                    } else {
                        std::time::Duration::from_secs(1)
                    };
                    match receive.recv_timeout(poll_timeout) {
                        Ok(Command::Refresh) => refresh = true,
                        Ok(Command::Playback) => {
                            next_cover = Instant::now();
                        }
                        Ok(Command::LyricsChanged) => {
                            requested = None;
                            if !config.load(Ordering::Relaxed)
                                && let Ok(mut data) = state.lock()
                            {
                                data.lyrics = Arc::new(vec![]);
                                data.lyrics_loaded = false;
                            }
                        }
                        Ok(command) => {
                            if let Some((session, _, _)) = &attached {
                                let action = match command {
                                    Command::Toggle => session.TryTogglePlayPauseAsync()?.join(),
                                    Command::Previous => session.TrySkipPreviousAsync()?.join(),
                                    Command::Next => session.TrySkipNextAsync()?.join(),
                                    Command::Seek(fraction) => {
                                        let duration =
                                            state.lock().map(|s| s.duration).unwrap_or(0.);
                                        let target = fraction.clamp(0., 1.) * duration;
                                        if let Ok(mut snap) = state.lock() {
                                            snap.position = target;
                                            snap.updated = Some(Instant::now());
                                            snap.seek_guard = Some((target, Instant::now()));
                                        }
                                        session
                                            .TryChangePlaybackPositionAsync(
                                                (target * 10_000_000.) as i64,
                                            )?
                                            .join()
                                    }
                                    _ => Ok(false),
                                };
                                if let Err(error) = action {
                                    refresh = true;
                                    if let Ok(mut data) = state.lock() {
                                        data.error = error.to_string();
                                    }
                                }
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if attached.is_none() {
                                refresh = true;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                Ok(())
            })();
            if let Err(e) = result
                && let Ok(mut data) = state.lock()
            {
                data.error = format!("Media Windows belum tersedia: {e}");
                notify(&gui);
            }
        });
        Self {
            snapshot,
            send,
            lyrics_enabled: enabled,
            notify: target,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duration_correction_and_expired_lyrics_retry_trigger_new_requests() {
        let now = Instant::now();
        let mut track = Track {
            title: "Track".into(),
            artist: "Artist".into(),
            duration: 233.,
            ..Default::default()
        };
        assert!(lyric_request_due(
            &track,
            &Some(("Track|Artist".into(), 210.)),
            now
        ));
        assert!(lyric_request_due(
            &track,
            &Some(("Track|Artist".into(), 0.)),
            now
        ));
        let requested = Some(("Track|Artist".into(), 233.));
        assert!(!lyric_request_due(&track, &requested, now));
        track.lyrics_retry = Some(now + std::time::Duration::from_secs(300));
        assert!(!lyric_request_due(&track, &requested, now));
        assert!(lyric_request_due(
            &track,
            &requested,
            now + std::time::Duration::from_secs(301)
        ));
    }
    #[test]
    fn late_cover_retries_continue_slowly_and_invalid_images_are_rejected() {
        assert_eq!(cover_retry_delay(36).as_secs(), 30);
        assert_eq!(cover_retry_delay(255).as_secs(), 30);
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            );
        }
        assert!(validated_thumbnail(vec![1, 2, 3, 4]).is_none());
        assert!(
            validated_thumbnail(include_bytes!("../assets/fixtures/cover.png").to_vec()).is_some()
        );
    }
    #[test]
    fn cover_cache_replaces_duplicates_and_obeys_byte_budget() {
        let mut cache = std::collections::VecDeque::new();
        let cover = Arc::new(vec![0; 2 * 1024 * 1024]);
        for key in ["one", "two", "three"] {
            cache_cover(&mut cache, (key.into(), cover.clone(), None));
        }
        assert_eq!(cache.len(), 2);
        assert_eq!(cache[0].0, "two");
        cache_cover(&mut cache, ("three".into(), cover, None));
        assert_eq!(cache.len(), 2);
        cache_cover(
            &mut cache,
            ("large".into(), Arc::new(vec![0; 4 * 1024 * 1024 + 1]), None),
        );
        assert_eq!(cache.len(), 2);
    }
    #[test]
    fn paused_track_position_and_lyrics_stay_frozen() {
        let t = Track {
            position: 12.,
            duration: 120.,
            updated: Instant::now().checked_sub(std::time::Duration::from_secs(30)),
            lyrics: Arc::new(crate::lyrics::parse("[00:00]A\n[00:10]B\n[00:20]C")),
            ..Default::default()
        };
        assert_eq!(t.position_now(), 12.);
        assert_eq!(t.lyric_index(), Some(1));
    }
}
