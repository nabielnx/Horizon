use std::sync::OnceLock;
use windows::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, LOGPIXELSX, ReleaseDC};

/// The notch and taskbar are anchored to the primary display.
#[derive(Clone, Copy)]
pub(super) struct Scale(u32);

impl Scale {
    pub const STANDARD: Self = Self(96);

    pub const fn new(dpi: u32) -> Self {
        Self(if dpi < 96 { 96 } else { dpi })
    }

    pub fn system() -> Self {
        static SYSTEM: OnceLock<Scale> = OnceLock::new();
        *SYSTEM.get_or_init(|| unsafe {
            let dc = GetDC(None);
            if dc.is_invalid() {
                return Self::STANDARD;
            }
            let dpi = GetDeviceCaps(Some(dc), LOGPIXELSX);
            ReleaseDC(None, dc);
            Self::new(dpi.max(0) as u32)
        })
    }

    pub fn dpi(self) -> f32 {
        self.0 as f32
    }

    pub fn physical(self, dips: f32) -> f32 {
        dips * (self.dpi() / 96.)
    }

    pub fn logical(self, pixels: f32) -> f32 {
        pixels * (96. / self.dpi())
    }

    pub fn px(self, dips: i32) -> i32 {
        self.physical(dips as f32).round() as i32
    }

    pub fn dip(self, pixels: i32) -> i32 {
        self.logical(pixels as f32).round() as i32
    }

    pub fn point(self, x: i32, y: i32) -> (i32, i32) {
        (
            self.logical(x as f32).floor() as i32,
            self.logical(y as f32).floor() as i32,
        )
    }
}
