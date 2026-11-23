//! Spotify catalog and Connect playback. All network and OAuth work is on demand.
use crate::{assistant, calendar, lyrics};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Networking::WinHttp::*,
        Security::Cryptography::*,
        UI::Shell::{SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW, ShellExecuteExW},
        UI::WindowsAndMessaging::{PostMessageW, WM_APP},
    },
    core::*,
};

pub const REDIRECT: &str = "http://127.0.0.1:43871/spotify/callback";
pub const READY: u32 = WM_APP + 19;
pub const EDIT_ACTION: u32 = WM_APP + 20;
const CALLBACK: &str = "/spotify/callback";
const SCOPES: &str = "user-read-playback-state user-modify-playback-state";
const NETWORK: &str = "Tidak dapat terhubung ke Spotify.";
const LOGIN: &str = "Sambungkan ulang akun Spotify.";
const TOKEN_STORE: &str = "Sesi Spotify gagal disimpan di Windows.";
const NO_DEVICE: &str = "Buka Spotify desktop dengan akun yang sama, lalu coba lagi.";
type SpotifyResult<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub artist: String,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    Free,
    Setup,
    Connected,
}

#[derive(Default)]
pub struct Search {
    pub open: bool,
    pub songs: Vec<Song>,
    pub offset: usize,
    pub notice: &'static str,
    pub mode: SearchMode,
    pub query: String,
    pub edit: HWND,
    token: Option<Token>,
    pending: Option<Receiver<Completion>>,
    initialized: bool,
}

struct Token {
    access: String,
    expires: Instant,
}

pub enum Job {
    Connect,
    Search(String),
    Play(String),
}

pub enum Update {
    Connected(String),
    Songs(Vec<Song>),
    Playing,
}

struct Completion {
    result: SpotifyResult<Update>,
    token: Option<Token>,
}

