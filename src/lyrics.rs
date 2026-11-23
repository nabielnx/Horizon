use std::{
    ffi::c_void,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
};
use windows::core::*;
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Line {
    pub at: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<Word>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Word {
    pub at: f64,
    pub end: Option<f64>,
    pub text: String,
}
impl Line {
    pub fn playback_end(&self, next: f64) -> f64 {
        let start = self.at + 0.001;
        let next = next.max(start);
        let end = self.end.unwrap_or(next);
        let end = self.words.last().map_or(end, |last| {
            // Some records mark each word's start but estimate the line end
            // close to the last onset. Allow that word time to finish.
            let last_end = last.end.unwrap_or_else(|| {
                let letters = last.text.trim().chars().count().min(8) as f64;
                last.at + 0.44 + letters * 0.035
            });
            end.max(last_end)
        });
        end.clamp(start, next)
    }

    /// Fractional UTF-16 position; DirectWrite resolves it to glyph geometry.
    pub fn sung_units(&self, position: f64, next: f64) -> f32 {
        let length = self.text.trim_end().encode_utf16().count() as f64;
        let end = self.playback_end(next);
        if self.words.is_empty() {
            return (length * ((position - self.at) / (end - self.at)).clamp(0., 1.)) as f32;
        }
        let mut units = 0.;
        for (index, word) in self.words.iter().enumerate() {
            if position < word.at {
                break;
            }
            let next_word = self.words.get(index + 1);
            let stop = word
                .end
                .or_else(|| next_word.map(|w| w.at))
                .unwrap_or(end)
                .min(end)
                .max(word.at + 0.001);
            let fraction = ((position - word.at) / (stop - word.at)).clamp(0., 1.);
            let text = if next_word.is_none() {
                word.text.trim_end()
            } else {
                &word.text
            };
            units += text.encode_utf16().count() as f64 * fraction;
            if fraction < 1. {
                break;
            }
        }
        units.min(length) as f32
    }
}

pub fn parse_record(record: &serde_json::Value) -> Vec<Line> {
    #[derive(serde::Deserialize)]
    struct Document {
        version: String,
        #[serde(default)]
        lines: Vec<RawLine>,
    }
    #[derive(serde::Deserialize)]
    struct RawLine {
        text: String,
        start_ms: u64,
        end_ms: Option<u64>,
        #[serde(default)]
        words: Vec<RawWord>,
    }
    #[derive(serde::Deserialize)]
    struct RawWord {
        text: String,
        start_ms: u64,
        end_ms: Option<u64>,
    }
    if let Some(yaml) = record["lyricsfile"]
        .as_str()
        .filter(|s| s.len() <= 512 * 1024)
        && let Ok(document) = serde_yaml_ng::from_str::<Document>(yaml)
        && document.version == "1.0"
        && document.lines.len() <= 2048
    {
        let mut lines = Vec::new();
        for line in document.lines {
            if line.text.len() > 4096
                || line.start_ms > 86_400_000
                || line
                    .end_ms
                    .is_some_and(|end| end < line.start_ms || end > 86_400_000)
            {
                continue;
            }
            let valid_words = line.words.len() <= 1024
                && line
                    .words
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<String>()
                    == line.text
                && line
                    .words
                    .windows(2)
                    .all(|w| w[0].start_ms <= w[1].start_ms)
                && line.words.iter().all(|w| {
                    w.start_ms >= line.start_ms
                        && w.start_ms <= 86_400_000
                        && w.end_ms
                            .is_none_or(|end| end >= w.start_ms && end <= 86_400_000)
                });
            let words = if valid_words {
                line.words
                    .into_iter()
                    .map(|w| Word {
                        at: w.start_ms as f64 / 1000.,
                        end: w.end_ms.map(|v| v as f64 / 1000.),
                        text: w.text,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            lines.push(Line {
                at: line.start_ms as f64 / 1000.,
                end: line.end_ms.map(|v| v as f64 / 1000.),
                text: line.text,
                words,
            });
        }
        lines.sort_by(|a, b| a.at.total_cmp(&b.at));
        if !lines.is_empty() {
            return lines;
        }
    }
    record["syncedLyrics"]
        .as_str()
        .map(parse)
        .unwrap_or_default()
}
#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(
        agent: *const u16,
        access: u32,
        proxy: *const u16,
        bypass: *const u16,
        flags: u32,
    ) -> *mut c_void;
    fn WinHttpConnect(
        session: *mut c_void,
        host: *const u16,
        port: u16,
        reserved: u32,
    ) -> *mut c_void;
    fn WinHttpOpenRequest(
        connection: *mut c_void,
        verb: *const u16,
        path: *const u16,
        version: *const u16,
        referer: *const u16,
        types: *const *const u16,
        flags: u32,
    ) -> *mut c_void;
    fn WinHttpSetTimeouts(
        handle: *mut c_void,
        resolve: i32,
        connect: i32,
        send: i32,
        receive: i32,
    ) -> i32;
    fn WinHttpSendRequest(
        request: *mut c_void,
        headers: *const u16,
        header_len: u32,
        optional: *const c_void,
        optional_len: u32,
        total: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(request: *mut c_void, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        request: *mut c_void,
        level: u32,
        name: *const u16,
        buffer: *mut c_void,
        len: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpReadData(
        request: *mut c_void,
        buffer: *mut c_void,
        length: u32,
        read: *mut u32,
    ) -> i32;
    fn WinHttpCloseHandle(handle: *mut c_void) -> i32;
}
struct Handle(*mut c_void);
impl Handle {
    fn new(value: *mut c_void) -> Result<Self> {
        if value.is_null() {
            Err(Error::from_thread())
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
enum Response {
    Ok(Vec<u8>),
    NotFound,
    Transient,
    ServerDown,
}
static LAST_HTTP_STATUS: AtomicU32 = AtomicU32::new(0);
const NEGATIVE_TTL: u64 = 30;
const POSITIVE_TTL: u64 = 30 * 86400;
const LOOKUP_BUDGET: std::time::Duration = std::time::Duration::from_secs(8);
const CACHE_FORMAT: u8 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub enum Status {
    #[default]
    Searching,
    Ready,
    Unavailable,
    NotFound,
    Unsupported,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Attempt {
    pub endpoint: &'static str,
    pub elapsed_ms: u64,
    pub http_status: u32,
    pub outcome: &'static str,
}

pub fn status_message(status: Status, elapsed: std::time::Duration) -> &'static str {
    match status {
        Status::Searching if elapsed.as_secs() >= 3 => "Layanan lirik lambat…",
        Status::Searching => "Mencari lirik…",
        Status::Unavailable => "Layanan lirik belum merespons",
        Status::NotFound => "Lirik sinkron belum ditemukan",
        Status::Unsupported => "Lirik tidak dicari untuk media ini",
        Status::Ready => "",
    }
}

struct HttpClient {
    connection: Handle,
    _session: Handle,
}
impl HttpClient {
    fn new() -> Result<Self> {
        Self::for_host("lrclib.net")
    }
    fn for_host(host: &str) -> Result<Self> {
        unsafe {
            let session = Handle::new(WinHttpOpen(
                w!("Horizon/0.3 (https://github.com/nabielnx/Horizon)").as_ptr(),
                4,
                std::ptr::null(),
                std::ptr::null(),
                0,
            ))?;
            if WinHttpSetTimeouts(session.0, 1500, 2000, 2000, 2500) == 0 {
                return Err(Error::from_thread());
            }
            let connection = Handle::new(WinHttpConnect(
                session.0,
                HSTRING::from(host).as_ptr(),
                443,
                0,
            ))?;
            Ok(Self {
                connection,
                _session: session,
            })
        }
    }
}
/// Bounded HTTPS for artwork, deliberately independent of LRCLIB cooldowns.
pub(crate) fn artwork_get(
    host: &str,
    path: &str,
    limit: usize,
    alive: &impl Fn() -> bool,
) -> Result<Vec<u8>> {
    unsafe {
        if !alive() {
            return Err(Error::from_hresult(windows::Win32::Foundation::E_ABORT));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
        let client = HttpClient::for_host(host)?;
        let path = HSTRING::from(path);
        let request = Handle::new(WinHttpOpenRequest(
            client.connection.0,
            w!("GET").as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0x00800000,
        ))?;
        if WinHttpSendRequest(request.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == 0
        {
            return Err(Error::from_thread());
        }
        let mut status = 0_u32;
        let mut size = 4;
        if WinHttpQueryHeaders(
            request.0,
            19 | 0x20000000,
            std::ptr::null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(Error::from_thread());
        }
        if status != 200 {
            return Err(Error::new(
                windows::Win32::Foundation::E_FAIL,
                format!("Artwork HTTP {status}"),
            ));
        }
        let mut bytes = Vec::new();
        loop {
            if !alive() || std::time::Instant::now() >= deadline {
                return Err(Error::from_hresult(windows::Win32::Foundation::E_ABORT));
            }
            let mut buf = [0_u8; 8192];
            let mut count = 0;
            if WinHttpReadData(
                request.0,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &mut count,
            ) == 0
            {
                return Err(Error::from_thread());
            }
            if count == 0 {
                return Ok(bytes);
            }
            if bytes.len() + count as usize > limit {
                return Err(Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Artwork response too large",
                ));
            }
            bytes.extend_from_slice(&buf[..count as usize]);
        }
    }
}
thread_local! {
    // Worker-local ownership keeps handles off the UI thread and reuses TLS.
    static HTTP_CLIENT: std::cell::RefCell<Option<HttpClient>> = const { std::cell::RefCell::new(None) };
}

fn get(path: &str, alive: impl Fn() -> bool, deadline: std::time::Instant) -> Result<Response> {
    LAST_HTTP_STATUS.store(0, Ordering::Relaxed);
    if !alive() {
        return Ok(Response::NotFound);
    }
    if retry_at().is_some() {
        return Ok(Response::ServerDown);
    }
    HTTP_CLIENT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(HttpClient::new()?);
        }
        request(slot.as_ref().unwrap(), path, alive, deadline)
    })
}
fn response_retry_after(request: &Handle) -> Option<std::time::Duration> {
    let mut buffer = [0u16; 256];
    let mut length = std::mem::size_of_val(&buffer) as u32;
    let ok = unsafe {
        WinHttpQueryHeaders(
            request.0,
            65535,
            w!("Retry-After").as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    parse_retry_after(
        &String::from_utf16_lossy(&buffer[..end]),
        chrono::Utc::now(),
    )
}
fn parse_retry_after(
    value: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<std::time::Duration> {
    let seconds = value.trim().parse::<u64>().ok().or_else(|| {
        chrono::DateTime::parse_from_rfc2822(value.trim())
            .ok()
            .map(|at| (at.timestamp() - now.timestamp()).max(0) as u64)
    })?;
    Some(std::time::Duration::from_secs(seconds.clamp(1, 604800)))
}
fn request(
    client: &HttpClient,
    path: &str,
    alive: impl Fn() -> bool,
    deadline: std::time::Instant,
) -> Result<Response> {
    LAST_HTTP_STATUS.store(0, Ordering::Relaxed);
    unsafe {
        let path = HSTRING::from(path);
        let request = Handle::new(WinHttpOpenRequest(
            client.connection.0,
            w!("GET").as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0x00800000,
        ))?;
        // Bound DNS/connect/send together, then recompute before receive/read.
        // A slow exact lookup must leave time for a different search route.
        let remaining = deadline
            .saturating_duration_since(std::time::Instant::now())
            .as_millis();
        if remaining == 0 {
            return Ok(Response::Transient);
        }
        let phase = (remaining / 4).clamp(1, 700) as i32;
        if WinHttpSetTimeouts(request.0, phase, phase, phase, phase) == 0 {
            return Err(Error::from_thread());
        }
        let req_send =
            WinHttpSendRequest(request.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0);
        if !alive() {
            return Ok(Response::NotFound);
        }
        if req_send == 0 {
            return Ok(Response::Transient);
        }
        let receive_ms = deadline
            .saturating_duration_since(std::time::Instant::now())
            .as_millis()
            .min(1500) as i32;
        if receive_ms == 0 {
            return Ok(Response::Transient);
        }
        if WinHttpSetTimeouts(request.0, phase, phase, phase, receive_ms) == 0
            || WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == 0
        {
            return Ok(Response::Transient);
        }
        if !alive() {
            return Ok(Response::NotFound);
        }
        let mut status = 0u32;
        let mut size = 4;
        if WinHttpQueryHeaders(
            request.0,
            19 | 0x20000000,
            std::ptr::null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(Error::from_thread());
        }
        LAST_HTTP_STATUS.store(status, Ordering::Relaxed);
        if matches!(status, 429 | 503)
            && let Some(delay) = response_retry_after(&request)
        {
            if let Ok(mut retry) = RETRY_AT.lock() {
                retry.at = Some(std::time::Instant::now() + delay);
                retry.rate_limited = status == 429;
            }
            return Ok(Response::ServerDown);
        }
        if status == 429 {
            defer_rate_limit();
            return Ok(Response::ServerDown);
        }
        if status >= 500 || status == 408 || status == 403 {
            defer_retry();
            return Ok(Response::ServerDown);
        }
        if status == 404 {
            return Ok(Response::NotFound);
        }
        if status != 200 {
            return Ok(Response::Transient);
        }
        reset_retry();
        let mut result = Vec::new();
        loop {
            if !alive() {
                return Ok(Response::NotFound);
            }
            let receive_ms = deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_millis()
                .min(1500) as i32;
            if receive_ms == 0 {
                return Ok(Response::Transient);
            }
            if WinHttpSetTimeouts(request.0, phase, phase, phase, receive_ms) == 0 {
                return Err(Error::from_thread());
            }
            let mut buf = [0u8; 8192];
            let mut read = 0;
            if WinHttpReadData(
                request.0,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &mut read,
            ) == 0
            {
                return Err(Error::from_thread());
            }
            if read == 0 {
                break;
            }
            if result.len() + read as usize > 512 * 1024 {
                return Err(Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Lyric response too large",
                ));
            }
            result.extend_from_slice(&buf[..read as usize]);
        }
        Ok(Response::Ok(result))
    }
}
pub(crate) fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
pub fn parse(text: &str) -> Vec<Line> {
    let mut lines = vec![];
    for row in text.lines() {
        let mut row = row;
        let mut times = vec![];
        while row.starts_with('[') {
            let Some(end) = row.find(']') else { break };
            let stamp = &row[1..end];
            if let Some((min, sec)) = stamp.split_once(':')
                && let (Ok(min), Ok(sec)) = (min.parse::<u32>(), sec.parse::<f64>())
                && sec.is_finite()
                && (0.0..60.).contains(&sec)
            {
                times.push(min as f64 * 60. + sec);
            }
            row = &row[end + 1..];
        }
        for at in times {
            if row.len() <= 4096 {
                lines.push(Line {
                    at,
                    text: row.trim().chars().take(1024).collect(),
                    ..Default::default()
                });
            }
        }
    }
    lines.sort_by(|a, b| a.at.total_cmp(&b.at));
    lines
}

pub fn clean_title(title: &str) -> String {
    let title = title.trim();
    for (open, close) in [(" (", ")"), (" [", "]"), (" - ", "")] {
        if let Some((base, annotation)) = title.rsplit_once(open)
            && let Some(annotation) = annotation.strip_suffix(close)
            && matches!(
                annotation.trim().to_ascii_lowercase().as_str(),
                "official video" | "official music video" | "official audio" | "lyric video"
            )
        {
            return base.trim().to_owned();
        }
    }
    title.to_owned()
}

type CacheKey = (String, String, u64, String);
struct CacheEntry {
    format_version: u8,
    lines: Arc<Vec<Line>>,
    expires: std::time::Instant,
}
static LYRIC_CACHE: Mutex<std::collections::VecDeque<(CacheKey, CacheEntry)>> =
    Mutex::new(std::collections::VecDeque::new());
struct RetryState {
    at: Option<std::time::Instant>,
    failures: u8,
    rate_limited: bool,
}
static RETRY_AT: Mutex<RetryState> = Mutex::new(RetryState {
    at: None,
    failures: 0,
    rate_limited: false,
});
pub fn retry_at() -> Option<std::time::Instant> {
    let mut retry = RETRY_AT.lock().ok()?;
    if retry.at.is_some_and(|at| at <= std::time::Instant::now()) {
        retry.at = None;
        retry.rate_limited = false;
    }
    retry.at
}
fn retry_seconds(failures: u8) -> u64 {
    match failures {
        0..=1 => 2,
        2 => 5,
        3 => 10,
        _ => 30,
    }
}
fn defer_retry() {
    if let Ok(mut retry) = RETRY_AT.lock() {
        retry.failures = retry.failures.saturating_add(1);
        retry.at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_secs(retry_seconds(retry.failures)),
        );
    }
}
fn defer_rate_limit() {
    if let Ok(mut retry) = RETRY_AT.lock() {
        retry.at = Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
        retry.rate_limited = true;
    }
}
fn reset_retry() {
    if let Ok(mut retry) = RETRY_AT.lock() {
        retry.at = None;
        retry.failures = 0;
        retry.rate_limited = false;
    }
}
fn begin_track_retry() {
    let _ = retry_at();
    if let Ok(mut retry) = RETRY_AT.lock()
        && !retry.rate_limited
    {
        retry.at = None;
        retry.failures = 0;
    }
}
fn cache_key(title: &str, artist: &str, duration: f64, album: &str) -> CacheKey {
    (
        title.trim().to_lowercase(),
        artist.trim().to_lowercase(),
        duration.max(0.).round() as u64,
        album.trim().to_lowercase(),
    )
}
pub fn cached(title: &str, artist: &str, duration: f64, album: &str) -> Option<Arc<Vec<Line>>> {
    let key = cache_key(title, artist, duration, album);
    let mut cache = LYRIC_CACHE.lock().ok()?;
    cache.retain(|(_, entry)| entry.expires > std::time::Instant::now());
    cache
        .iter()
        .find(|(stored, entry)| *stored == key && !entry.lines.is_empty())
        .or_else(|| {
            cache
                .iter()
                .filter(|(stored, entry)| {
                    !entry.lines.is_empty()
                        && stored.0 == key.0
                        && stored.1 == key.1
                        && stored.3 == key.3
                        && stored.2 > 0
                        && key.2 > 0
                        && stored.2.abs_diff(key.2) <= 1
                })
                .min_by_key(|(stored, _)| stored.2.abs_diff(key.2))
        })
        .or_else(|| cache.iter().find(|(stored, _)| *stored == key))
        .map(|(_, entry)| entry.lines.clone())
}
fn store_cache(
    title: &str,
    artist: &str,
    duration: f64,
    album: &str,
    lines: impl Into<Arc<Vec<Line>>>,
) {
    let lines = lines.into();
    let key = cache_key(title, artist, duration, album);
    let ttl = if lines.is_empty() {
        NEGATIVE_TTL
    } else {
        POSITIVE_TTL
    };
    if let Ok(mut cache) = LYRIC_CACHE.lock() {
        if lines.is_empty()
            && cache
                .iter()
                .any(|(stored, entry)| *stored == key && !entry.lines.is_empty())
        {
            return;
        }
        cache.retain(|(stored, _)| *stored != key);
        cache.push_back((
            key,
            CacheEntry {
                format_version: CACHE_FORMAT,
                lines,
                expires: std::time::Instant::now() + std::time::Duration::from_secs(ttl),
            },
        ));
        while cache.len() > 32
            || cache
                .iter()
                .map(|(key, entry)| {
                    key.0.capacity()
                        + key.1.capacity()
                        + key.3.capacity()
                        + entry.lines.capacity() * std::mem::size_of::<Line>()
                        + entry
                            .lines
                            .iter()
                            .map(|line| {
                                line.text.capacity()
                                    + line.words.capacity() * std::mem::size_of::<Word>()
                                    + line
                                        .words
                                        .iter()
                                        .map(|word| word.text.capacity())
                                        .sum::<usize>()
                            })
                            .sum::<usize>()
                })
                .sum::<usize>()
                > 2 * 1024 * 1024
        {
            cache.pop_front();
        }
    }
}

fn title_match_key(title: &str) -> String {
    let title = clean_title(title).to_lowercase().replace(['–', '—'], "-");
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    for (opening, closing) in [(" (", ')'), (" [", ']')] {
        if let Some((name, version)) = title.rsplit_once(opening)
            && let Some(version) = version.strip_suffix(closing)
        {
            return format!("{} - {}", name.trim_end(), version.trim());
        }
    }
    title
}

fn select_synced(
    rows: &[serde_json::Value],
    title: &str,
    artist: &str,
    target_dur: f64,
    album: &str,
) -> Vec<Line> {
    let title = title_match_key(title);
    let artist = artist.trim().to_lowercase();
    let album = album.trim().to_lowercase();
    let mut best = Vec::new();
    let mut best_difference = f64::INFINITY;
    let mut best_album = false;
    let mut ambiguous = false;
    for row in rows {
        if !row["trackName"]
            .as_str()
            .is_some_and(|name| title_match_key(name) == title)
            || artist.is_empty()
            || !row["artistName"]
                .as_str()
                .is_some_and(|name| name.trim().to_lowercase() == artist)
        {
            continue;
        }
        let difference = if target_dur > 10. {
            (row["duration"].as_f64().unwrap_or(0.) - target_dur).abs()
        } else {
            0.
        };
        let album_matches = !album.is_empty()
            && row["albumName"]
                .as_str()
                .is_some_and(|name| name.trim().to_lowercase() == album);
        if difference > 2.
            || best_album && !album_matches
            || album_matches == best_album && difference > best_difference
        {
            continue;
        }
        let lines = parse_record(row);
        if lines.is_empty() {
            continue;
        }
        if album_matches != best_album || difference < best_difference {
            best = lines;
            best_difference = difference;
            best_album = album_matches;
            ambiguous = false;
        } else if best != lines {
            ambiguous = true;
        }
    }
    if ambiguous { Vec::new() } else { best }
}

fn exact_paths(title: &str, artist: &str, duration: f64) -> Vec<String> {
    let duration = if duration.is_finite() && duration > 10. {
        format!("&duration={}", duration.round().clamp(1., 3600.) as u64)
    } else {
        String::new()
    };
    let mut paths = Vec::new();
    for (title, artist) in [
        (title.trim().to_string(), artist.trim()),
        (clean_title(title), artist.trim()),
    ] {
        let path = format!(
            "/api/get?track_name={}&artist_name={}{}",
            encode(&title),
            encode(artist),
            duration
        );
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}
struct Lookup {
    lines: Arc<Vec<Line>>,
    status: Status,
    attempts: Vec<Attempt>,
}
fn lookup_with(
    title: &str,
    artist: &str,
    duration: f64,
    album: &str,
    alive: impl Fn() -> bool,
    deadline: std::time::Instant,
    mut transport: impl FnMut(&str, std::time::Instant) -> Result<Response>,
) -> Lookup {
    let mut result = Lookup {
        lines: Arc::default(),
        status: Status::NotFound,
        attempts: vec![],
    };
    let mut plan: Vec<_> = exact_paths(title, artist, duration)
        .into_iter()
        .map(|p| (p, false))
        .collect();
    let title_clean = clean_title(title);
    let artist = artist.trim();
    plan.extend(
        [
            format!(
                "/api/search?track_name={}&artist_name={}",
                encode(&title_clean),
                encode(artist)
            ),
            format!(
                "/api/search?q={}",
                encode(format!("{artist} {title_clean}").trim())
            ),
            format!("/api/search?q={}", encode(title_clean.trim())),
        ]
        .into_iter()
        .map(|p| (p, true)),
    );
    let mut incomplete = false;
    for (path, search) in plan {
        if !alive() {
            return result;
        }
        if std::time::Instant::now() >= deadline {
            incomplete = true;
            break;
        }
        let started = std::time::Instant::now();
        let response = transport(&path, deadline);
        if !alive() {
            return result;
        }
        let mut stop = false;
        let outcome = match response {
            Ok(Response::Ok(bytes)) => {
                let parsed = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
                if !parsed.as_ref().is_some_and(|value| {
                    if search {
                        value.is_array()
                    } else {
                        value.is_object()
                    }
                }) {
                    incomplete = true;
                    "invalid_response"
                } else {
                    let parsed = parsed.unwrap();
                    let rows = if let Some(rows) = parsed.as_array() {
                        rows.as_slice()
                    } else if album.trim().is_empty()
                        || parsed["albumName"]
                            .as_str()
                            .is_some_and(|name| name.trim().eq_ignore_ascii_case(album.trim()))
                    {
                        std::slice::from_ref(&parsed)
                    } else {
                        &[]
                    };
                    result.lines = Arc::new(select_synced(rows, title, artist, duration, album));
                    if result.lines.is_empty() {
                        "no_synced_match"
                    } else {
                        "found"
                    }
                }
            }
            Ok(Response::NotFound) => "not_found",
            Ok(Response::ServerDown) => {
                incomplete = true;
                stop = true; // Respect provider cooldown / Retry-After, no request storm.
                "provider_unavailable"
            }
            Ok(Response::Transient) | Err(_) => {
                incomplete = true;
                "transport_failed" // A timeout must not suppress the search fallback.
            }
        };
        result.attempts.push(Attempt {
            endpoint: if search { "search" } else { "exact" },
            elapsed_ms: started.elapsed().as_millis() as u64,
            http_status: LAST_HTTP_STATUS.load(Ordering::Relaxed),
            outcome,
        });
        if !result.lines.is_empty() {
            result.status = Status::Ready;
            return result;
        }
        if stop {
            break;
        }
    }
    if incomplete {
        result.status = Status::Unavailable;
    }
    result
}
fn fetch(
    title: &str,
    artist: &str,
    duration: f64,
    album: &str,
    alive: impl Fn() -> bool,
) -> Lookup {
    if let Some(lines) = cached(title, artist, duration, album) {
        return Lookup {
            status: if lines.is_empty() {
                Status::NotFound
            } else {
                Status::Ready
            },
            lines,
            attempts: vec![],
        };
    }
    if duration > 900. {
        return Lookup {
            lines: Arc::default(),
            status: Status::Unsupported,
            attempts: vec![],
        };
    }
    let result = lookup_with(
        title,
        artist,
        duration,
        album,
        &alive,
        std::time::Instant::now() + LOOKUP_BUDGET,
        |path, deadline| get(path, &alive, deadline),
    );
    if alive() {
        match result.status {
            Status::Ready | Status::NotFound => {
                store_cache(title, artist, duration, album, result.lines.clone())
            }
            Status::Unavailable if retry_at().is_none() => defer_retry(),
            _ => {}
        }
    }
    result
}
pub struct Job {
    pub generation: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
}
fn persist_disk_cache(path: &std::path::Path) {
    let now = std::time::Instant::now();
    let unix = crate::lyrics_disk::now();
    let fresh: Vec<_> = match LYRIC_CACHE.lock() {
        Ok(cache) => cache
            .iter()
            .filter(|(_, entry)| !entry.lines.is_empty() && entry.expires > now)
            .map(|(key, entry)| crate::lyrics_disk::Record {
                format_version: entry.format_version,
                title: key.0.clone(),
                artist: key.1.clone(),
                album: key.3.clone(),
                duration: key.2,
                expires: unix + entry.expires.duration_since(now).as_secs(),
                lines: entry.lines.as_ref().clone(),
            })
            .collect(),
        Err(_) => return,
    };
    // Disk history is larger than the hot memory cache. Preserve evicted songs.
    let mut records = crate::lyrics_disk::read(path);
    for record in fresh {
        records.retain(|old| {
            !(old.title == record.title
                && old.artist == record.artist
                && old.album == record.album
                && old.duration == record.duration)
        });
        records.push(record);
    }
    let _ = crate::lyrics_disk::write(path, &records);
}
fn warm_disk_track(path: &std::path::Path, title: &str, artist: &str, duration: f64, album: &str) {
    if cached(title, artist, duration, album).is_some_and(|lines| !lines.is_empty()) {
        return;
    }
    let key = cache_key(title, artist, duration, album);
    if let Some(record) =
        crate::lyrics_disk::matching(path, &key.0, &key.1, key.2, &key.3, CACHE_FORMAT)
    {
        let expires = record.expires;
        let version = record.format_version;
        store_cache(title, artist, duration, album, record.lines);
        if let Ok(mut cache) = LYRIC_CACHE.lock()
            && let Some((_, entry)) = cache.iter_mut().find(|(stored, _)| *stored == key)
        {
            entry.expires = std::time::Instant::now()
                + std::time::Duration::from_secs(
                    expires
                        .saturating_sub(crate::lyrics_disk::now())
                        .min(POSITIVE_TTL),
                );
            entry.format_version = version;
        }
    }
}
pub fn worker(
    state: Arc<Mutex<crate::media::Track>>,
    enabled: Arc<AtomicBool>,
    notify: Arc<std::sync::atomic::AtomicUsize>,
) -> mpsc::Sender<Job> {
    let (send, receive) = mpsc::channel::<Job>();
    std::thread::spawn(move || {
        let disk_path = std::env::var_os("APPDATA")
            .map(|base| std::path::PathBuf::from(base).join("Horizon/lyrics-cache.json"));
        let mut last_track = String::new();
        while let Ok(mut job) = receive.recv() {
            while let Ok(new) = receive.try_recv() {
                job = new;
            }
            if !enabled.load(Ordering::Relaxed) {
                continue;
            }
            let key = format!("{}|{}", job.title, job.artist);
            if key != last_track {
                begin_track_retry();
                last_track = key;
            }
            let matches_job = |track: &crate::media::Track| {
                enabled.load(Ordering::Relaxed)
                    && track.generation == job.generation
                    && track.title == job.title
                    && track.artist == job.artist
                    && track.album == job.album
                    && (track.duration - job.duration).abs() < 2.
            };
            let alive = || state.lock().is_ok_and(|track| matches_job(&track));
            if !alive() {
                continue;
            }
            if let Some(path) = &disk_path {
                warm_disk_track(path, &job.title, &job.artist, job.duration, &job.album);
            }
            if let Some(lines) = cached(&job.title, &job.artist, job.duration, &job.album)
                .filter(|lines| !lines.is_empty())
                && let Ok(mut data) = state.lock()
                && matches_job(&data)
            {
                data.lyrics = lines;
                data.lyrics_loaded = true;
                data.lyrics_status = Status::Ready;
                crate::media::notify(&notify);
            }
            let result = fetch(&job.title, &job.artist, job.duration, &job.album, alive);
            let persist = !result.lines.is_empty() && alive();
            if let Ok(mut data) = state.lock()
                && matches_job(&data)
            {
                let empty = result.lines.is_empty();
                if !empty {
                    data.lyrics = result.lines;
                }
                let server_retry = if empty && result.status == Status::Unavailable {
                    retry_at()
                } else {
                    None
                };
                data.lyrics_retry = server_retry.or_else(|| {
                    (empty && result.status == Status::NotFound).then(|| {
                        std::time::Instant::now() + std::time::Duration::from_secs(NEGATIVE_TTL)
                    })
                });
                data.lyrics_status = if data.lyrics.is_empty() {
                    result.status
                } else {
                    Status::Ready
                };
                data.lyrics_attempts = result.attempts;
                data.lyrics_loaded = data.lyrics_status != Status::Unavailable;
                crate::media::notify(&notify);
            }
            if persist && let Some(path) = &disk_path {
                persist_disk_cache(path);
            }
        }
    });
    send
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_timeout_can_recover_through_search_without_waiting_for_retry() {
        let mut calls = 0;
        let result = lookup_with(
            "Fixture song",
            "Artist",
            180.,
            "",
            || true,
            std::time::Instant::now() + LOOKUP_BUDGET,
            |path, _| {
                calls += 1;
                if path.starts_with("/api/get?") {
                    return Ok(Response::Transient);
                }
                Ok(Response::Ok(br#"[
                    {"trackName":"Another song","artistName":"Artist","duration":180,"syncedLyrics":"[00:01]Wrong"},
                    {"trackName":"Fixture song (Live)","artistName":"Artist","duration":180,"syncedLyrics":"[00:01]Wrong version"},
                    {"trackName":"Fixture song","artistName":"Artist","duration":181,"syncedLyrics":"[00:01]Recovered"}
                ]"#.to_vec()))
            },
        );
        assert_eq!(calls, 2);
        assert_eq!(result.status, Status::Ready);
        assert_eq!(result.lines[0].text, "Recovered");
        assert_eq!(result.attempts[0].outcome, "transport_failed");
    }

    #[test]
    fn album_breaks_ties_between_identically_named_recordings() {
        let rows = serde_json::json!([
            {"trackName":"Song","artistName":"Artist","albumName":"A","duration":180,"syncedLyrics":"[00:01]Version A"},
            {"trackName":"Song","artistName":"Artist","albumName":"B","duration":180,"syncedLyrics":"[00:02]Version B"}
        ]);
        let rows = rows.as_array().unwrap();
        assert!(select_synced(rows, "Song", "Artist", 180., "").is_empty());
        let lines = select_synced(rows, "Song", "Artist", 180., "B");
        assert_eq!(lines[0].text, "Version B");
        store_cache("Song", "Artist", 180., "B", lines);
        assert!(cached("Song", "Artist", 180., "A").is_none());
    }

    #[test]
    fn accepts_punctuation_changes_in_a_live_recording_title() {
        let result = lookup_with(
            "Fixture Song - Live at The Theatre",
            "Artist",
            237.413,
            "Live at The Theatre",
            || true,
            std::time::Instant::now() + LOOKUP_BUDGET,
            |path, _| {
                assert!(path.starts_with("/api/get?"));
                Ok(Response::Ok(
                    br#"{
                    "trackName":"Fixture Song (Live at The Theatre)",
                    "artistName":"Artist",
                    "albumName":"Live at The Theatre",
                    "duration":237,
                    "syncedLyrics":"[00:01]Matched recording"
                }"#
                    .to_vec(),
                ))
            },
        );
        assert_eq!(result.attempts.len(), 1);
        assert_eq!(result.status, Status::Ready);
        assert_ne!(
            title_match_key("Fixture Song - Live at The Theatre"),
            title_match_key("Fixture Song")
        );
        assert_ne!(
            title_match_key("Fixture Song - Live at The Theatre"),
            title_match_key("Fixture Song (Live at The Arena)")
        );
    }
    #[test]
    fn incomplete_and_completed_missing_lookups_have_different_states() {
        for (failed, expected) in [(true, Status::Unavailable), (false, Status::NotFound)] {
            let result = lookup_with(
                "Missing fixture",
                "Artist",
                180.,
                "",
                || true,
                std::time::Instant::now() + LOOKUP_BUDGET,
                |_, _| {
                    Ok(if failed {
                        Response::Transient
                    } else {
                        Response::NotFound
                    })
                },
            );
            assert_eq!(result.status, expected);
            assert_eq!(result.attempts.len(), 4);
        }
        let malformed = lookup_with(
            "Invalid fixture",
            "Artist",
            180.,
            "",
            || true,
            std::time::Instant::now() + LOOKUP_BUDGET,
            |_, _| Ok(Response::Ok(b"null".to_vec())),
        );
        assert_eq!(malformed.status, Status::Unavailable);
    }
    #[test]
    fn budget_exhaustion_and_track_cancellation_stop_fallbacks() {
        let result = lookup_with(
            "Budget fixture",
            "Artist",
            180.,
            "",
            || true,
            std::time::Instant::now() - std::time::Duration::from_secs(1),
            |_, _| panic!("Budget expired"),
        );
        assert_eq!(result.status, Status::Unavailable);
        assert!(result.attempts.is_empty());
        let active = std::cell::Cell::new(true);
        let result = lookup_with(
            "Stale fixture",
            "Artist",
            180.,
            "",
            || active.get(),
            std::time::Instant::now() + LOOKUP_BUDGET,
            |_, _| {
                active.set(false);
                Ok(Response::Ok(
                    br#"{"syncedLyrics":"[00:01]Old song"}"#.to_vec(),
                ))
            },
        );
        assert!(
            result.lines.is_empty(),
            "Never publish a previous track's response"
        );
        let limited = lookup_with(
            "Busy fixture",
            "Artist",
            180.,
            "",
            || true,
            std::time::Instant::now() + LOOKUP_BUDGET,
            |_, _| Ok(Response::ServerDown),
        );
        assert_eq!(
            limited.attempts.len(),
            1,
            "Honor provider cooldown without trying every route"
        );
        assert_eq!(limited.status, Status::Unavailable);
    }
    #[test]
    fn disk_history_exceeds_hot_cache_and_recovers_older_songs() {
        let path = std::env::temp_dir().join(format!(
            "horizon-extended-history-{}.json",
            std::process::id()
        ));
        let rows: Vec<_> = (0..48)
            .map(|i| crate::lyrics_disk::Record {
                format_version: CACHE_FORMAT,
                title: format!("extended fixture {i}"),
                artist: "Archive artist".into(),
                album: String::new(),
                duration: 180,
                expires: crate::lyrics_disk::now() + POSITIVE_TTL,
                lines: vec![Line {
                    at: 1.,
                    text: format!("Saved line {i}"),
                    ..Default::default()
                }],
            })
            .collect();
        crate::lyrics_disk::write(&path, &rows).unwrap();
        assert_eq!(crate::lyrics_disk::read(&path).len(), 48);
        warm_disk_track(&path, "Extended fixture 0", "Archive artist", 181., "");
        assert_eq!(
            cached("Extended fixture 0", "Archive artist", 181., "").unwrap()[0].text,
            "Saved line 0"
        );
        persist_disk_cache(&path);
        let saved = crate::lyrics_disk::read(&path);
        assert!(
            saved.iter().any(|r| r.title == "extended fixture 47"),
            "Saving hot entries must preserve older disk-only entries"
        );
        assert!(
            saved
                .iter()
                .any(|r| r.expires > crate::lyrics_disk::now() + 29 * 86400)
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn slow_service_messages_never_claim_lyrics_are_missing() {
        use std::time::Duration;
        assert_eq!(
            status_message(Status::Searching, Duration::ZERO),
            "Mencari lirik…"
        );
        assert_eq!(
            status_message(Status::Searching, Duration::from_secs(3)),
            "Layanan lirik lambat…"
        );
        assert_ne!(
            status_message(Status::Unavailable, Duration::ZERO),
            status_message(Status::NotFound, Duration::ZERO)
        );
    }
    #[test]
    fn lyricsfile_word_timing_uses_utf16_positions_and_preserves_gaps() {
        let record = serde_json::json!({"lyricsfile": "version: '1.0'\nlines:\n- text: 'Hi world'\n  start_ms: 1000\n  end_ms: 5000\n  words:\n  - {text: 'Hi ', start_ms: 1000, end_ms: 2000}\n  - {text: world, start_ms: 3000, end_ms: 5000}\n"});
        let lines = parse_record(&record);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].words.len(), 2);
        assert_eq!(lines[0].sung_units(0.9, 9.), 0.);
        assert_eq!(lines[0].sung_units(2.5, 9.), 3.);
        assert_eq!(lines[0].sung_units(4., 9.), 5.5);
        assert_eq!(lines[0].sung_units(5., 9.), 8.);
        let malformed = serde_json::json!({"lyricsfile": "version: '9.0'\nlines: []", "syncedLyrics": "[00:01]Fallback"});
        assert_eq!(parse_record(&malformed)[0].text, "Fallback");
    }
    #[test]
    fn a_short_estimated_line_end_leaves_time_for_the_last_word() {
        let mut line = Line {
            at: 1.,
            end: Some(2.3),
            text: "Hi world ".into(),
            words: vec![
                Word {
                    at: 1.,
                    end: None,
                    text: "Hi ".into(),
                },
                Word {
                    at: 2.,
                    end: None,
                    text: "world ".into(),
                },
            ],
        };
        assert!(line.sung_units(2.3, 4.) < 8.);
        assert_eq!(line.sung_units(line.playback_end(4.), 4.), 8.);
        assert_eq!(line.playback_end(2.4), 2.4);
        assert_eq!(line.sung_units(2.4, 2.4), 8.);
        line.words[1].end = Some(2.3);
        assert_eq!(line.playback_end(4.), 2.3);
        assert_eq!(line.sung_units(2.3, 4.), 8.);
    }
    #[test]
    fn old_cache_lines_deserialize_without_fabricated_word_timings() {
        let line: Line = serde_json::from_str(r#"{"at":1.0,"text":"Hi 🎵"}"#).unwrap();
        assert!(line.words.is_empty());
        assert_eq!(line.sung_units(2., 3.), 2.5);
        assert!(
            parse("[00:01]First\n[00:02]\n[00:03]Next")[1]
                .text
                .is_empty()
        );
    }
    #[test]
    fn legacy_cache_requires_a_fresh_verified_match() {
        let path = std::env::temp_dir().join(format!(
            "horizon-legacy-karaoke-{}.json",
            std::process::id()
        ));
        crate::lyrics_disk::write(
            &path,
            &[crate::lyrics_disk::Record {
                format_version: 0,
                title: "Legacy karaoke fixture".into(),
                artist: "Fixture artist".into(),
                album: String::new(),
                duration: 120,
                expires: crate::lyrics_disk::now() + 300,
                lines: vec![Line {
                    at: 1.,
                    text: "Keep this cached line".into(),
                    ..Default::default()
                }],
            }],
        )
        .unwrap();
        warm_disk_track(&path, "Legacy karaoke fixture", "Fixture artist", 120., "");
        assert!(cached("Legacy karaoke fixture", "Fixture artist", 120., "").is_none());
        store_cache(
            "Legacy karaoke fixture",
            "Fixture artist",
            121.,
            "",
            vec![Line {
                at: 1.,
                text: "Updated duration".into(),
                ..Default::default()
            }],
        );

        store_cache(
            "Legacy karaoke fixture",
            "Fixture artist",
            120.,
            "",
            Vec::new(),
        );
        assert!(
            !cached("Legacy karaoke fixture", "Fixture artist", 120., "")
                .unwrap()
                .is_empty()
        );
        store_cache(
            "Legacy karaoke fixture",
            "Fixture artist",
            120.,
            "",
            vec![Line {
                at: 1.,
                text: "Refreshed".into(),
                ..Default::default()
            }],
        );

        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn exact_lookup_precedes_search_even_without_duration_and_preserves_band_names() {
        let paths = exact_paths("Song", "AC/DC", 0.);
        assert!(paths[0].starts_with("/api/get?"));
        assert!(paths[0].contains("artist_name=AC%2FDC"));
        assert!(!paths[0].contains("duration="));
        assert_eq!(exact_paths("Song", "Artist", 171.).len(), 1);
        let cleaned = exact_paths("Song (Official Video)", "Artist", 171.);
        assert_eq!(cleaned.len(), 2);
        assert!(cleaned[1].contains("track_name=Song&"));
    }
    #[test]
    fn retry_after_accepts_server_seconds_and_http_dates() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-02T02:00:00Z")
            .unwrap()
            .to_utc();
        assert_eq!(parse_retry_after("1", now).unwrap().as_secs(), 1);
        assert_eq!(
            parse_retry_after("Mon, 02 Nov 2026 02:00:07 GMT", now)
                .unwrap()
                .as_secs(),
            7
        );
        assert!(parse_retry_after("invalid", now).is_none());
    }
    #[test]
    fn disk_cache_warms_memory_and_tolerates_small_duration_changes() {
        let path =
            std::env::temp_dir().join(format!("horizon-lyrics-warm-{}.json", std::process::id()));
        let lines = vec![Line {
            at: 1.,
            text: "Cache fixture".into(),
            ..Default::default()
        }];
        crate::lyrics_disk::write(
            &path,
            &[crate::lyrics_disk::Record {
                format_version: CACHE_FORMAT,
                title: "Disk cache fixture".into(),
                artist: "Artist".into(),
                album: String::new(),
                duration: 200,
                expires: crate::lyrics_disk::now() + 3600,
                lines: lines.clone(),
            }],
        )
        .unwrap();
        warm_disk_track(&path, "Disk cache fixture", "Artist", 200., "");
        let exact = cached("Disk cache fixture", "Artist", 200., "").unwrap();
        let rounded = cached("Disk cache fixture", "Artist", 201., "").unwrap();
        assert_eq!(*exact, lines);
        assert!(Arc::ptr_eq(&exact, &rounded));
        assert!(cached("Disk cache fixture", "Artist", 210., "").is_none());
        assert!(cached("Disk cache fixture (Live)", "Artist", 200., "").is_none());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn server_cooldown_expires_without_poisoning_the_cache() {
        defer_retry();
        assert!(retry_at().is_some());
        RETRY_AT.lock().unwrap().at =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        assert!(retry_at().is_none());
        assert_eq!([1, 2, 3, 4].map(retry_seconds), [2, 5, 10, 30]);
        defer_retry();
        begin_track_retry();
        assert!(retry_at().is_none());
        defer_rate_limit();
        begin_track_retry();
        assert!(retry_at().is_some());
        reset_retry();
    }
    #[test]
    fn cache_separates_versions_and_expires_negative_results() {
        store_cache("Cache fixture (Live)", "Artist", 210., "", vec![]);
        assert!(cached("Cache fixture (Live)", "Artist", 210., "").is_some());
        assert!(cached("Cache fixture", "Artist", 210., "").is_none());
        assert!(cached("Cache fixture (Live)", "Artist", 240., "").is_none());
        let key = cache_key("Cache fixture (Live)", "Artist", 210., "");
        if let Ok(mut cache) = LYRIC_CACHE.lock() {
            cache
                .iter_mut()
                .find(|(stored, _)| *stored == key)
                .unwrap()
                .1
                .expires = std::time::Instant::now() - std::time::Duration::from_secs(1);
        }
        assert!(cached("Cache fixture (Live)", "Artist", 210., "").is_none());
    }
    #[test]
    #[ignore = "explicit live LRCLIB connectivity check; no personal metadata"]
    fn live_lookup() {
        assert!(
            !fetch("I Want to Live", "Borislav Slavov", 233., "", || true)
                .lines
                .is_empty(),
            "Live LRCLIB lookup returned no lyrics; last HTTP status: {}",
            LAST_HTTP_STATUS.load(Ordering::Relaxed)
        );
    }
    #[test]
    fn timestamps_sort_and_repeat() {
        let l = parse("[ar:meta]\n[00:10.50]Later\n[00:01.00][00:03.00]Early");
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].at, 1.);
        assert_eq!(l[1].text, "Early");
        assert_eq!(l[2].at, 10.5);
    }
    #[test]
    fn metadata_and_invalid_times_are_ignored() {
        assert!(parse("[99:NaN]bad\n[00:99]bad\n[ar:hello]artist").is_empty());
        assert_eq!(encode("a & b"), "a%20%26%20b");
    }
}
