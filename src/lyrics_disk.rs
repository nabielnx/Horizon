use crate::lyrics::Line;
use serde::{
    Deserialize, Serialize,
    de::{SeqAccess, Visitor},
};
use std::{
    io::{self, Read},
    path::Path,
};

const LIMIT: usize = 8 * 1024 * 1024;
const RECORD_LIMIT: usize = 256;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Record {
    #[serde(default)]
    pub format_version: u8,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    pub duration: u64,
    pub expires: u64,
    pub lines: Vec<Line>,
}

pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn read(path: &Path) -> Vec<Record> {
    let load = || -> Option<Vec<Record>> {
        if std::fs::metadata(path).ok()?.len() > LIMIT as u64 {
            return None;
        }
        let file = std::fs::File::open(path).ok()?;
        let mut rows: Vec<Record> =
            serde_json::from_reader(io::BufReader::new(file.take(LIMIT as u64 + 1))).ok()?;
        let now = now();
        rows.retain(|row| valid(row, now));
        if rows.len() > RECORD_LIMIT {
            rows.drain(..rows.len() - RECORD_LIMIT);
        }
        Some(rows)
    };
    load().unwrap_or_default()
}

fn valid(r: &Record, now: u64) -> bool {
    r.expires > now
        && r.duration > 0
        && !r.lines.is_empty()
        && r.lines.len() <= 2048
        && r.lines.iter().all(|l| {
            l.at.is_finite()
                && l.at >= 0.
                && l.text.len() <= 4096
                && l.end.is_none_or(|end| end.is_finite() && end >= l.at)
                && l.words.len() <= 1024
                && l.words.iter().all(|w| {
                    w.at.is_finite()
                        && w.at >= l.at
                        && w.text.len() <= 4096
                        && w.end.is_none_or(|end| end.is_finite() && end >= w.at)
                })
        })
}

pub(super) fn matching(
    path: &Path,
    title: &str,
    artist: &str,
    duration: u64,
    album: &str,
    minimum_format: u8,
) -> Option<Record> {
    if duration == 0 || std::fs::metadata(path).ok()?.len() > LIMIT as u64 {
        return None;
    }
    struct Match<'a> {
        title: &'a str,
        artist: &'a str,
        duration: u64,
        album: &'a str,
        minimum_format: u8,
    }
    impl<'de> Visitor<'de> for Match<'_> {
        type Value = Option<Record>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a list of cached lyric records")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut rows: A) -> Result<Self::Value, A::Error> {
            let now = now();
            let mut best: Option<Record> = None;
            while let Some(row) = rows.next_element::<Record>()? {
                let distance = row.duration.abs_diff(self.duration);
                if distance <= 1
                    && row.format_version >= self.minimum_format
                    && row.title.trim().to_lowercase() == self.title
                    && row.artist.trim().to_lowercase() == self.artist
                    && row.album.trim().to_lowercase() == self.album
                    && valid(&row, now)
                    && best
                        .as_ref()
                        .is_none_or(|old| distance <= old.duration.abs_diff(self.duration))
                {
                    best = Some(row);
                }
            }
            Ok(best)
        }
    }
    use serde::Deserializer;
    let file = std::fs::File::open(path).ok()?;
    let mut json =
        serde_json::Deserializer::from_reader(io::BufReader::new(file.take(LIMIT as u64 + 1)));
    let matched = json
        .deserialize_seq(Match {
            title,
            artist,
            duration,
            album,
            minimum_format,
        })
        .ok()?;
    json.end().ok()?;
    matched
}

pub(super) fn write(path: &Path, records: &[Record]) -> io::Result<()> {
    let mut rows: Vec<_> = records
        .iter()
        .filter(|r| !r.lines.is_empty() && r.duration > 0 && r.expires > now())
        .collect();
    if rows.len() > RECORD_LIMIT {
        rows.drain(..rows.len() - RECORD_LIMIT);
    }
    let bytes = loop {
        let bytes = serde_json::to_vec(&rows).map_err(io::Error::other)?;
        if bytes.len() <= LIMIT {
            break bytes;
        }
        rows.remove(0);
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes)?;
    #[cfg(windows)]
    unsafe {
        use std::os::windows::ffi::OsStrExt;
        use windows::{Win32::Storage::FileSystem::*, core::PCWSTR};
        let from: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(io::Error::other)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_lyrics_survive_reload_but_missing_results_do_not() {
        let path =
            std::env::temp_dir().join(format!("horizon-lyrics-disk-{}.json", std::process::id()));
        let row = Record {
            format_version: 2,
            title: "Fixture".into(),
            artist: "Artist".into(),
            album: String::new(),
            duration: 200,
            expires: now() + 3600,
            lines: vec![Line {
                at: 0.,
                text: "Fixture line".into(),
                ..Default::default()
            }],
        };
        let mut missing = row.clone();
        missing.title = "Missing".into();
        missing.lines.clear();
        write(&path, &[row.clone(), missing]).unwrap();
        let loaded = read(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].lines, row.lines);
        write(&path, &loaded).unwrap();
        assert_eq!(read(&path).len(), 1);
        std::fs::write(&path, b"broken").unwrap();
        assert!(read(&path).is_empty());
        std::fs::remove_file(path).unwrap();
    }
}
