//! Explorer's thumbnail cache avoids full image decoding for common formats.
use image::RgbaImage;
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::SIZE,
        Graphics::Gdi::{
            CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
        },
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY},
    },
};

pub struct ComApartment(bool);
impl ComApartment {
    pub fn new() -> Self {
        Self(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() })
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

pub fn load(path: &Path) -> Option<(RgbaImage, (u32, u32))> {
    // Native dimensions must remain accurate when a thumbnail previews the main image.
    // Unsupported header readers fall back to the application's decoder.
    let dimensions = image::image_dimensions(path).ok()?;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None).ok()?;
        let bitmap = factory
            .GetImage(SIZE { cx: 163, cy: 163 }, SIIGBF_THUMBNAILONLY)
            .ok()?;
        let result = (|| {
            let mut descriptor = BITMAP::default();
            if GetObjectW(
                bitmap,
                std::mem::size_of::<BITMAP>() as i32,
                Some((&mut descriptor as *mut BITMAP).cast()),
            ) == 0
            {
                return None;
            }
            let width = descriptor.bmWidth;
            let height = descriptor.bmHeight;
            if width <= 0 || height <= 0 || width > 163 || height > 163 {
                return None;
            }
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let dc = CreateCompatibleDC(HDC::default());
            if dc.0.is_null() {
                return None;
            }
            let rows = GetDIBits(
                dc,
                bitmap,
                0,
                height as u32,
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            );
            let _ = DeleteDC(dc);
            if rows != height {
                return None;
            }
            // Shell bitmaps use premultiplied BGRA; the renderer takes straight RGBA.
            let has_alpha = pixels.chunks_exact(4).any(|pixel| pixel[3] != 0);
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                if !has_alpha {
                    pixel[3] = 255;
                }
                let alpha = u32::from(pixel[3]);
                if alpha > 0 {
                    for channel in &mut pixel[..3] {
                        *channel = (u32::from(*channel) * 255 / alpha).min(255) as u8;
                    }
                }
            }
            Some((
                RgbaImage::from_raw(width as u32, height as u32, pixels)?,
                dimensions,
            ))
        })();
        let _ = DeleteObject(bitmap);
        result
    }
}
