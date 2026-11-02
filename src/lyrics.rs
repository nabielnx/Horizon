use std::{
    ffi::c_void,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
};
use windows::core::*;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Line {
    pub at: f64,
    pub text: String,
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
    ServerDown,
}
static LAST_HTTP_STATUS: AtomicU32 = AtomicU32::new(0);

fn get(path: &str, alive: impl Fn() -> bool) -> Result<Response> {
    if retry_at().is_some() {
        return Ok(Response::ServerDown);
    }
    LAST_HTTP_STATUS.store(0, Ordering::Relaxed);
    unsafe {
        let session = Handle::new(WinHttpOpen(
            w!("Horizon/0.3 (https://github.com/nabielnx/Horizon)").as_ptr(),
            4,
            std::ptr::null(),
            std::ptr::null(),
            0,
        ))?;
        if WinHttpSetTimeouts(session.0, 3000, 3000, 3000, 3500) == 0 {
            return Err(Error::from_thread());
        }
        let connection = Handle::new(WinHttpConnect(session.0, w!("lrclib.net").as_ptr(), 443, 0))?;
        let path = HSTRING::from(path);
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            w!("GET").as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0x00800000,
        ))?;
        let req_send =
            WinHttpSendRequest(request.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0);
        if req_send == 0 || WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == 0 {
            defer_retry();
            return Ok(Response::ServerDown);
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
        if status == 429 {
            defer_rate_limit();
            return Ok(Response::ServerDown);
        }
        if status >= 500 || status == 408 || status == 403 {
            defer_retry();
            return Ok(Response::ServerDown);
        }
        if status != 200 {
            return Ok(Response::NotFound);
        }
        reset_retry();
        let mut result = Vec::new();
        loop {
            if !alive() {
                return Ok(Response::NotFound);
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
fn encode(text: &str) -> String {
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
            if !row.trim().is_empty() {
                lines.push(Line {
                    at,
                    text: row.trim().chars().take(1024).collect(),
                });
            }
        }
    }
    lines.sort_by(|a, b| a.at.total_cmp(&b.at));
    lines
}

pub fn clean_title(title: &str) -> String {
    let mut t = title.to_string();
    if let Some((head, _)) = t.split_once(" - ") {
        t = head.to_string();
    }
    if let Some((head, _)) = t.split_once('(') {
        t = head.to_string();
    }
    if let Some((head, _)) = t.split_once('[') {
        t = head.to_string();
    }
    t.trim().to_string()
}

pub fn first_artist(artist: &str) -> &str {
    artist
        .split(&[',', ';', '/', '&'][..])
        .next()
        .unwrap_or(artist)
        .trim()
}

type CacheKey = (String, String, u64);
struct CacheEntry {
    lines: Vec<Line>,
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
fn cache_key(title: &str, artist: &str, duration: f64) -> CacheKey {
    (
        title.trim().to_lowercase(),
        artist.trim().to_lowercase(),
        duration.max(0.).round() as u64,
    )
}
pub fn cached(title: &str, artist: &str, duration: f64) -> Option<Vec<Line>> {
    let key = cache_key(title, artist, duration);
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
                        && stored.2 > 0
                        && key.2 > 0
                        && stored.2.abs_diff(key.2) <= 1
                })
                .min_by_key(|(stored, _)| stored.2.abs_diff(key.2))
        })
        .or_else(|| cache.iter().find(|(stored, _)| *stored == key))
        .map(|(_, entry)| entry.lines.clone())
}
pub fn store_cache(title: &str, artist: &str, duration: f64, lines: Vec<Line>) {
    let key = cache_key(title, artist, duration);
    let ttl = if lines.is_empty() { 300 } else { 86400 };
    if let Ok(mut cache) = LYRIC_CACHE.lock() {
        cache.retain(|(stored, _)| *stored != key);
        cache.push_back((
            key,
            CacheEntry {
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
                        + entry.lines.capacity() * std::mem::size_of::<Line>()
                        + entry
                            .lines
                            .iter()
                            .map(|line| line.text.capacity())
                            .sum::<usize>()
                })
                .sum::<usize>()
                > 2 * 1024 * 1024
        {
            cache.pop_front();
        }
    }
}

enum SearchResult {
    Found(String),
    NotFound,
    ServerDown,
}

fn search_synced(
    path: &str,
    target_dur: f64,
    artist: &str,
    alive: &impl Fn() -> bool,
) -> SearchResult {
    let bytes = match get(path, alive) {
        Ok(Response::Ok(bytes)) => bytes,
        Ok(Response::ServerDown) => return SearchResult::ServerDown,
        Err(_) => {
            defer_retry();
            return SearchResult::ServerDown;
        }
        _ => return SearchResult::NotFound,
    };
    let Some(data) = serde_json::from_slice::<serde_json::Value>(&bytes).ok() else {
        return SearchResult::NotFound;
    };
    let Some(rows) = data.as_array() else {
        return SearchResult::NotFound;
    };

    if target_dur > 10. {
        let mut best: Option<(&serde_json::Value, f64)> = None;
        for row in rows {
            if !artist.is_empty()
                && !row["artistName"]
                    .as_str()
                    .is_some_and(|name| first_artist(name).eq_ignore_ascii_case(artist))
            {
                continue;
            }
            if row["syncedLyrics"].as_str().is_some() {
                let dur = row["duration"].as_f64().unwrap_or(0.);
                let diff = (dur - target_dur).abs();
                if diff <= 12. && best.as_ref().is_none_or(|b| diff < b.1) {
                    best = Some((row, diff));
                }
            }
        }
        if let Some((row, _)) = best
            && let Some(lrc) = row["syncedLyrics"].as_str()
        {
            return SearchResult::Found(lrc.to_string());
        }
    }

    if target_dur > 10. {
        return SearchResult::NotFound;
    }
    if let Some(lrc) = rows
        .iter()
        .filter(|row| {
            artist.is_empty()
                || row["artistName"]
                    .as_str()
                    .is_some_and(|name| first_artist(name).eq_ignore_ascii_case(artist))
        })
        .find_map(|row| row["syncedLyrics"].as_str())
    {
        SearchResult::Found(lrc.to_string())
    } else {
        SearchResult::NotFound
    }
}

fn fetch(title: &str, artist: &str, duration: f64, alive: impl Fn() -> bool) -> Vec<Line> {
    if !alive() {
        return vec![];
    }
    if let Some(cached) = cached(title, artist, duration) {
        return cached;
    }
    // Skip non-music media / long video recaps (> 15 minutes)
    if duration > 900.0 {
        store_cache(title, artist, duration, vec![]);
        return vec![];
    }
    let fa = first_artist(artist);
    let ct = clean_title(title);

    // 1. Exact match with original title + first artist + duration
    if duration > 10. {
        let path = format!(
            "/api/get?track_name={}&artist_name={}&duration={}",
            encode(title),
            encode(fa),
            duration.round().clamp(1., 3600.) as u64
        );
        match get(&path, &alive) {
            Ok(Response::Ok(bytes)) => {
                if let Ok(data) = serde_json::from_slice::<serde_json::Value>(&bytes)
                    && let Some(text) = data["syncedLyrics"].as_str()
                {
                    let lines = parse(text);
                    if !lines.is_empty() {
                        store_cache(title, artist, duration, lines.clone());
                        return lines;
                    }
                }
            }
            Ok(Response::ServerDown) => {
                return vec![];
            }
            Err(_) => {
                defer_retry();
                return vec![];
            }
            _ => {}
        }
        if !alive() {
            return vec![];
        }
    }

    // 2. Search with clean title and first artist on /api/search (high reliability)
    let path = format!(
        "/api/search?track_name={}&artist_name={}",
        encode(&ct),
        encode(fa)
    );
    match search_synced(&path, duration, fa, &alive) {
        SearchResult::Found(text) => {
            let lines = parse(&text);
            if !lines.is_empty() {
                store_cache(title, artist, duration, lines.clone());
                return lines;
            }
        }
        SearchResult::ServerDown => {
            return vec![];
        }
        SearchResult::NotFound => {}
    }
    if !alive() {
        return vec![];
    }

    // 3. Search query: "/api/search?q=artist title"
    let query = format!("{} {}", fa, ct);
    let path = format!("/api/search?q={}", encode(query.trim()));
    match search_synced(&path, duration, fa, &alive) {
        SearchResult::Found(text) => {
            let lines = parse(&text);
            if !lines.is_empty() {
                store_cache(title, artist, duration, lines.clone());
                return lines;
            }
        }
        SearchResult::ServerDown => {
            return vec![];
        }
        SearchResult::NotFound => {}
    }
    if !alive() {
        return vec![];
    }

    // 4. Search query: "/api/search?q=title"
    let path = format!("/api/search?q={}", encode(ct.trim()));
    match search_synced(&path, duration, fa, &alive) {
        SearchResult::Found(text) => {
            let lines = parse(&text);
            if !lines.is_empty() {
                store_cache(title, artist, duration, lines.clone());
                return lines;
            }
        }
        SearchResult::ServerDown => return vec![],
        SearchResult::NotFound => {}
    }

    if !alive() {
        return vec![];
    }
    store_cache(title, artist, duration, vec![]);
    vec![]
}
pub struct Job {
    pub title: String,
    pub artist: String,
    pub duration: f64,
}
fn load_disk_cache(path: &std::path::Path) {
    let unix = crate::lyrics_disk::now();
    for record in crate::lyrics_disk::read(path) {
        let key = cache_key(&record.title, &record.artist, record.duration as f64);
        store_cache(
            &record.title,
            &record.artist,
            record.duration as f64,
            record.lines,
        );
        if let Ok(mut cache) = LYRIC_CACHE.lock()
            && let Some((_, entry)) = cache.iter_mut().find(|(stored, _)| *stored == key)
        {
            entry.expires = std::time::Instant::now()
                + std::time::Duration::from_secs(record.expires.saturating_sub(unix).min(86400));
        }
    }
}
fn persist_disk_cache(path: &std::path::Path) {
    let now = std::time::Instant::now();
    let unix = crate::lyrics_disk::now();
    let records: Vec<_> = match LYRIC_CACHE.lock() {
        Ok(cache) => cache
            .iter()
            .filter(|(_, entry)| !entry.lines.is_empty() && entry.expires > now)
            .map(|(key, entry)| crate::lyrics_disk::Record {
                title: key.0.clone(),
                artist: key.1.clone(),
                duration: key.2,
                expires: unix + entry.expires.duration_since(now).as_secs(),
                lines: entry.lines.clone(),
            })
            .collect(),
        Err(_) => return,
    };
    let _ = crate::lyrics_disk::write(path, &records);
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
        let mut disk_loaded = false;
        let mut last_track = String::new();
        while let Ok(mut job) = receive.recv() {
            while let Ok(new) = receive.try_recv() {
                job = new;
            }
            if !enabled.load(Ordering::Relaxed) {
                continue;
            }
            if !disk_loaded {
                if let Some(path) = &disk_path {
                    load_disk_cache(path);
                }
                disk_loaded = true;
            }
            let key = format!("{}|{}", job.title, job.artist);
            if key != last_track {
                begin_track_retry();
                last_track = key;
            }
            let alive = || {
                enabled.load(Ordering::Relaxed)
                    && state.lock().is_ok_and(|s| {
                        s.title == job.title
                            && s.artist == job.artist
                            && (s.duration - job.duration).abs() < 2.
                    })
            };
            let lines = fetch(&job.title, &job.artist, job.duration, alive);
            let persist = !lines.is_empty() && alive();
            if alive()
                && let Ok(mut data) = state.lock()
            {
                let empty = lines.is_empty();
                if !empty {
                    data.lyrics = Arc::new(lines);
                } else {
                    data.lyrics = Arc::new(vec![]);
                }
                let server_retry = if empty { retry_at() } else { None };
                data.lyrics_retry = server_retry.or_else(|| {
                    empty.then(|| std::time::Instant::now() + std::time::Duration::from_secs(300))
                });
                data.lyrics_loaded = server_retry.is_none();
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
    fn disk_cache_warms_memory_and_tolerates_small_duration_changes() {
        let path =
            std::env::temp_dir().join(format!("horizon-lyrics-warm-{}.json", std::process::id()));
        let lines = vec![Line {
            at: 1.,
            text: "Cache fixture".into(),
        }];
        crate::lyrics_disk::write(
            &path,
            &[crate::lyrics_disk::Record {
                title: "Disk cache fixture".into(),
                artist: "Artist".into(),
                duration: 200,
                expires: crate::lyrics_disk::now() + 3600,
                lines: lines.clone(),
            }],
        )
        .unwrap();
        load_disk_cache(&path);
        assert_eq!(cached("Disk cache fixture", "Artist", 200.).unwrap(), lines);
        assert_eq!(cached("Disk cache fixture", "Artist", 201.).unwrap(), lines);
        assert!(cached("Disk cache fixture", "Artist", 210.).is_none());
        assert!(cached("Disk cache fixture (Live)", "Artist", 200.).is_none());
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
        store_cache("Cache fixture (Live)", "Artist", 210., vec![]);
        assert!(cached("Cache fixture (Live)", "Artist", 210.).is_some());
        assert!(cached("Cache fixture", "Artist", 210.).is_none());
        assert!(cached("Cache fixture (Live)", "Artist", 240.).is_none());
        let key = cache_key("Cache fixture (Live)", "Artist", 210.);
        if let Ok(mut cache) = LYRIC_CACHE.lock() {
            cache
                .iter_mut()
                .find(|(stored, _)| *stored == key)
                .unwrap()
                .1
                .expires = std::time::Instant::now() - std::time::Duration::from_secs(1);
        }
        assert!(cached("Cache fixture (Live)", "Artist", 210.).is_none());
    }
    #[test]
    #[ignore = "explicit live LRCLIB connectivity check; no personal metadata"]
    fn live_lookup() {
        assert!(
            !fetch("I Want to Live", "Borislav Slavov", 233., || true).is_empty(),
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
