use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use windows::{
    Foundation::TimeSpan,
    Media::SpeechRecognition::*,
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::WinRT::*,
        UI::WindowsAndMessaging::{PostMessageW, WM_APP},
    },
    core::RuntimeType,
};

pub const READY: u32 = WM_APP + 18;
pub type Dictation = mpsc::Receiver<Result<String, &'static str>>;
const UNAVAILABLE: &str =
    "Dikte gagal. Cek akses mikrofon dan Pengenalan ucapan online di Windows.";

pub fn start(hwnd: HWND) -> std::io::Result<Dictation> {
    let (send, receive) = mpsc::channel();
    let window = hwnd.0 as usize;
    std::thread::Builder::new()
        .name("horizon-dictation".into())
        .spawn(move || {
            let result = recognize();
            if send.send(result).is_ok() {
                unsafe {
                    let _ = PostMessageW(Some(HWND(window as _)), READY, WPARAM(0), LPARAM(0));
                }
            }
        })?;
    Ok(receive)
}

fn recognize() -> Result<String, &'static str> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|_| UNAVAILABLE)?;
    let result = (|| {
        let recognizer = SpeechRecognizer::new().map_err(|_| UNAVAILABLE)?;
        // The default grammar is free-text dictation in the Windows speech language.
        let timeouts = recognizer.Timeouts().map_err(|_| UNAVAILABLE)?;
        timeouts
            .SetInitialSilenceTimeout(TimeSpan {
                Duration: 50_000_000,
            })
            .map_err(|_| UNAVAILABLE)?;
        timeouts
            .SetEndSilenceTimeout(TimeSpan {
                Duration: 15_000_000,
            })
            .map_err(|_| UNAVAILABLE)?;
        let grammar = wait(
            recognizer
                .CompileConstraintsAsync()
                .map_err(|_| UNAVAILABLE)?,
            Duration::from_secs(8),
        )?;
        if grammar.Status().map_err(|_| UNAVAILABLE)? != SpeechRecognitionResultStatus::Success {
            return Err(UNAVAILABLE);
        }
        let speech = wait(
            recognizer.RecognizeAsync().map_err(|_| UNAVAILABLE)?,
            Duration::from_secs(18),
        )?;
        match speech.Status().map_err(|_| UNAVAILABLE)? {
            SpeechRecognitionResultStatus::Success => {}
            SpeechRecognitionResultStatus::MicrophoneUnavailable => {
                return Err("Mikrofon tidak tersedia. Cek pengaturan mikrofon Windows.");
            }
            SpeechRecognitionResultStatus::TopicLanguageNotSupported => {
                return Err("Bahasa ucapan Windows belum mendukung dikte ini.");
            }
            SpeechRecognitionResultStatus::TimeoutExceeded
            | SpeechRecognitionResultStatus::AudioQualityFailure => {
                return Err("Ucapan belum tertangkap. Coba lagi.");
            }
            _ => return Err(UNAVAILABLE),
        }
        let text = speech.Text().map_err(|_| UNAVAILABLE)?.to_string_lossy();
        if text.trim().is_empty() {
            return Err("Ucapan belum tertangkap. Coba lagi.");
        }
        Ok(text)
    })();
    unsafe { RoUninitialize() };
    result
}

fn wait<T: RuntimeType>(
    operation: windows_future::IAsyncOperation<T>,
    timeout: Duration,
) -> Result<T, &'static str> {
    let deadline = Instant::now() + timeout;
    while operation.Status().map_err(|_| UNAVAILABLE)?.0 == 0 {
        if Instant::now() >= deadline {
            let _ = operation.Cancel();
            return Err("Dikte belum merespons. Coba lagi.");
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    operation.GetResults().map_err(|_| UNAVAILABLE)
}