impl Search {
    pub fn restore(&mut self, client_id: &str) {
        if !self.initialized {
            self.initialized = true;
            if valid_id(client_id)
                && assistant::load_key(&credential(client_id)).is_ok_and(|key| key.is_some())
            {
                self.mode = SearchMode::Connected;
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn start(&mut self, hwnd: HWND, client_id: String, job: Job) {
        if self.busy() {
            return;
        }
        self.notice = match &job {
            Job::Connect => "Selesaikan login di browser…",
            Job::Search(_) => "Mencari lagu…",
            Job::Play(_) => "Memulai Spotify…",
        };
        let mut token = self.token.take();
        let (tx, rx) = mpsc::channel();
        let hwnd = hwnd.0 as usize;
        match thread::Builder::new().spawn(move || {
            let result = run_job(&client_id, &mut token, job);
            let _ = tx.send(Completion { result, token });
            unsafe {
                let _ = PostMessageW(Some(HWND(hwnd as _)), READY, WPARAM(0), LPARAM(0));
            }
        }) {
            Ok(_) => self.pending = Some(rx),
            Err(_) => self.notice = NETWORK,
        }
    }

    pub fn finish(&mut self) -> Option<SpotifyResult<Update>> {
        let outcome = match self.pending.as_ref()?.try_recv() {
            Ok(outcome) => outcome,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Completion {
                result: Err(NETWORK),
                token: None,
            },
        };
        self.pending = None;
        self.token = outcome.token;
        self.notice = "";
        if let Err(error) = outcome.result {
            self.notice = error;
            if error == LOGIN {
                self.mode = SearchMode::Setup;
                self.songs.clear();
                self.offset = 0;
            }
        }
        Some(outcome.result)
    }

    pub fn disconnect(&mut self, client_id: &str) -> bool {
        if assistant::delete_key(&credential(client_id)).is_err() {
            self.notice = "Sesi Spotify gagal dihapus dari Windows.";
            return false;
        }
        self.token = None;
        self.mode = SearchMode::Free;
        self.notice = "";
        self.songs.clear();
        self.offset = 0;
        true
    }
}

pub fn search_url(query: &str, browser: bool) -> Option<String> {
    let query = query.trim();
    let prefix = if browser {
        "https://open.spotify.com/search/"
    } else {
        "spotify:search:"
    };
    (!query.is_empty()).then(|| format!("{prefix}{}", lyrics::encode(query)))
}

pub fn open_desktop_search(hwnd: HWND, query: &str) -> bool {
    let Some(uri) = search_url(query, false) else {
        return false;
    };
    let uri = HSTRING::from(uri);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
        hwnd,
        lpVerb: w!("open"),
        lpFile: PCWSTR(uri.as_ptr()),
        nShow: 1,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).is_ok() }
}

pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn credential(client_id: &str) -> HSTRING {
    HSTRING::from(format!("Horizon:Spotify:{client_id}"))
}

fn form(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{key}={}", lyrics::encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn opaque_ascii(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_graphic())
}

fn random_value() -> SpotifyResult<String> {
    let mut bytes = [0; 32];
    unsafe {
        BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
            .ok()
            .map_err(|_| NETWORK)?;
    }
    base64url(&bytes)
}

fn base64url(bytes: &[u8]) -> SpotifyResult<String> {
    assistant::encode_base64(bytes)
        .map(|value| {
            value
                .trim_end_matches('=')
                .replace('+', "-")
                .replace('/', "_")
        })
        .map_err(|_| NETWORK)
}

fn challenge(verifier: &str) -> SpotifyResult<String> {
    let mut hash = [0; 32];
    unsafe {
        BCryptHash(
            BCRYPT_SHA256_ALG_HANDLE,
            None,
            verifier.as_bytes(),
            &mut hash,
        )
        .ok()
        .map_err(|_| NETWORK)?;
    }
    base64url(&hash)
}

fn callback(target: &str, state: &str) -> Option<SpotifyResult<String>> {
    let (path, query) = target.split_once('?')?;
    if path != CALLBACK {
        return None;
    }
    let pairs: Vec<_> = query
        .split('&')
        .filter_map(|part| part.split_once('='))
        .collect();
    if pairs.iter().filter(|(key, _)| *key == "state").count() != 1
        || !pairs.contains(&("state", state))
    {
        return None;
    }
    if pairs.iter().any(|(key, _)| *key == "error") {
        return Some(Err("Login Spotify dibatalkan."));
    }
    let mut codes = pairs.iter().filter(|(key, _)| *key == "code");
    let code = codes.next()?.1;
    // Spotify supplies URL-encoded parameters. Decode once before form-encoding
    // the authorization code again for the token exchange.
    if codes.next().is_some() {
        return None;
    }
    let uri = windows::Foundation::Uri::CreateUri(&HSTRING::from(format!(
        "http://127.0.0.1{CALLBACK}?code={code}"
    )))
    .ok()?;
    let code = uri
        .QueryParsed()
        .ok()?
        .GetFirstValueByName(&HSTRING::from("code"))
        .ok()?
        .to_string();
    opaque_ascii(&code).then_some(Ok(code))
}

fn authorize(client_id: &str) -> SpotifyResult<Token> {
    let listener = TcpListener::bind(("127.0.0.1", 43871))
        .map_err(|_| "Port login Spotify sedang dipakai. Coba lagi.")?;
    listener.set_nonblocking(true).map_err(|_| NETWORK)?;
    let state = random_value()?;
    let verifier = random_value()?;
    let url = format!(
        "https://accounts.spotify.com/authorize?{}",
        form(&[
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT),
            ("state", &state),
            ("scope", SCOPES),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge(&verifier)?),
        ])
    );
    if !unsafe { calendar::open_url(HWND::default(), &url) } {
        return Err("Browser gagal dibuka untuk login Spotify.");
    }
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(_) => return Err(NETWORK),
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(600)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(600)));
        let mut line = String::new();
        let read = BufReader::new((&mut stream).take(8192)).read_line(&mut line);
        let code = read.ok().and_then(|_| {
            let mut parts = line.split_ascii_whitespace();
            (parts.next()? == "GET").then_some(())?;
            callback(parts.next()?, &state)
        });
        let (status, message) = if code.is_some() {
            ("200 OK", "Return to Horizon to continue.")
        } else {
            (
                "404 Not Found",
                "This is not a valid Horizon login callback.",
            )
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{message}",
            message.len()
        );
        if let Some(code) = code {
            return exchange(
                client_id,
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code?),
                    ("redirect_uri", REDIRECT),
                    ("code_verifier", &verifier),
                ],
            );
        }
    }
    Err("Waktu login Spotify habis. Coba lagi.")
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

