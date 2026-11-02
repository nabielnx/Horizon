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
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
}
pub struct Completed {
    pub key: String,
    pub image: Result<Artwork, String>,
}
fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
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
                && normalize(r["artistName"].as_str().unwrap_or("")) == normalize(&job.artist)
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
    // An alternate single/release must identify one recording and one cover.
    matches
        .iter()
        .all(|r| r["artworkUrl100"] == first["artworkUrl100"])
        .then_some(first)
}
pub fn lookup(job: &Job, alive: &impl Fn() -> bool) -> Result<Artwork, String> {
    let query = format!("{} {}", job.title, job.artist)
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect::<String>();
    let bytes = crate::lyrics::artwork_get(
        "itunes.apple.com",
        &format!("/search?term={query}&media=music&entity=song&limit=25"),
        256 * 1024,
        alive,
    )
    .map_err(|e| e.to_string())?;
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
        format!("No exact cover match: {candidates}")
    })?;
    let url = row["artworkUrl100"].as_str().ok_or("Missing artwork URL")?;
    let (host, path) = artwork_address(url).ok_or("Invalid artwork host")?;
    let bytes =
        crate::lyrics::artwork_get(host, path, 512 * 1024, alive).map_err(|e| e.to_string())?;
    let palette = crate::artwork::palette(&bytes).ok_or("Invalid catalogue image")?;
    Ok(Artwork {
        bytes: Arc::new(bytes),
        palette,
        link: row["trackViewUrl"].as_str().unwrap().into(),
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
                        crate::media::track_cover_key(&t) == job.key && t.cover.is_none()
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
    fn matching_rejects_wrong_versions_durations_hosts_and_ambiguous_releases() {
        let mut job = Job {
            key: "fixture".into(),
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
