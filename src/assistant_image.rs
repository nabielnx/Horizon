use windows::{
    Win32::{
        Foundation::{HGLOBAL, HWND},
        Graphics::{Gdi::HBITMAP, Imaging::*},
        System::{
            Com::{StructuredStorage::*, *},
            DataExchange::*,
            Memory::*,
        },
    },
    core::*,
};

const CF_BITMAP: u32 = 2;
const MAX_PNG: usize = 4 * 1024 * 1024;

pub unsafe fn paste(hwnd: HWND) -> Option<Result<Vec<u8>>> {
    unsafe {
        if !IsClipboardFormatAvailable(CF_BITMAP).is_ok() {
            return None;
        }
        if let Err(error) = OpenClipboard(Some(hwnd)) {
            return Some(Err(error));
        }
        let result = (|| {
            let bitmap = GetClipboardData(CF_BITMAP)?;
            let factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let source = factory.CreateBitmapFromHBITMAP(
                HBITMAP(bitmap.0),
                Default::default(),
                WICBitmapIgnoreAlpha,
            )?;
            png(&factory, &source.cast()?)
        })();
        let _ = CloseClipboard();
        Some(result)
    }
}

unsafe fn png(factory: &IWICImagingFactory, source: &IWICBitmapSource) -> Result<Vec<u8>> {
    unsafe {
        let (mut width, mut height) = (0, 0);
        source.GetSize(&mut width, &mut height)?;
        let (width, height) = crate::artwork::fit_size(width, height, 1280, 1280);
        let scaler = factory.CreateBitmapScaler()?;
        scaler.Initialize(source, width, height, WICBitmapInterpolationModeFant)?;
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
        let encoder = factory.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let (mut frame, mut options) = (None, None);
        encoder.CreateNewFrame(&mut frame, &mut options)?;
        let frame = frame.ok_or_else(Error::from_thread)?;
        frame.Initialize(options.as_ref())?;
        frame.WriteSource(&scaler, std::ptr::null())?;
        frame.Commit()?;
        encoder.Commit()?;
        let mut stat = STATSTG::default();
        stream.Stat(&mut stat, STATFLAG_NONAME)?;
        let size = stat.cbSize as usize;
        if size == 0 || size > MAX_PNG {
            return Err(Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                "Clipboard image exceeds the size limit",
            ));
        }
        let memory = GetHGlobalFromStream(&stream)?;
        let bytes = GlobalLock(memory);
        if bytes.is_null() {
            return Err(Error::from_thread());
        }
        let result = std::slice::from_raw_parts(bytes.cast::<u8>(), size).to_vec();
        let _ = GlobalUnlock(memory);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_encoder_produces_a_readable_png() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).unwrap();
            let bitmap = factory
                .CreateBitmapFromMemory(
                    2,
                    2,
                    &GUID_WICPixelFormat32bppBGRA,
                    8,
                    &[
                        0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
                    ],
                )
                .unwrap();
            let encoded = png(&factory, &bitmap.cast().unwrap()).unwrap();
            assert!(encoded.starts_with(b"\x89PNG\r\n\x1a\n"));
            let image = crate::artwork::preview(&encoded).unwrap();
            assert_eq!((image.width, image.height), (2, 2));
        }
    }
}