fn exchange(client_id: &str, params: &[(&str, &str)]) -> SpotifyResult<Token> {
    let mut fields = vec![("client_id", client_id)];
    fields.extend_from_slice(params);
    let (status, body) = request(
        "accounts.spotify.com",
        "POST",
        "/api/token",
        "Content-Type: application/x-www-form-urlencoded\r\n",
        form(&fields).as_bytes(),
    )?;
    if status != 200 {
        return Err(match status {
            400 | 401 if params[0].1 == "refresh_token" => LOGIN,
            400 | 401 => "Spotify menolak login. Cek Client ID dan redirect URI.",
            429 => "Spotify membatasi permintaan. Coba lagi sebentar.",
            _ => NETWORK,
        });
    }
    let data: TokenResponse = serde_json::from_slice(&body).map_err(|_| NETWORK)?;
    if !opaque_ascii(&data.access_token) {
        return Err(NETWORK);
    }
    if let Some(refresh) = data.refresh_token {
        assistant::save_key(&credential(client_id), &refresh).map_err(|_| TOKEN_STORE)?;
    } else if params[0].1 == "authorization_code" {
        return Err(TOKEN_STORE);
    }
    Ok(Token {
        access: data.access_token,
        expires: Instant::now() + Duration::from_secs(data.expires_in.min(3600).saturating_sub(30)),
    })
}

fn api(
    client_id: &str,
    token: &mut Option<Token>,
    verb: &str,
    path: &str,
    body: &[u8],
) -> SpotifyResult<Vec<u8>> {
    for _ in 0..2 {
        if token
            .as_ref()
            .is_none_or(|token| token.expires <= Instant::now())
        {
            let refresh = assistant::load_key(&credential(client_id))
                .map_err(|_| TOKEN_STORE)?
                .ok_or(LOGIN)?;
            *token = Some(exchange(
                client_id,
                &[("grant_type", "refresh_token"), ("refresh_token", &refresh)],
            )?);
        }
        let access = &token.as_ref().ok_or(LOGIN)?.access;
        let (status, data) = request(
            "api.spotify.com",
            verb,
            path,
            &format!("Authorization: Bearer {access}\r\nContent-Type: application/json\r\n"),
            body,
        )?;
        match status {
            200 | 204 => return Ok(data),
            401 => *token = None,
            403 => return Err("Spotify menolak akses. Cek Premium dan akses akun di Dashboard."),
            404 if verb == "PUT" => return Err(NO_DEVICE),
            429 => return Err("Spotify membatasi permintaan. Coba lagi sebentar."),
            _ => return Err(NETWORK),
        }
    }
    Err(LOGIN)
}

fn run_job(client_id: &str, token: &mut Option<Token>, job: Job) -> SpotifyResult<Update> {
    match job {
        Job::Connect => {
            *token = Some(authorize(client_id)?);
            Ok(Update::Connected(client_id.to_owned()))
        }
        Job::Search(query) => {
            let path = format!(
                "/v1/search?type=track&limit=10&q={}",
                lyrics::encode(&query)
            );
            let data = api(client_id, token, "GET", &path, &[])?;
            Ok(Update::Songs(decode_songs(&data)?))
        }
        Job::Play(id) => {
            let data = api(client_id, token, "GET", "/v1/me/player/devices", &[])?;
            let devices: Devices = serde_json::from_slice(&data).map_err(|_| NETWORK)?;
            let name = std::env::var("COMPUTERNAME").unwrap_or_default();
            let device = select_computer(&devices.devices, &name).ok_or(NO_DEVICE)?;
            let path = format!("/v1/me/player/play?device_id={}", lyrics::encode(device));
            let body = json!({ "uris": [format!("spotify:track:{id}")] });
            api(client_id, token, "PUT", &path, body.to_string().as_bytes())?;
            Ok(Update::Playing)
        }
    }
}

fn decode_songs(data: &[u8]) -> SpotifyResult<Vec<Song>> {
    let data: Value = serde_json::from_slice(data).map_err(|_| NETWORK)?;
    let items = data["tracks"]["items"].as_array().ok_or(NETWORK)?;
    Ok(items
        .iter()
        .take(10)
        .filter_map(|item| {
            let id = item["id"].as_str()?;
            if id.len() != 22
                || !id.bytes().all(|b| b.is_ascii_alphanumeric())
                || item["is_playable"] == false
            {
                return None;
            }
            let artist = item["artists"]
                .as_array()?
                .iter()
                .filter_map(|artist| artist["name"].as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Some(Song {
                id: id.into(),
                title: item["name"].as_str()?.into(),
                artist,
            })
        })
        .collect())
}

#[derive(Deserialize)]
struct Devices {
    devices: Vec<Device>,
}

#[derive(Deserialize)]
struct Device {
    id: Option<String>,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    is_active: bool,
    is_restricted: bool,
}

fn select_computer<'a>(devices: &'a [Device], name: &str) -> Option<&'a str> {
    let mut computers = devices.iter().filter(|device| {
        !device.is_restricted
            && device.kind.eq_ignore_ascii_case("computer")
            && device.id.as_deref().is_some_and(opaque_ascii)
    });
    let preferred = computers
        .clone()
        .find(|device| device.name.eq_ignore_ascii_case(name))
        .or_else(|| computers.clone().find(|device| device.is_active));
    if let Some(device) = preferred {
        return device.id.as_deref();
    }
    let only = computers.next()?;
    if computers.next().is_none() {
        only.id.as_deref()
    } else {
        None
    }
}

