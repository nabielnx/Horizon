use windows::{
    Win32::{Graphics::Imaging::*, System::Com::*},
    core::*,
};
pub unsafe fn source(bytes: &[u8], size: u32) -> Result<IWICBitmapSource> {
    source_rect(bytes, size, size)
}
/// Static UI exports retain their native pixel dimensions and alpha. Upscaling
/// every small glyph to a cover-sized bitmap wastes substantial private memory.
pub unsafe fn source_native(bytes: &[u8]) -> Result<IWICBitmapSource> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromMemory(bytes)?;
        let decoder = factory.CreateDecoderFromStream(
            &stream,
            std::ptr::null(),
            WICDecodeMetadataCacheOnLoad,
        )?;
        let frame = decoder.GetFrame(0)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.,
            WICBitmapPaletteTypeCustom,
        )?;
        converter.cast()
    }
}

#[derive(Clone)]
pub struct PreviewImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Fit without cropping or changing the source aspect ratio.
pub fn fit_size(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let scale = (max_width as f64 / width.max(1) as f64)
        .min(max_height as f64 / height.max(1) as f64)
        .min(1.);
    (
        (width as f64 * scale).round().max(1.) as u32,
        (height as f64 * scale).round().max(1.) as u32,
    )
}

pub unsafe fn preview(bytes: &[u8]) -> Result<PreviewImage> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromMemory(bytes)?;
        let decoder = factory.CreateDecoderFromStream(
            &stream,
            std::ptr::null(),
            WICDecodeMetadataCacheOnLoad,
        )?;
        let frame = decoder.GetFrame(0)?;
        let (mut width, mut height) = (0, 0);
        frame.GetSize(&mut width, &mut height)?;
        let (width, height) = fit_size(width, height, 640, 320);
        let scaler = factory.CreateBitmapScaler()?;
        scaler.Initialize(&frame, width, height, WICBitmapInterpolationModeFant)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &scaler,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.,
            WICBitmapPaletteTypeCustom,
        )?;
        let mut pixels = vec![0; width as usize * height as usize * 4];
        converter.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
        Ok(PreviewImage {
            width,
            height,
            pixels,
        })
    }
}

