//! Optional Apple catalogue fallback. Matching never strips remix/live labels.
use crate::media::{Command, Track};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Clone)]
pub struct Artwork {
    pub bytes: Arc<Vec<u8>>,
    pub palette: [u32; 3],
    pub link: String,
}
pub struct Job {
    pub key: String,
    pub generation: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
}
pub struct Completed {
    pub key: String,
    pub generation: u64,
    pub image: Result<Artwork, String>,
}
fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn artist_matches(expected: &str, found: &str) -> bool {
    let primary = |name: &str| {
        let lower = name.to_lowercase();
        [", ", "; ", " feat. ", " featuring ", " ft. "]
            .iter()
            .filter_map(|separator| lower.find(separator))
            .min()
            .map_or_else(|| normalize(name), |split| normalize(&lower[..split]))
    };
    let expected_name = normalize(expected);
    let found_name = normalize(found);
    !expected_name.is_empty()
        && (expected_name == found_name
            || expected_name == primary(found)
            || found_name == primary(expected))
}
fn album(text: &str) -> String {
    normalize(text.trim_end_matches(" - Single").trim_end_matches(" - EP"))
}
fn artwork_address(url: &str) -> Option<(&str, &str)> {
    let (host, tail) = url.strip_prefix("https://")?.split_once('/')?;
    (host.ends_with(".mzstatic.com")
        && !host.contains([':', '@', '\\'])
        && !tail.contains(['\r', '\n']))
    .then_some((host, &url[8 + host.len()..]))
}
fn choose<'a>(job: &Job, rows: &'a [serde_json::Value]) -> Option<&'a serde_json::Value> {
    let matches = rows
        .iter()
        .filter(|r| {
            normalize(r["trackName"].as_str().unwrap_or("")) == normalize(&job.title)
                && artist_matches(&job.artist, r["artistName"].as_str().unwrap_or(""))
                && (job.duration <= 0.
                    || r["trackTimeMillis"]
                        .as_f64()
                        .is_some_and(|ms| (ms / 1000. - job.duration).abs() <= 3.))
                && artwork_address(r["artworkUrl100"].as_str().unwrap_or("")).is_some()
                && r["trackViewUrl"].as_str().is_some_and(|url| {
                    url.starts_with("https://music.apple.com/")
                        || url.starts_with("https://itunes.apple.com/")
                })
        })
        .collect::<Vec<_>>();
    if !job.album.is_empty()
        && let Some(row) = matches
            .iter()
            .copied()
            .find(|r| album(r["collectionName"].as_str().unwrap_or("")) == album(&job.album))
    {
        return Some(row);
    }
    let matches = matches
        .into_iter()
        .filter(|r| {
            job.duration > 0.
                && r["trackTimeMillis"]
                    .as_f64()
                    .is_some_and(|ms| (ms / 1000. - job.duration).abs() <= 1.5)
        })
        .collect::<Vec<_>>();
    let first = *matches.first()?;
    if matches
        .iter()
        .all(|r| r["artworkUrl100"] == first["artworkUrl100"])
    {
        return Some(first);
    }
    // Different releases can carry different artwork for the same track metadata.
    // With no album match, use a dated catalogue release only when every candidate
    // has essentially the same duration. This is alternate-release artwork, not
    // proof that the player's precise album edition was identified.
    let releases = matches
        .iter()
        .copied()
        .map(|row| {
            let id = row["trackId"].as_u64().filter(|id| *id > 0)?;
            let date = chrono::DateTime::parse_from_rfc3339(row["releaseDate"].as_str()?).ok()?;
            let duration = row["trackTimeMillis"].as_f64()?;
            Some((row, date, id, duration))
        })
        .collect::<Option<Vec<_>>>()?;
    let min_duration = releases.iter().map(|r| r.3).fold(f64::INFINITY, f64::min);
    let max_duration = releases
        .iter()
        .map(|r| r.3)
        .fold(f64::NEG_INFINITY, f64::max);
    if max_duration - min_duration > 250. {
        return None;
    }
    releases.into_iter().min_by_key(|r| (r.1, r.2)).map(|r| r.0)
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetUserDefaultGeoName(name: *mut u16, count: i32) -> i32;
}

fn storefront() -> [u8; 2] {
    let mut name = [0u16; 8];
    let length = unsafe { GetUserDefaultGeoName(name.as_mut_ptr(), name.len() as i32) };
    if length == 3
        && name[..2]
            .iter()
            .all(|&unit| (u16::from(b'A')..=u16::from(b'Z')).contains(&unit))
    {
        [name[0] as u8, name[1] as u8]
    } else {
        *b"US"
    }
}