struct Handle(*mut std::ffi::c_void);
impl Handle {
    fn new(value: *mut std::ffi::c_void) -> SpotifyResult<Self> {
        if value.is_null() {
            Err(NETWORK)
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

fn request(
    host: &str,
    verb: &str,
    path: &str,
    headers: &str,
    body: &[u8],
) -> SpotifyResult<(u32, Vec<u8>)> {
    unsafe {
        let null = PCWSTR::null();
        let session = Handle::new(WinHttpOpen(
            w!("Horizon"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null,
            null,
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 3000, 3500, 5000, 5000).map_err(|_| NETWORK)?;
        let connection = Handle::new(WinHttpConnect(session.0, &HSTRING::from(host), 443, 0))?;
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            &HSTRING::from(verb),
            &HSTRING::from(path),
            null,
            null,
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes()),
        )
        .map_err(|_| NETWORK)?;
        let headers: Vec<u16> = headers.encode_utf16().collect();
        let size = body.len() as u32;
        WinHttpSendRequest(
            request.0,
            Some(&headers),
            (!body.is_empty()).then_some(body.as_ptr().cast()),
            size,
            size,
            0,
        )
        .map_err(|_| NETWORK)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|_| NETWORK)?;
        let mut status = 0u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null,
            Some((&mut status as *mut u32).cast()),
            &mut 4u32,
            &mut 0,
        )
        .map_err(|_| NETWORK)?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let mut read = 0;
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
            .map_err(|_| NETWORK)?;
            if read == 0 {
                return Ok((status, data));
            }
            if data.len() + read as usize > 256 * 1024 {
                return Err(NETWORK);
            }
            data.extend_from_slice(&buffer[..read as usize]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_search_preserves_unicode_and_escapes_url_delimiters() {
        assert_eq!(search_url("   ", true), None);
        assert_eq!(search_url("   ", false), None);
        assert_eq!(
            search_url("  Céline / A&B?#  ", true).as_deref(),
            Some("https://open.spotify.com/search/C%C3%A9line%20%2F%20A%26B%3F%23")
        );
        assert_eq!(
            search_url("  Céline / A&B?#  ", false).as_deref(),
            Some("spotify:search:C%C3%A9line%20%2F%20A%26B%3F%23")
        );
    }

    #[test]
    fn pkce_challenge_matches_the_standard_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk").unwrap(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn callback_requires_the_expected_path_and_single_matching_state() {
        for invalid in [
            "/?code=abc&state=xyz",
            "/spotify/callback?code=abc",
            "/spotify/callback?code=abc&state=wrong",
            "/spotify/callback?code=abc&state=xyz&state=wrong",
            "/spotify/callback?code=abc&state=xyz&code=other",
        ] {
            assert!(callback(invalid, "xyz").is_none());
        }
        assert_eq!(
            callback("/spotify/callback?code=ab%2Bc%2F&state=xyz", "xyz"),
            Some(Ok("ab+c/".into()))
        );
    }

    #[test]
    fn playback_prefers_the_local_unrestricted_computer() {
        let list = |name, id, kind, is_active, is_restricted| Device {
            name,
            id,
            kind,
            is_active,
            is_restricted,
        };
        let devices = [
            list(
                "Phone".into(),
                Some("phone".into()),
                "Smartphone".into(),
                true,
                false,
            ),
            list(
                "PC".into(),
                Some("restricted".into()),
                "Computer".into(),
                false,
                true,
            ),
            list(
                "PC".into(),
                Some("local".into()),
                "Computer".into(),
                false,
                false,
            ),
            list(
                "Laptop".into(),
                Some("other".into()),
                "Computer".into(),
                false,
                false,
            ),
        ];
        assert_eq!(select_computer(&devices, "pc"), Some("local"));
        assert_eq!(select_computer(&devices, "unknown"), None);
        assert_eq!(select_computer(&devices[..2], "pc"), None);
    }
}