pub unsafe fn source_rect(bytes: &[u8], width: u32, height: u32) -> Result<IWICBitmapSource> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromMemory(bytes)?;
        let decoder = factory.CreateDecoderFromStream(
            &stream,
            std::ptr::null(),
            WICDecodeMetadataCacheOnLoad,
        )?;
        let frame = decoder.GetFrame(0)?;
        let scaler = factory.CreateBitmapScaler()?;
        scaler.Initialize(
            &frame,
            width.max(1),
            height.max(1),
            WICBitmapInterpolationModeFant,
        )?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &scaler,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.,
            WICBitmapPaletteTypeCustom,
        )?;
        converter.cast()
    }
}
pub fn palette(bytes: &[u8]) -> Option<[u32; 3]> {
    unsafe {
        let image = source(bytes, 32).ok()?;
        let mut pixels = [0u8; 4096];
        image.CopyPixels(std::ptr::null(), 128, &mut pixels).ok()?;
        visible_palette(&pixels)
    }
}
fn visible_palette(pixels: &[u8]) -> Option<[u32; 3]> {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p[3] >= 64)
        .then(|| from_pixels(pixels))
}
pub fn from_pixels(pixels: &[u8]) -> [u32; 3] {
    let mut buckets = [[0u32; 4]; 64];
    for p in pixels.as_chunks::<4>().0 {
        if p[3] < 64 {
            continue;
        }
        if p[0].max(p[1]).max(p[2]) < 12 {
            continue;
        }
        let i = ((p[2] >> 6) as usize) * 16 + ((p[1] >> 6) as usize) * 4 + (p[0] >> 6) as usize;
        let b = &mut buckets[i];
        b[0] += 1;
        b[1] += p[2] as u32;
        b[2] += p[1] as u32;
        b[3] += p[0] as u32;
    }
    // Prefer meaningful cover accents over a large black/grey background.
    // Keep the existing dark tone encoding so material and wave colours agree.
    let rgb = |b: &[u32; 4]| [b[1] / b[0], b[2] / b[0], b[3] / b[0]];
    let score = |b: &[u32; 4]| {
        let c = rgb(b);
        (b[0] as f64).sqrt()
            * (0.25 + (c.iter().max().unwrap() - c.iter().min().unwrap()) as f64 / 255.)
    };
    let total: u32 = buckets.iter().map(|b| b[0]).sum();
    let chromatic = |b: &[u32; 4]| {
        if b[0] == 0 {
            return false;
        }
        let c = rgb(b);
        let hi = *c.iter().max().unwrap();
        let lo = *c.iter().min().unwrap();
        hi >= 24 && hi - lo >= 20 && (hi - lo) as f32 / hi as f32 >= 0.22
    };
    let chromatic_count: u32 = buckets
        .iter()
        .filter(|b| chromatic(b) && b[0] >= 2)
        .map(|b| b[0])
        .sum();
    let prefer_colour = chromatic_count >= (total / 25).max(2);
    let mut primary = None::<&[u32; 4]>;
    for b in &buckets {
        if b[0] > 0
            && (!prefer_colour || (chromatic(b) && b[0] >= 2))
            && primary.is_none_or(|p| score(b) > score(p))
        {
            primary = Some(b);
        }
    }
    let Some(primary) = primary else {
        return crate::render::palette(0);
    };
    let first = rgb(primary);
    let mut second = first;
    let mut distance = 0.;
    for b in buckets.iter().filter(|b| b[0] > 0) {
        if score(b) < score(primary) * 0.12 {
            continue;
        }
        let candidate = rgb(b);
        let d = (0..3)
            .map(|i| (candidate[i] as f64 - first[i] as f64).powi(2))
            .sum::<f64>()
            * (b[0] as f64).sqrt();
        if d > distance {
            distance = d;
            second = candidate;
        }
    }
    let blend = std::array::from_fn(|i| (first[i] + second[i]) / 2);
    let tone = |rgb: [u32; 3], gain: f64| {
        let c = rgb.map(|c| (8. + c as f64 * gain) as u32);
        (c[0] << 16) | (c[1] << 8) | c[2]
    };
    [tone(first, 0.34), tone(blend, 0.24), tone(second, 0.16)]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transparent_placeholders_are_not_accepted_as_cover_images() {
        assert!(visible_palette(&[0, 0, 0, 0].repeat(1024)).is_none());
        assert!(visible_palette(&[0, 0, 0, 255].repeat(1024)).is_some());
    }
    #[test]
    fn preview_fit_preserves_portrait_landscape_and_small_images() {
        assert_eq!(fit_size(1200, 800, 640, 320), (480, 320));
        assert_eq!(fit_size(800, 1200, 640, 320), (213, 320));
        assert_eq!(fit_size(40, 20, 640, 320), (40, 20));
    }

    #[test]
    fn preview_decodes_to_bounded_pixels() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let image = preview(include_bytes!("../assets/fixtures/cover.png")).unwrap();
            assert!(image.width <= 640 && image.height <= 320);
            assert_eq!(
                image.pixels.len(),
                image.width as usize * image.height as usize * 4
            );
        }
    }
    #[test]
    fn palette_stays_dark_and_respects_cover() {
        let p = from_pixels(&[20, 30, 220, 255].repeat(256));
        assert!((p[0] >> 16) & 255 > (p[0] & 255));
        assert!((p[0] >> 16) & 255 < 120);
        assert_ne!(p[0], p[2]);
    }
    #[test]
    fn cover_accent_survives_a_mostly_dark_neutral_background() {
        let mut pixels = [20, 20, 20, 255].repeat(230);
        pixels.extend([220, 120, 20, 255].repeat(26));
        let primary = from_pixels(&pixels)[0];
        assert!((primary & 255) > ((primary >> 16) & 255) * 2);
        let neutral = from_pixels(&[100, 100, 100, 255].repeat(256))[0];
        assert_eq!(neutral & 255, (neutral >> 16) & 255);
    }
    #[test]
    fn matches_previous_design_palette() {
        assert_eq!(
            from_pixels(&[20, 30, 220, 255].repeat(256)),
            [0x52120e, 0x3c0f0c, 0x2b0c0b]
        );
        assert_eq!(
            from_pixels(&[0, 0, 0, 255].repeat(256)),
            crate::render::palette(0)
        );
        let mut mixed = [70, 30, 180, 255].repeat(180);
        mixed.extend([190, 120, 30, 255].repeat(76));
        let p = from_pixels(&mixed);
        assert!(p[0] >> 16 > p[0] & 255);
        assert!(p[2] & 255 > p[2] >> 16);
    }
}
