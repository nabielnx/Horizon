use std::{collections::VecDeque, io::Read, sync::mpsc};

#[derive(Clone)]
pub enum Preview {
    Image(crate::artwork::PreviewImage),
    Text(String),
    Unavailable,
}
pub struct Worker {
    send: mpsc::Sender<String>,
    receive: mpsc::Receiver<(String, Preview)>,
    requested: String,
    cache: VecDeque<(String, Preview)>,
}
impl Worker {
    pub fn new(notify: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Self {
        let (send, jobs) = mpsc::channel::<String>();
        let (results, receive) = mpsc::channel();
        std::thread::spawn(move || {
            use windows::Win32::System::Com::*;
            let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            while let Ok(mut path) = jobs.recv() {
                while let Ok(latest) = jobs.try_recv() {
                    path = latest;
                }
                let preview = load(&path);
                if results.send((path, preview)).is_err() {
                    break;
                }
                crate::media::notify(&notify);
            }
            if com {
                unsafe { CoUninitialize() };
            }
        });
        Self {
            send,
            receive,
            requested: String::new(),
            cache: VecDeque::new(),
        }
    }
    pub fn get(&mut self, path: &str) -> Option<&Preview> {
        while let Ok(result) = self.receive.try_recv() {
            self.cache.retain(|(key, _)| key != &result.0);
            self.cache.push_back(result);
            while self.cache.len() > 3 {
                self.cache.pop_front();
            }
            while self.cache.len() > 1
                && self
                    .cache
                    .iter()
                    .map(|(_, preview)| match preview {
                        Preview::Image(image) => image.pixels.len(),
                        Preview::Text(text) => text.len(),
                        Preview::Unavailable => 0,
                    })
                    .sum::<usize>()
                    > 12 * 1024 * 1024
            {
                self.cache.pop_front();
            }
        }
        if !self.cache.iter().any(|(key, _)| key == path) && self.requested != path {
            self.requested = path.into();
            let _ = self.send.send(path.into());
        }
        self.cache
            .iter()
            .find(|(key, _)| key == path)
            .map(|(_, value)| value)
    }
}
fn load(path: &str) -> Preview {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let image = matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "ico"
    );
    let text = matches!(
        extension.as_str(),
        "txt"
            | "md"
            | "rs"
            | "json"
            | "log"
            | "py"
            | "js"
            | "ts"
            | "toml"
            | "yaml"
            | "yml"
            | "html"
            | "css"
            | "c"
            | "cpp"
            | "sql"
    );
    if !image && !text {
        return Preview::Unavailable;
    }
    let Ok(file) = std::fs::File::open(path) else {
        return Preview::Unavailable;
    };
    let limit = if image { 12 * 1024 * 1024 } else { 1024 };
    if image
        && file
            .metadata()
            .map_or(true, |metadata| metadata.len() > limit)
    {
        return Preview::Unavailable;
    }
    let mut bytes = Vec::new();
    if file.take(limit + 1).read_to_end(&mut bytes).is_err() || image && bytes.len() as u64 > limit
    {
        return Preview::Unavailable;
    }
    if image {
        unsafe { crate::artwork::preview(&bytes) }
            .map(Preview::Image)
            .unwrap_or(Preview::Unavailable)
    } else {
        Preview::Text(
            String::from_utf8_lossy(&bytes[..bytes.len().min(1024)])
                .lines()
                .take(4)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

pub fn local_path(path: &str) -> bool {
    let path = std::path::Path::new(path);
    path.is_absolute() && path.exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_text_preview_is_bounded_and_local_paths_are_validated() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!("horizon-shelf-{}.txt", std::process::id()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&vec![b'x'; 2048]).unwrap();
        file.set_len(16 * 1024 * 1024).unwrap();
        drop(file);
        let name = path.to_string_lossy();
        let Preview::Text(text) = load(&name) else {
            panic!("text preview")
        };
        assert_eq!(text.len(), 1024);
        assert!(local_path(&name));
        assert!(!local_path("https://example.com/file.txt"));
        std::fs::remove_file(path).unwrap();
    }
}
