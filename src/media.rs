use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use windows::{
    Foundation::TypedEventHandler,
    Media::Control::*,
    Storage::Streams::{DataReader, InputStreamOptions},
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::WinRT::*,
        UI::WindowsAndMessaging::{PostMessageW, WM_APP},
    },
};
#[derive(Clone, Default)]
pub struct Track {
    pub generation: u64,
    pub can_seek: bool,
    pub seek_start: f64,
    pub seek_feedback: Option<Instant>,
    pub source_app: String,
    pub album: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub updated: Option<Instant>,
    pub playback_rate: Option<f64>,
    pub cover: Option<Arc<Vec<u8>>>,
    pub cover_revision: u64,
    pub cover_error: String,
    pub cover_attempts: u8,
    pub cover_failures: std::collections::VecDeque<String>,
    pub cover_source_url: Option<String>,
    pub palette: Option<[u32; 3]>,
    pub lyrics: Arc<Vec<crate::lyrics::Line>>,
    pub lyrics_loaded: bool,
    pub lyrics_retry: Option<Instant>,
    pub lyrics_status: crate::lyrics::Status,
    pub lyrics_started: Option<Instant>,
    pub lyrics_attempts: Vec<crate::lyrics::Attempt>,
    pub error: String,
    pub seek_guard: Option<(f64, Instant)>,
}

pub struct LyricFrame {
    pub active: Option<usize>,
    pub latest: Option<usize>,
    pub outgoing: Option<(usize, f32)>,
    pub entry: f32,
}

impl Track {
    fn finish_seek(&mut self, generation: u64, target: f64, accepted: bool, now: Instant) {
        if self.generation != generation {
            return;
        }
        if accepted {
            self.position = target;
            self.updated = Some(now);
            self.seek_guard = Some((target, now));
            self.seek_feedback = None;
        } else {
            self.seek_guard = None;
            self.seek_feedback = Some(now);
        }
    }

    fn clear_lyrics(&mut self) {
        self.lyrics = Arc::default();
        self.lyrics_loaded = false;
        self.lyrics_retry = None;
        self.lyrics_status = crate::lyrics::Status::Searching;
        self.lyrics_started = None;
        self.lyrics_attempts.clear();
    }

