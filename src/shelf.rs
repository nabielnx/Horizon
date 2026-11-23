use std::{collections::VecDeque, io::Read, path::Path, sync::mpsc};

use crate::artwork::PreviewImage;
use windows::{
    Win32::{
        Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
        UI::{
            Controls::{IImageList, ILD_TRANSPARENT},
            Shell::*,
            WindowsAndMessaging::DestroyIcon,
        },
    },
    core::PCWSTR,
};

#[derive(Clone)]
pub enum Preview {
    Image(PreviewImage),
    Text(String),
    Unavailable,
}
#[derive(Default)]
struct Request {
    preview: Option<String>,
    icons: Vec<String>,
}
enum Loaded {
    Preview(String, Preview),
    Icon(String, Option<PreviewImage>),
}
pub struct Worker {
    send: mpsc::Sender<Request>,
    receive: mpsc::Receiver<Loaded>,
    requested: Request,
    previews: VecDeque<(String, Preview)>,
    icons: VecDeque<(String, Option<PreviewImage>)>,
}
impl Worker {
    pub fn new(notify: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Self {
        let (send, jobs) = mpsc::channel::<Request>();
        let (results, receive) = mpsc::channel();
        std::thread::spawn(move || {
            use windows::Win32::System::Com::*;
            let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            'work: while let Ok(mut request) = jobs.recv() {
                while let Ok(latest) = jobs.try_recv() {
                    request = latest;
                }
                if let Some(path) = request.preview {
                    let preview = load(&path);
                    if results.send(Loaded::Preview(path, preview)).is_err() {
                        break;
                    }
                    crate::media::notify(&notify);
                }
                for path in request.icons {
                    let image = if com { load_icon(&path) } else { None };
                    if results.send(Loaded::Icon(path, image)).is_err() {
                        break 'work;
                    }
                    crate::media::notify(&notify);
                }
            }
            if com {
                unsafe { CoUninitialize() };
            }
        });
        Self {
            send,
            receive,
            requested: Request::default(),
            previews: VecDeque::new(),
            icons: VecDeque::new(),
        }
    }
    pub fn request<'a>(
        &mut self,
        preview: Option<&str>,
        icons: impl Iterator<Item = &'a str> + Clone,
    ) {
        let changed = self.requested.preview.as_deref() != preview
            || !self
                .requested
                .icons
                .iter()
                .map(String::as_str)
                .eq(icons.clone());
        if changed {
            self.requested = Request {
                preview: preview.map(str::to_owned),
                icons: icons.map(str::to_owned).collect(),
            };
        }
        while let Ok(result) = self.receive.try_recv() {
            match result {
                Loaded::Preview(path, content) => {
                    self.previews.retain(|(key, _)| key != &path);
                    self.previews.push_back((path, content));
                    if self.previews.len() > 3
                        && let Some(old) = self
                            .previews
                            .iter()
                            .position(|(key, _)| Some(key) != self.requested.preview.as_ref())
                    {
                        self.previews.remove(old);
                    }
                }
                Loaded::Icon(path, image) => {
                    self.icons.retain(|(key, _)| key != &path);
                    self.icons.push_back((path, image));
                    if self.icons.len() > 12
                        && let Some(old) = self
                            .icons
                            .iter()
                            .position(|(key, _)| !self.requested.icons.contains(key))
                    {
                        self.icons.remove(old);
                    }
                }
            }
        }
        if changed {
            let _ = self.send.send(Request {
                preview: self
                    .requested
                    .preview
                    .as_ref()
                    .filter(|path| self.preview(path).is_none())
                    .cloned(),
                icons: self
                    .requested
                    .icons
                    .iter()
                    .filter(|path| !self.icons.iter().any(|(key, _)| key == *path))
                    .cloned()
                    .collect(),
            });
        }
    }
    pub fn preview(&self, path: &str) -> Option<&Preview> {
        self.previews
            .iter()
            .find(|(key, _)| key == path)
            .map(|(_, value)| value)
    }
    pub fn icon(&self, path: &str) -> Option<&PreviewImage> {
        self.icons
            .iter()
            .find(|(key, _)| key == path)
            .and_then(|(_, image)| image.as_ref())
    }
}

fn load_icon(path: &str) -> Option<PreviewImage> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return None;
    }
    use std::os::windows::ffi::OsStrExt;
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let mut info = SHFILEINFOW::default();
        if SHGetFileInfoW(
            PCWSTR(name.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_SYSICONINDEX,
        ) == 0
        {
            return None;
        }
        let large = SHGetImageList::<IImageList>(SHIL_EXTRALARGE as i32)
            .and_then(|icons| icons.GetIcon(info.iIcon, ILD_TRANSPARENT.0))
            .ok()
            .filter(|icon| !icon.is_invalid());
        let icon = large.unwrap_or(info.hIcon);
        let image = crate::artwork::icon_preview(icon).ok();
        let _ = DestroyIcon(info.hIcon);
        if let Some(icon) = large {
            let _ = DestroyIcon(icon);
        }
        image.filter(|image| image.pixels.chunks_exact(4).any(|pixel| pixel[3] > 0))
    }
}
enum PreviewKind {
    Image,
    Text,
}

fn preview_kind(path: &str) -> Option<PreviewKind> {
    let extension = Path::new(path)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "ico" => Some(PreviewKind::Image),
        "txt" | "md" | "rs" | "json" | "log" | "py" | "js" | "ts" | "toml" | "yaml" | "yml"
        | "html" | "css" | "c" | "cpp" | "sql" => Some(PreviewKind::Text),
        _ => None,
    }
}

pub fn supports_preview(path: &str) -> bool {
    preview_kind(path).is_some()
}

fn load(path: &str) -> Preview {
    let Some(kind) = preview_kind(path) else {
        return Preview::Unavailable;
    };
    let image = matches!(kind, PreviewKind::Image);
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
    fn shortcut_icons_use_the_windows_association_for_each_file() {
        use windows::{
            Win32::System::Com::*,
            core::{HSTRING, Interface},
        };

        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
            let system =
                std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
            let wide = |path: &Path| HSTRING::from(path.to_string_lossy().as_ref());
            let mut images = Vec::new();
            for icon_index in [3, 16] {
                let path = std::env::temp_dir().join(format!(
                    "horizon-icon-{}-{icon_index}.lnk",
                    std::process::id()
                ));
                let link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
                link.SetPath(&wide(&system.join("cmd.exe"))).unwrap();
                link.SetIconLocation(&wide(&system.join("shell32.dll")), icon_index)
                    .unwrap();
                link.cast::<IPersistFile>()
                    .unwrap()
                    .Save(&wide(&path), false)
                    .unwrap();
                let image = load_icon(&path.to_string_lossy());
                std::fs::remove_file(path).unwrap();
                images.push(image.expect("Windows shortcut icon"));
            }
            CoUninitialize();
            assert!(images[0].pixels != images[1].pixels);
        }
    }
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