fn find_cover(
    job: &Job,
    country: &str,
    alive: &impl Fn() -> bool,
    get: &impl Fn(&str, &str, usize) -> Result<Vec<u8>, String>,
) -> Result<Artwork, String> {
    let query = crate::lyrics::encode(&format!("{} {}", job.title, job.artist));
    let mut countries = [country, "US"].into_iter();
    if country == "US" {
        countries.next();
    }
    let mut failure = String::new();
    for country in countries {
        if !alive() {
            return Err("Superseded cover request".into());
        }
        match lookup_country(job, country, &query, get) {
            Ok(artwork) => return Ok(artwork),
            Err(error) => {
                if !failure.is_empty() {
                    failure.push_str("; ");
                }
                failure.push_str(&format!("{country}: {error}"));
            }
        }
    }
    Err(failure)
}

fn lookup_country(
    job: &Job,
    country: &str,
    query: &str,
    get: &impl Fn(&str, &str, usize) -> Result<Vec<u8>, String>,
) -> Result<Artwork, String> {
    let bytes = get(
        "itunes.apple.com",
        &format!("/search?term={query}&country={country}&media=music&entity=song&limit=25"),
        256 * 1024,
    )?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let rows = json["results"]
        .as_array()
        .ok_or("Missing catalogue results")?;
    let row = choose(job, rows).ok_or_else(|| {
        let candidates = rows
            .iter()
            .take(4)
            .map(|r| {
                format!(
                    "{} / {} / {} / {}s",
                    r["trackName"].as_str().unwrap_or(""),
                    r["artistName"].as_str().unwrap_or(""),
                    r["collectionName"].as_str().unwrap_or(""),
                    r["trackTimeMillis"].as_f64().unwrap_or(0.) / 1000.
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "No cover match for album {:?}, {:.3}s: {candidates}",
            job.album, job.duration
        )
    })?;
    let url = row["artworkUrl100"].as_str().ok_or("Missing artwork URL")?;
    let (host, path) = artwork_address(url).ok_or("Invalid artwork host")?;
    let bytes = get(host, path, 512 * 1024)?;
    let palette = crate::artwork::palette(&bytes).ok_or("Invalid catalogue image")?;
    Ok(Artwork {
        bytes: Arc::new(bytes),
        palette,
        link: row["trackViewUrl"].as_str().unwrap().into(),
    })
}

pub fn lookup(job: &Job, alive: &impl Fn() -> bool) -> Result<Artwork, String> {
    let country = storefront();
    let country = std::str::from_utf8(&country).unwrap_or("US");
    find_cover(job, country, alive, &|host, path, limit| {
        crate::lyrics::artwork_get(host, path, limit, alive).map_err(|error| error.to_string())
    })
}
pub fn worker(
    state: Arc<Mutex<Track>>,
    enabled: Arc<AtomicBool>,
    wake: mpsc::Sender<Command>,
) -> (mpsc::SyncSender<Job>, mpsc::Receiver<Completed>) {
    let (send, jobs) = mpsc::sync_channel::<Job>(1);
    let (results, done) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = unsafe {
            windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            )
        };
        let mut cache = std::collections::VecDeque::<(String, Artwork)>::new();
        let mut next_network = std::time::Instant::now();
        while let Ok(job) = jobs.recv() {
            let alive = || {
                enabled.load(Ordering::Relaxed)
                    && state.lock().is_ok_and(|t| {
                        t.generation == job.generation
                            && crate::media::track_cover_key(&t) == job.key
                            && t.cover.is_none()
                    })
            };
            let image = if !alive() {
                Err("Superseded cover request".into())
            } else if let Some((_, art)) = cache.iter().find(|(key, _)| key == &job.key) {
                Ok(art.clone())
            } else if std::time::Instant::now() < next_network {
                Err("Catalogue cooldown".into())
            } else {
                next_network = std::time::Instant::now() + std::time::Duration::from_secs(4);
                let result = lookup(&job, &alive);
                if let Ok(art) = &result {
                    while !cache.is_empty()
                        && (cache.len() >= 4
                            || cache.iter().map(|(_, a)| a.bytes.len()).sum::<usize>()
                                + art.bytes.len()
                                > 1024 * 1024)
                    {
                        cache.pop_front();
                    }
                    cache.push_back((job.key.clone(), art.clone()));
                }
                result
            };
            if results
                .send(Completed {
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
    (send, done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_in_the_local_catalogue_before_the_us_store() {
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let job = Job {
            key: "fixture".into(),
            generation: 0,
            title: "Example Song".into(),
            artist: "Primary Artist".into(),
            album: "Example Song".into(),
            duration: 235.785,
        };
        let paths = std::cell::RefCell::new(Vec::new());
        let cover = find_cover(&job, "ID", &|| true, &|host, path, _| {
            paths.borrow_mut().push((host.to_owned(), path.to_owned()));
            if host == "itunes.apple.com" {
                let row = serde_json::json!({
                    "trackName": "Example Song",
                    "artistName": "Primary Artist, Guest Artist & Another Artist",
                    "collectionName": "Example Song - Single",
                    "trackTimeMillis": 235785,
                    "artworkUrl100": "https://is1-ssl.mzstatic.com/image/test.jpg",
                    "trackViewUrl": "https://music.apple.com/id/album/example-song/42",
                });
                return Ok(serde_json::json!({"results": [row]})
                    .to_string()
                    .into_bytes());
            }
            Ok(include_bytes!("../assets/fixtures/cover.png").to_vec())
        })
        .unwrap();
        assert!(cover.link.contains("/id/"));
        let paths = paths.into_inner();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].1.contains("&country=ID&"));
        assert_eq!(paths[1].0, "is1-ssl.mzstatic.com");

        let countries = std::cell::RefCell::new(Vec::new());
        let _ = find_cover(&job, "ID", &|| true, &|_, path, _| {
            countries.borrow_mut().push(path.to_owned());
            Ok(br#"{"results":[]}"#.to_vec())
        });
        let countries = countries.into_inner();
        assert_eq!(countries.len(), 2);
        assert!(countries[0].contains("&country=ID&"));
        assert!(countries[1].contains("&country=US&"));
    }

    #[test]
    fn alternate_release_artwork_is_stable_without_a_matching_album() {
        let mut job = Job {
            key: "fixture".into(),
            generation: 0,
            title: "Example Song".into(),
            artist: "Example Artist".into(),
            album: String::new(),
            duration: 205.4,
        };
        let original = serde_json::json!({
            "trackId": 100, "trackName": "Example Song", "artistName": "Example Artist",
            "collectionName": "First Album", "trackTimeMillis": 205305,
            "releaseDate": "2025-06-01T07:00:00Z",
            "artworkUrl100": "https://is1-ssl.mzstatic.com/image/original.jpg",
            "trackViewUrl": "https://music.apple.com/track/100"
        });
        let mut reissue = original.clone();
        reissue["trackId"] = serde_json::json!(200);
        reissue["collectionName"] = serde_json::json!("Expanded Album");
        reissue["releaseDate"] = serde_json::json!("2026-06-01T07:00:00Z");
        reissue["artworkUrl100"] =
            serde_json::json!("https://is1-ssl.mzstatic.com/image/reissue.jpg");
        let forward = [original.clone(), reissue.clone()];
        let reverse = [reissue.clone(), original.clone()];
        assert_eq!(choose(&job, &forward), Some(&original));
        assert_eq!(choose(&job, &reverse), Some(&original));
        job.album = "Expanded Album".into();
        assert_eq!(choose(&job, &forward), Some(&reissue));
        job.album = "Album unavailable in catalogue".into();
        assert_eq!(choose(&job, &forward), Some(&original));
        reissue["trackTimeMillis"] = serde_json::json!(206100);
        assert!(choose(&job, &[original.clone(), reissue.clone()]).is_none());
        reissue["trackTimeMillis"] = serde_json::json!(205305);
        reissue["releaseDate"] = serde_json::json!("invalid date");
        assert!(choose(&job, &[original.clone(), reissue]).is_none());
        job.duration = 0.;
        assert!(choose(&job, &forward).is_none());
    }
    #[test]
    fn matching_rejects_wrong_versions_durations_hosts_and_ambiguous_releases() {
        let mut job = Job {
            key: "fixture".into(),
            generation: 0,
            title: "Fixture Live".into(),
            artist: "Artist".into(),
            album: "Record".into(),
            duration: 200.,
        };
        let row = serde_json::json!({"trackName":"Fixture Live", "artistName":"Artist", "collectionName":"Record - Single", "trackTimeMillis":200100, "artworkUrl100":"https://is1-ssl.mzstatic.com/image/test.jpg", "trackViewUrl":"https://music.apple.com/track/fixture"});
        assert!(choose(&job, std::slice::from_ref(&row)).is_some());
        for (field, value) in [
            ("trackName", serde_json::json!("Fixture Remix")),
            ("artistName", serde_json::json!("Other")),
            ("trackTimeMillis", serde_json::json!(220000)),
            (
                "artworkUrl100",
                serde_json::json!("https://mzstatic.com.attacker.test/a.jpg"),
            ),
        ] {
            let mut wrong = row.clone();
            wrong[field] = value;
            assert!(choose(&job, &[wrong]).is_none());
        }
        let mut single = row.clone();
        single["collectionName"] = serde_json::json!("Single Release");
        single["artworkUrl100"] =
            serde_json::json!("https://is1-ssl.mzstatic.com/image/single.jpg");
        assert!(choose(&job, std::slice::from_ref(&single)).is_some());
        assert_eq!(choose(&job, &[single.clone(), row.clone()]).unwrap(), &row);
        job.album.clear();
        let mut competing = row.clone();
        competing["artworkUrl100"] =
            serde_json::json!("https://is1-ssl.mzstatic.com/image/other.jpg");
        assert!(choose(&job, &[row, competing]).is_none());
    }
}