    pub fn lyrics_message(&self) -> &'static str {
        if self.title.is_empty() {
            return "";
        }
        crate::i18n::tr(crate::lyrics::status_message(
            self.lyrics_status,
            self.lyrics_started
                .map_or(Duration::ZERO, |at| at.elapsed()),
        ))
    }
    pub fn position_now(&self) -> f64 {
        let extra = if self.playing {
            self.updated
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.)
        } else {
            0.
        };
        (self.position + extra * self.playback_rate.unwrap_or(1.))
            .max(0.)
            .min(self.duration.max(0.))
    }
    pub fn lyric_index(&self) -> Option<usize> {
        self.lyric_index_at(self.position_now())
    }
    pub fn next_lyric_time(&self, index: usize) -> f64 {
        self.lyrics
            .get(index + 1)
            .map_or(self.duration, |line| line.at)
    }
    pub fn lyric_index_at(&self, at: f64) -> Option<usize> {
        let idx = self.lyrics.partition_point(|line| line.at <= at);
        let index = idx.checked_sub(1)?;
        let line = &self.lyrics[index];
        if at >= line.playback_end(self.next_lyric_time(index)) || line.text.is_empty() {
            None
        } else {
            Some(index)
        }
    }

    pub fn lyric_frame(&self, at: f64, animate: bool) -> LyricFrame {
        let started = self.lyrics.partition_point(|line| line.at <= at);
        let active = self.lyric_index_at(at);
        let latest = self.lyrics[..started]
            .iter()
            .rposition(|line| !line.text.is_empty());
        let mut frame = LyricFrame {
            active,
            latest,
            outgoing: None,
            entry: 1.,
        };
        if !animate {
            return frame;
        }

        let rate = self.playback_rate.unwrap_or(1.).max(0.01);
        let progress = |elapsed: f64, seconds: f64| {
            let t = (elapsed / (seconds * rate)).clamp(0., 1.) as f32;
            t * t * (3. - 2. * t)
        };
        if let Some(index) = active {
            frame.entry = progress(at - self.lyrics[index].at, 0.16);
        }
        let previous = match active {
            Some(index) if frame.entry < 1. => self.lyrics[..index]
                .iter()
                .rposition(|line| !line.text.is_empty()),
            Some(_) => None,
            None => latest,
        };
        if let Some(index) = previous {
            let end = self.lyrics[index].playback_end(self.next_lyric_time(index));
            let fade = progress(at - end, 0.18);
            if fade < 1. {
                frame.outgoing = Some((index, fade));
            }
        }
        frame
    }

    pub fn lyric_boundary_wait(&self, fallback: Duration) -> Duration {
        let position = self.position_now();
        let started = self.lyrics.partition_point(|line| line.at <= position);
        let mut boundary = self
            .lyrics
            .get(started)
            .map_or(f64::INFINITY, |line| line.at);
        if let Some(index) = started.checked_sub(1) {
            let line = &self.lyrics[index];
            let end = line.playback_end(self.next_lyric_time(index));
            if !line.text.is_empty() && end > position {
                boundary = boundary.min(end);
            }
        }
        let wait = (boundary - position) / self.playback_rate.unwrap_or(1.).max(0.01);
        Duration::from_secs_f64(wait.clamp(0.001, fallback.as_secs_f64()))
    }
}
pub enum Command {
    Refresh,
    Playback,
    Timeline,
    LyricsChanged,
    CoverReady,
    Toggle,
    Previous,
    Next,
    Seek { fraction: f64, generation: u64 },
}
pub struct Media {
    pub snapshot: Arc<Mutex<Track>>,
    pub send: mpsc::Sender<Command>,
    pub lyrics_enabled: Arc<AtomicBool>,
    pub cover_fallback_enabled: Arc<AtomicBool>,
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
fn await_operation<T: windows::core::RuntimeType>(
    operation: windows_future::IAsyncOperation<T>,
    deadline: Instant,
    alive: &impl Fn() -> bool,
) -> windows::core::Result<T> {
    while operation.Status()?.0 == 0 {
        if !alive() || Instant::now() >= deadline {
            let _ = operation.Cancel();
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x800705B4u32 as i32),
                "Media request expired or was superseded",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !alive() {
        return Err(windows::core::Error::new(
            windows::Win32::Foundation::E_ABORT,
            "Media request was superseded",
        ));
    }
    operation.GetResults()
}

fn thumbnail_error(stage: &str, error: windows::core::Error) -> windows::core::Error {
    let (code, detail) = if error.code().is_ok() {
        (
            windows::Win32::Foundation::E_FAIL,
            "Player returned no thumbnail object".to_owned(),
        )
    } else {
        (error.code(), error.to_string())
    };
    windows::core::Error::new(code, format!("{stage}: {detail}"))
}

fn read_thumbnail(
    props: &GlobalSystemMediaTransportControlsSessionMediaProperties,
    alive: &impl Fn() -> bool,
) -> windows::core::Result<(Arc<Vec<u8>>, [u32; 3])> {
    let deadline = Instant::now() + Duration::from_secs(4);
    let reference = props
        .Thumbnail()
        .map_err(|e| thumbnail_error("Thumbnail metadata", e))?;
    let operation = reference
        .OpenReadAsync()
        .map_err(|e| thumbnail_error("Open thumbnail", e))?;
    let stream = await_operation(operation, deadline, alive)
        .map_err(|e| thumbnail_error("Read thumbnail", e))?;
    let bytes = read_cover_stream(&windows::core::Interface::cast(&stream)?, deadline, alive);
    let _ = stream.Close();
    validated_thumbnail(bytes?).ok_or_else(|| {
        windows::core::Error::new(
            windows::Win32::Foundation::E_FAIL,
            "Empty or undecodable media thumbnail",
        )
    })
}

fn read_cover_stream(
    stream: &windows::Storage::Streams::IRandomAccessStream,
    deadline: Instant,
    alive: &impl Fn() -> bool,
) -> windows::core::Result<Vec<u8>> {
    const LIMIT: usize = 25_000_000;
    let size = stream.Size().unwrap_or(0) as usize;
    if size > LIMIT {
        return Err(windows::core::Error::new(
            windows::Win32::Foundation::E_INVALIDARG,
            "Media thumbnail exceeds byte limit",
        ));
    }
    let input = stream.GetInputStreamAt(0)?;
    let reader = DataReader::CreateDataReader(&input)?;
    let result = (|| {
        reader.SetInputStreamOptions(InputStreamOptions::Partial)?;
        let mut bytes = Vec::with_capacity(size.min(512 * 1024));
        loop {
            // Some players expose a stream before a useful Size is known.
            // Read actual partial chunks, keeping both time and bytes bounded.
            let to_read = if size > 0 {
                if bytes.len() >= size {
                    break;
                }
                (size - bytes.len()).min(64 * 1024)
            } else {
                (LIMIT + 1 - bytes.len()).min(64 * 1024)
            };
            let loaded =
                await_operation(reader.LoadAsync(to_read as u32)?, deadline, alive)? as usize;
            if loaded == 0 {
                break;
            }
            if bytes.len() + loaded > LIMIT {
                return Err(windows::core::Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Media thumbnail exceeds byte limit",
                ));
            }
            let offset = bytes.len();
            bytes.resize(offset + loaded, 0);
            reader.ReadBytes(&mut bytes[offset..])?;
        }
        Ok(bytes)
    })();
    let _ = reader.Close();
    result
}

pub(crate) fn track_cover_key(track: &Track) -> String {
    format!(
        "{}|{}|{}|{}",
        track.source_app, track.title, track.artist, track.album
    )
}
fn keep_last_track(state: &Mutex<Track>, source_app: &str) -> bool {
    let Ok(mut data) = state.lock() else {
        return false;
    };
    if data.source_app == source_app {
        return true;
    }
    *data = Track {
        generation: data.generation.wrapping_add(1),
        ..Track::default()
    };
    false
}
fn same_song(data: &Track, source: &str, title: &str, artist: &str) -> bool {
    data.source_app == source && data.title == title && data.artist == artist
}
fn remember_cover_failure(data: &mut Track, error: &str) {
    if data.cover_failures.len() >= 12 {
        data.cover_failures.pop_front();
    }
    data.cover_failures.push_back(format!(
        "{} / {} / {:.3}s | attempt {} | {} | {}",
        data.title.chars().take(128).collect::<String>(),
        data.album.chars().take(128).collect::<String>(),
        data.duration,
        data.cover_attempts,
        chrono::Local::now().format("%H:%M:%S"),
        error.chars().take(512).collect::<String>()
    ));
}
fn apply_fallback(data: &mut Track, key: &str, art: crate::cover_fallback::Artwork) -> bool {
    if data.cover.is_some() || track_cover_key(data) != key {
        return false;
    }
    data.cover = Some(art.bytes);
    data.palette = Some(art.palette);
    data.cover_source_url = Some(art.link);
    data.cover_revision = data.cover_revision.wrapping_add(1);
    true
}
struct CoverJob {
    key: String,
    generation: u64,
    session: GlobalSystemMediaTransportControlsSession,
    properties: Option<GlobalSystemMediaTransportControlsSessionMediaProperties>,
}
struct CoverResult {
    key: String,
    generation: u64,
    image: windows::core::Result<(Arc<Vec<u8>>, [u32; 3])>,
}
fn cover_worker(
    state: Arc<Mutex<Track>>,
    wake: mpsc::Sender<Command>,
) -> (mpsc::SyncSender<CoverJob>, mpsc::Receiver<CoverResult>) {
    let (send, receive) = mpsc::sync_channel::<CoverJob>(1);
    let (results, completed) = mpsc::channel();
    std::thread::spawn(move || {
        if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
            return;
        }
        while let Ok(job) = receive.recv() {
            let alive = || {
                state.lock().is_ok_and(|track| {
                    track.generation == job.generation && track_cover_key(&track) == job.key
                })
            };
            let image = (|| {
                if let Some(props) = job.properties.as_ref() {
                    return read_thumbnail(props, &alive);
                }
                let props = await_operation(
                    job.session.TryGetMediaPropertiesAsync()?,
                    Instant::now() + Duration::from_secs(2),
                    &alive,
                )?;
                let source = job.session.SourceAppUserModelId()?.to_string();
                let title = props.Title()?.to_string();
                let artist = props.Artist()?.to_string();
                let album = props.AlbumTitle().unwrap_or_default();
                if !state.lock().is_ok_and(|track| {
                    track.generation == job.generation
                        && same_song(&track, &source, &title, &artist)
                        && (album.is_empty() || album == track.album.as_str())
                }) {
                    return Err(windows::core::Error::new(
                        windows::Win32::Foundation::E_ABORT,
                        "Cover belongs to a newer track",
                    ));
                }
                read_thumbnail(&props, &alive)
            })();
            if results
                .send(CoverResult {
                    key: job.key,
                    generation: job.generation,
                    image,
                })
                .is_err()
            {
                break;
            }
            let _ = wake.send(Command::CoverReady);
        }
    });
    (send, completed)
}

