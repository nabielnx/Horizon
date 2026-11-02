use crate::lyrics::Line;
use serde::{Deserialize, Serialize};
use std::{io, path::Path};

const LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Record {
    pub title: String,
    pub artist: String,
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
        let bytes = std::fs::read(path).ok()?;
        let mut rows: Vec<Record> = serde_json::from_slice(&bytes).ok()?;
        let now = now();
        rows.retain(|r| {
            r.expires > now
                && r.duration > 0
                && !r.lines.is_empty()
                && r.lines.len() <= 2048
                && r.lines
                    .iter()
                    .all(|l| l.at.is_finite() && l.at >= 0. && l.text.len() <= 4096)
        });
        if rows.len() > 32 {
            rows.drain(..rows.len() - 32);
        }
        Some(rows)
    };
    load().unwrap_or_default()
}

pub(super) fn write(path: &Path, records: &[Record]) -> io::Result<()> {
    let mut rows: Vec<_> = records
        .iter()
        .filter(|r| !r.lines.is_empty() && r.duration > 0 && r.expires > now())
        .cloned()
        .collect();
    if rows.len() > 32 {
        rows.drain(..rows.len() - 32);
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
            title: "Fixture".into(),
            artist: "Artist".into(),
            duration: 200,
            expires: now() + 3600,
            lines: vec![Line {
                at: 0.,
                text: "Fixture line".into(),
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