fn validated_thumbnail(bytes: Vec<u8>) -> Option<(Arc<Vec<u8>>, [u32; 3])> {
    crate::artwork::palette(&bytes).map(|palette| (Arc::new(bytes), palette))
}

fn cover_retry_delay(attempt: u8) -> std::time::Duration {
    std::time::Duration::from_millis(match attempt {
        0..=10 => 250,
        11..=20 => 1000,
        21..=35 => 2500,
        _ => 5000,
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
            cover_fallback_enabled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn start(lyrics_enabled: bool, artwork_fallback: bool) -> Self {
        let snapshot = Arc::new(Mutex::new(Track::default()));
        let enabled = Arc::new(AtomicBool::new(lyrics_enabled));
        let fallback_enabled = Arc::new(AtomicBool::new(artwork_fallback));
        let fallback_config = fallback_enabled.clone();
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
                // A no-delay logon task can run before Windows media services
                // are ready. Retry here on the worker instead of blocking the
                // first panel or permanently losing the media session.
                let mut manager_attempts = 0u8;
                let manager = loop {
                    let request = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                        .and_then(|operation| {
                            await_operation(
                                operation,
                                Instant::now() + Duration::from_secs(3),
                                &|| true,
                            )
                        });
                    match request {
                        Ok(manager) => {
                            if let Ok(mut data) = state.lock() {
                                data.error.clear();
                            }
                            break manager;
                        }
                        Err(error) => {
                            if let Ok(mut data) = state.lock() {
                                data.error = format!("Media Windows belum tersedia: {error}");
                            }
                            notify(&gui);
                            manager_attempts = manager_attempts.saturating_add(1);
                            std::thread::sleep(Duration::from_millis(if manager_attempts < 4 {
                                500
                            } else {
                                2000
                            }));
                        }
                    }
                };
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
                let mut attached: Option<(
                    GlobalSystemMediaTransportControlsSession,
                    i64,
                    i64,
                    Option<i64>,
                )> = None;
                let mut refresh = true;
                let mut requested: Option<(String, f64)> = None;
                let mut cover_retries = 0u8;
                let mut next_cover = Instant::now();
                let mut next_metadata = Instant::now();
                let mut cover_cache: std::collections::VecDeque<CachedCover> =
                    std::collections::VecDeque::with_capacity(4);
                let (cover_jobs, cover_results) = cover_worker(state.clone(), events.clone());
                let mut cover_pending = false;
                let mut cover_refresh_requested = true;
                let mut cover_recheck_at = None;
                let mut missing_since: Option<Instant> = None;
                let (fallback_jobs, fallback_results) = crate::cover_fallback::worker(
                    state.clone(),
                    fallback_config.clone(),
                    events.clone(),
                );
                let mut fallback_pending = false;
                let mut next_fallback = Instant::now() + Duration::from_secs(4);
                let mut timeline_changed = false;
                loop {
                    let mut fresh_properties = None;
                    while let Ok(result) = fallback_results.try_recv() {
                        fallback_pending = false;
                        if let Ok(mut data) = state.lock()
                            && fallback_config.load(Ordering::Relaxed)
                            && data.generation == result.generation
                            && track_cover_key(&data) == result.key
                            && data.cover.is_none()
                        {
                            match result.image {
                                Ok(art) => {
                                    apply_fallback(&mut data, &result.key, art);
                                }
                                Err(error) => {
                                    remember_cover_failure(&mut data, &format!("Apple: {error}"));
                                }
                            }
                            notify(&gui);
                        }
                    }
                    if fallback_config.load(Ordering::Relaxed)
                        && !fallback_pending
                        && Instant::now() >= next_fallback
                    {
                        let job = state
                            .lock()
                            .ok()
                            .filter(|data| {
                                data.cover.is_none()
                                    && !data.title.is_empty()
                                    && !data.artist.is_empty()
                            })
                            .map(|data| crate::cover_fallback::Job {
                                key: track_cover_key(&data),
                                generation: data.generation,
                                title: data.title.clone(),
                                artist: data.artist.clone(),
                                album: data.album.clone(),
                                duration: data.duration,
                            });
                        if let Some(job) = job
                            && fallback_jobs.try_send(job).is_ok()
                        {
                            fallback_pending = true;
                            next_fallback = Instant::now() + Duration::from_secs(30);
                        }
                    }
                    if cover_recheck_at.is_some_and(|at: Instant| at <= Instant::now()) {
                        cover_refresh_requested = true;
                        next_cover = Instant::now();
                        cover_recheck_at = Some(Instant::now() + Duration::from_secs(15));
                    }
                    while let Ok(result) = cover_results.try_recv() {
                        cover_pending = false;
                        if let Ok(mut data) = state.lock()
                            && data.generation == result.generation
                            && track_cover_key(&data) == result.key
                        {
                            match result.image {
                                Ok((cover, palette)) => {
                                    let changed =
                                        data.cover.as_ref().is_none_or(|old| **old != *cover);
                                    cache_cover(
                                        &mut cover_cache,
                                        (result.key, cover.clone(), Some(palette)),
                                    );
                                    data.cover = Some(cover);
                                    data.cover_source_url = None;
                                    data.palette = Some(palette);
                                    if changed {
                                        data.cover_revision = data.cover_revision.wrapping_add(1);
                                    }
                                    data.cover_error.clear();
                                }
                                Err(error) => {
                                    if error.code() == windows::Win32::Foundation::E_ABORT {
                                        refresh = true;
                                    }
                                    data.cover_error = error.to_string();
                                    let error = data.cover_error.clone();
                                    remember_cover_failure(&mut data, &error);
                                }
                            }
                            notify(&gui);
                        }
                    }
                    if Instant::now() >= next_metadata {
                        refresh = true;
                    }
                    if refresh {
                        next_metadata = Instant::now()
                            + Duration::from_secs(if state.lock().is_ok_and(|data| data.playing) {
                                5
                            } else {
                                15
                            });
                        if let Some((old, media, play, timeline)) = attached.take() {
                            let _ = old.RemoveMediaPropertiesChanged(media);
                            let _ = old.RemovePlaybackInfoChanged(play);
                            if let Some(timeline) = timeline {
                                let _ = old.RemoveTimelinePropertiesChanged(timeline);
                            }
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
                            missing_since = None;
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
                            let tx = events.clone();
                            let timeline = session
                                .TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
                                    let _ = tx.send(Command::Timeline);
                                    Ok(())
                                }))
                                .ok();
                            if let Ok(op) = session.TryGetMediaPropertiesAsync()
                                && let Ok(props) = await_operation(
                                    op,
                                    Instant::now() + Duration::from_secs(2),
                                    &|| true,
                                )
                            {
                                let title =
                                    props.Title().map(|s| s.to_string()).unwrap_or_default();
                                let artist =
                                    props.Artist().map(|s| s.to_string()).unwrap_or_default();
                                if title.is_empty() {
                                    let same_player = keep_last_track(&state, &source_app);
                                    attached = Some((session, media, play, timeline));
                                    next_metadata = Instant::now() + Duration::from_millis(500);
                                    if !same_player {
                                        requested = None;
                                    }
                                    notify(&gui);
                                } else {
                                    let mut album = props
                                        .AlbumTitle()
                                        .map(|s| s.to_string())
                                        .unwrap_or_default();
                                    if album.is_empty()
                                        && let Ok(data) = state.lock()
                                        && same_song(&data, &source_app, &title, &artist)
                                    {
                                        album.clone_from(&data.album);
                                    }
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
                                            next_cover = Instant::now();
                                            cover_refresh_requested = true;
                                            next_fallback = Instant::now() + Duration::from_secs(4);
                                            cover_recheck_at =
                                                Some(Instant::now() + Duration::from_secs(3));
                                        }
                                        let cached_entry = cover_cache
                                            .iter()
                                            .find(|(k, _, _)| k == &track_key)
                                            .map(|(_, c, p)| (c.clone(), *p));
                                        let (cover, palette) = cached_entry
                                            .map_or((None, None), |(c, p)| (Some(c), p));
                                        let mut should_notify = false;
                                        if let Ok(mut data) = state.lock() {
                                            if changed {
                                                let song_changed =
                                                    !same_song(&data, &source_app, &title, &artist)
                                                        || !data.album.is_empty()
                                                            && data.album != album;
                                                if !song_changed {
                                                    // Album metadata may arrive late or disappear
                                                    // briefly. Keep this song's validated image.
                                                    data.album = album.clone();
                                                    data.clear_lyrics();
                                                    if let Some(cover) = cover.clone() {
                                                        if data
                                                            .cover
                                                            .as_ref()
                                                            .is_none_or(|old| **old != *cover)
                                                        {
                                                            data.cover_revision =
                                                                data.cover_revision.wrapping_add(1);
                                                        }
                                                        data.cover = Some(cover);
                                                        data.cover_source_url = None;
                                                        data.palette = palette;
                                                    }
                                                } else {
                                                    data.generation =
                                                        data.generation.wrapping_add(1);
                                                    data.source_app = source_app;
                                                    data.album = album;
                                                    data.title = title;
                                                    data.artist = artist;
                                                    data.duration = 0.;
                                                    data.position = 0.;
                                                    data.updated = None;
                                                    data.seek_guard = None;
                                                    data.seek_feedback = None;
                                                    data.cover = cover;
                                                    data.cover_source_url = None;
                                                    data.cover_revision =
                                                        data.cover_revision.wrapping_add(1);
                                                    data.palette = palette;
                                                    data.cover_error.clear();
                                                    data.cover_attempts = 0;
                                                    data.clear_lyrics();
                                                    data.error.clear();
                                                }
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
                                    fresh_properties = Some(props);
                                    attached = Some((session, media, play, timeline));
                                }
                            } else {
                                // Preserve the last good snapshot on a transient
                                // property failure from the same media player.
                                let same_player = keep_last_track(&state, &source_app);
                                attached = Some((session, media, play, timeline));
                                next_metadata = Instant::now() + Duration::from_millis(500);
                                if !same_player {
                                    requested = None;
                                }
                                notify(&gui);
                            }
                        } else {
                            attached = None;
                            next_metadata = Instant::now() + Duration::from_millis(250);
                            if let Ok(mut data) = state.lock()
                                && missing_since.get_or_insert_with(Instant::now).elapsed()
                                    >= Duration::from_millis(1500)
                            {
                                if !data.title.is_empty() {
                                    *data = Track {
                                        generation: data.generation.wrapping_add(1),
                                        ..Track::default()
                                    };
                                }
                            }
                            requested = None;
                            notify(&gui);
                        }
                        refresh = false;
                    }
                    if let Some((session, _, _, _)) = &attached {
                        let mut alive = false;
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
                                data.playback_rate = play
                                    .PlaybackRate()
                                    .ok()
                                    .and_then(|rate| rate.Value().ok())
                                    .filter(|rate| rate.is_finite() && *rate > 0.);
                                if let (Ok(end), Ok(start)) = (time.EndTime(), time.StartTime()) {
                                    data.seek_start = start.Duration as f64 / 10_000_000.;
                                    data.duration =
                                        (end.Duration - start.Duration) as f64 / 10_000_000.;
                                }
                                data.can_seek = data.duration > 0.
                                    && play
                                        .Controls()
                                        .and_then(|controls| controls.IsPlaybackPositionEnabled())
                                        .unwrap_or(false);
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
                                        - data.seek_start
                                        + if playing {
                                            since * data.playback_rate.unwrap_or(1.)
                                        } else {
                                            0.
                                        })
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
                                if changed || timeline_changed {
                                    notify(&gui);
                                }
                                let key = format!("{}|{}", data.title, data.artist);
                                if config.load(Ordering::Relaxed)
                                    && !data.title.is_empty()
                                    && lyric_request_due(&data, &requested, Instant::now())
                                {
                                    if requested.as_ref().is_some_and(|(_, duration)| {
                                        (data.duration - duration).abs() >= 2.
                                    }) {
                                        data.clear_lyrics();
                                    }
                                    data.lyrics_retry = None;
                                    if let Some(lines) = crate::lyrics::cached(
                                        &data.title,
                                        &data.artist,
                                        data.duration,
                                        &data.album,
                                    )
                                    .filter(|lines| !lines.is_empty())
                                    {
                                        data.lyrics = lines;
                                        data.lyrics_loaded = true;
                                        data.lyrics_status = crate::lyrics::Status::Ready;
                                        notify(&gui);
                                    }
                                    if data.lyrics.is_empty() {
                                        data.lyrics_status = crate::lyrics::Status::Searching;
                                        data.lyrics_started = Some(Instant::now());
                                        let _ = lyric_jobs.send(crate::lyrics::Job {
                                            generation: data.generation,
                                            title: data.title.clone(),
                                            artist: data.artist.clone(),
                                            album: data.album.clone(),
                                            duration: data.duration,
                                        });
                                    }
                                    requested = Some((key, data.duration));
                                }
                            }
                        }
                        if !alive {
                            refresh = true;
                        } else if !cover_pending && Instant::now() >= next_cover {
                            let job = state
                                .lock()
                                .ok()
                                .filter(|data| {
                                    !data.title.is_empty()
                                        && (data.cover.is_none() || cover_refresh_requested)
                                })
                                .map(|data| CoverJob {
                                    key: track_cover_key(&data),
                                    generation: data.generation,
                                    session: session.clone(),
                                    properties: fresh_properties,
                                });
                            if let Some(job) = job
                                && cover_jobs.try_send(job).is_ok()
                            {
                                cover_pending = true;
                                cover_refresh_requested = false;
                                cover_retries = cover_retries.saturating_add(1);
                                next_cover = Instant::now() + cover_retry_delay(cover_retries);
                                if let Ok(mut data) = state.lock() {
                                    data.cover_attempts = cover_retries;
                                }
                            }
                        }
                    }
                    timeline_changed = false;
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
                        Ok(Command::Refresh) => {
                            refresh = true;
                            cover_refresh_requested = true;
                            next_cover = Instant::now();
                            cover_retries = 0;
                        }
                        Ok(Command::CoverReady) => {}
                        Ok(Command::Timeline) => timeline_changed = true,
                        Ok(Command::Playback) => {
                            next_cover = Instant::now();
                            refresh = true;
                        }
                        Ok(Command::LyricsChanged) => {
                            requested = None;
                            if !config.load(Ordering::Relaxed)
                                && let Ok(mut data) = state.lock()
                            {
                                data.clear_lyrics();
                            }
                        }
                        Ok(command) => {
                            if let Some((session, _, _, _)) = &attached {
                                let action = match command {
                                    Command::Toggle => {
                                        session.TryTogglePlayPauseAsync().and_then(|operation| {
                                            await_operation(
                                                operation,
                                                Instant::now() + Duration::from_secs(2),
                                                &|| true,
                                            )
                                        })
                                    }
                                    Command::Previous => {
                                        session.TrySkipPreviousAsync().and_then(|operation| {
                                            await_operation(
                                                operation,
                                                Instant::now() + Duration::from_secs(2),
                                                &|| true,
                                            )
                                        })
                                    }
                                    Command::Next => {
                                        session.TrySkipNextAsync().and_then(|operation| {
                                            await_operation(
                                                operation,
                                                Instant::now() + Duration::from_secs(2),
                                                &|| true,
                                            )
                                        })
                                    }
                                    Command::Seek {
                                        fraction,
                                        generation,
                                    } => {
                                        let Some((target, start)) = state
                                            .lock()
                                            .ok()
                                            .filter(|track| {
                                                track.generation == generation && track.can_seek
                                            })
                                            .map(|track| {
                                                (
                                                    fraction.clamp(0., 1.) * track.duration,
                                                    track.seek_start,
                                                )
                                            })
                                        else {
                                            continue;
                                        };
                                        let result = session
                                            .TryChangePlaybackPositionAsync(
                                                ((start + target) * 10_000_000.) as i64,
                                            )
                                            .and_then(|operation| {
                                                await_operation(
                                                    operation,
                                                    Instant::now() + Duration::from_secs(2),
                                                    &|| true,
                                                )
                                            });
                                        if let Ok(mut track) = state.lock() {
                                            track.finish_seek(
                                                generation,
                                                target,
                                                matches!(result, Ok(true)),
                                                Instant::now(),
                                            );
                                        }
                                        timeline_changed = true;
                                        result
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
            cover_fallback_enabled: fallback_enabled,
            notify: target,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thumbnail_reads_start_at_zero_even_if_provider_cursor_is_at_eof() {
        use windows::core::Interface;
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let fixture = include_bytes!("../assets/fixtures/cover.png");
        let stream = windows::Storage::Streams::InMemoryRandomAccessStream::new().unwrap();
        let writer = windows::Storage::Streams::DataWriter::CreateDataWriter(&stream).unwrap();
        writer.WriteBytes(fixture).unwrap();
        writer.StoreAsync().unwrap().join().unwrap();
        writer.DetachStream().unwrap();
        stream.Seek(fixture.len() as u64).unwrap();
        let bytes = read_cover_stream(
            &stream.cast().unwrap(),
            Instant::now() + Duration::from_secs(2),
            &|| true,
        )
        .unwrap();
        assert_eq!(bytes, fixture);
        assert!(validated_thumbnail(bytes).is_some());
    }
    #[test]
    fn null_thumbnail_errors_report_missing_metadata_instead_of_success() {
        let error = thumbnail_error("Thumbnail metadata", windows::core::Error::empty());
        assert!(error.code().is_err());
        assert!(
            error
                .to_string()
                .contains("Player returned no thumbnail object")
        );
        let denied = thumbnail_error(
            "Open thumbnail",
            windows::core::Error::from_hresult(windows::Win32::Foundation::E_ACCESSDENIED),
        );
        assert_eq!(denied.code(), windows::Win32::Foundation::E_ACCESSDENIED);
    }
    #[test]
    fn fallback_cannot_replace_native_artwork_or_cross_a_track_boundary() {
        let mut data = Track {
            source_app: "player".into(),
            title: "A".into(),
            artist: "Artist".into(),
            ..Default::default()
        };
        let key = track_cover_key(&data);
        let art = crate::cover_fallback::Artwork {
            bytes: Arc::new(vec![1, 2]),
            palette: [0; 3],
            link: "https://music.apple.com/fixture".into(),
        };
        data.title = "B".into();
        assert!(!apply_fallback(&mut data, &key, art.clone()));
        data.title = "A".into();
        data.cover = Some(Arc::new(vec![9]));
        assert!(!apply_fallback(&mut data, &key, art.clone()));
        assert_eq!(**data.cover.as_ref().unwrap(), vec![9]);
        data.cover = None;
        assert!(apply_fallback(&mut data, &key, art));
        assert!(data.cover_source_url.is_some());
        assert_eq!(data.cover_revision, 1);
        for _ in 0..50 {
            remember_cover_failure(&mut data, "fixture");
        }
        assert_eq!(data.cover_failures.len(), 12);
        assert!(same_song(&data, "player", "A", "Artist"));
        assert!(!same_song(&data, "player", "B", "Artist"));
    }
    #[test]
    fn no_track_has_no_lyrics_status_message() {
        let track = Track {
            lyrics_status: crate::lyrics::Status::Searching,
            ..Default::default()
        };
        assert!(track.lyrics_message().is_empty());
    }

    #[test]
    fn lyric_selection_respects_exact_starts_ends_pause_and_seek() {
        let mut track = Track {
            duration: 10.,
            position: 0.95,
            lyrics: Arc::new(vec![
                crate::lyrics::Line {
                    at: 1.,
                    end: Some(2.),
                    text: "First".into(),
                    ..Default::default()
                },
                crate::lyrics::Line {
                    at: 3.,
                    text: "Next".into(),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        assert_eq!(track.lyric_index(), None);
        track.position = 1.;
        assert_eq!(track.lyric_index(), Some(0));
        track.position = 2.;
        assert_eq!(track.lyric_index(), None);
        track.position = 3.;
        assert_eq!(track.lyric_index(), Some(1));
        track.position = 1.5;
        assert_eq!(track.lyric_index(), Some(0));
        assert_eq!(track.position_now(), 1.5);
    }
    #[test]
    fn lyric_exit_fades_without_delaying_the_next_timestamp() {
        let mut track = Track {
            duration: 12.,
            lyrics: Arc::new(vec![
                crate::lyrics::Line {
                    at: 1.,
                    text: "First".into(),
                    ..Default::default()
                },
                crate::lyrics::Line {
                    at: 2.,
                    end: Some(3.),
                    text: "Second".into(),
                    ..Default::default()
                },
                crate::lyrics::Line {
                    at: 3.,
                    ..Default::default()
                },
                crate::lyrics::Line {
                    at: 5.,
                    text: "Third".into(),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        assert_eq!(track.lyric_frame(1.999, true).active, Some(0));
        let switch = track.lyric_frame(2., true);
        assert_eq!(switch.active, Some(1));
        assert_eq!(switch.outgoing, Some((0, 0.)));
        assert_eq!(switch.entry, 0.);
        let gap = track.lyric_frame(3.08, true);
        assert_eq!(gap.active, None);
        assert_eq!(gap.latest, Some(1));
        assert!(
            gap.outgoing
                .is_some_and(|(index, fade)| { index == 1 && fade > 0. && fade < 1. })
        );
        assert_eq!(track.lyric_frame(3.5, true).outgoing, None);
        assert_eq!(track.lyric_frame(2., false).outgoing, None);
        assert_eq!(track.lyric_frame(2., false).active, Some(1));
        track.position = 4.96;
        assert_eq!(
            track.lyric_boundary_wait(Duration::from_millis(250)),
            Duration::from_millis(40)
        );
        assert_eq!(track.lyric_frame(5., true).active, Some(3));
    }
    #[test]
    fn temporary_metadata_failure_keeps_the_same_players_cover_and_lyrics() {
        let state = Mutex::new(Track {
            source_app: "Player A".into(),
            title: "Fixture".into(),
            cover: Some(Arc::new(vec![1, 2, 3])),
            lyrics: Arc::new(vec![crate::lyrics::Line {
                at: 0.,
                text: "Fixture line".into(),
                ..Default::default()
            }]),
            ..Default::default()
        });
        assert!(keep_last_track(&state, "Player A"));
        assert!(state.lock().unwrap().cover.is_some());
        assert_eq!(state.lock().unwrap().lyrics.len(), 1);
        assert!(!keep_last_track(&state, "Player B"));
        assert!(state.lock().unwrap().cover.is_none());
        assert!(state.lock().unwrap().lyrics.is_empty());
    }
    #[test]
    fn a_stalled_media_operation_does_not_wait_past_its_deadline() {
        let (release, blocked) = mpsc::channel();
        let operation = windows_future::IAsyncOperation::<u32>::spawn(move || {
            blocked.recv().map_err(|_| {
                windows::core::Error::from_hresult(windows::Win32::Foundation::E_ABORT)
            })?;
            Ok(7)
        });
        let result = await_operation(
            operation,
            Instant::now() + Duration::from_millis(20),
            &|| true,
        );
        let _ = release.send(());
        assert_eq!(
            result.unwrap_err().code(),
            windows::core::HRESULT(0x800705B4u32 as i32)
        );
    }
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
        assert_eq!(cover_retry_delay(36).as_secs(), 5);
        assert_eq!(cover_retry_delay(255).as_secs(), 5);
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
    fn failed_or_stale_seek_cannot_reposition_the_lyrics_clock() {
        let now = Instant::now();
        let mut track = Track {
            generation: 12,
            duration: 180.,
            position: 20.,
            ..Default::default()
        };
        track.finish_seek(11, 80., true, now);
        assert_eq!(track.position_now(), 20.);
        track.finish_seek(12, 80., false, now);
        assert_eq!(track.position_now(), 20.);
        assert_eq!(track.seek_feedback, Some(now));
        track.finish_seek(12, 80., true, now);
        assert_eq!(track.position_now(), 80.);
        assert!(track.seek_feedback.is_none());
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
