//! Primary-screen capture → JPEG for the clicker's slide preview. Uses plain GDI
//! (`BitBlt` + `GetDIBits`), which is enough for a still preview: it grabs the
//! primary display only and isn't DPI-aware (the image is downscaled for the
//! preview anyway, so logical-vs-physical pixels don't matter here).

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageBuffer, Rgb};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

/// Capture the primary display, downscale so its longest side is at most `max_dim`
/// px, and JPEG-encode it at `quality`. Returns `(width, height, jpeg_bytes)` of
/// the (possibly downscaled) image, or `None` if the capture failed.
pub fn capture_primary_jpeg(max_dim: u32, quality: u8) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h, bgra) = unsafe { grab_primary_bgra()? };

    // Windows DIB 32bpp rows are BGRA; repack into an RGB image buffer.
    let mut rgb: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(w, h);
    for (i, px) in rgb.pixels_mut().enumerate() {
        let o = i * 4;
        *px = Rgb([bgra[o + 2], bgra[o + 1], bgra[o]]);
    }

    // Downscale to keep the preview small over the wire.
    let img = DynamicImage::ImageRgb8(rgb);
    let scaled = if w.max(h) > max_dim {
        img.resize(max_dim, max_dim, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgb = scaled.into_rgb8();
    let (sw, sh) = (rgb.width(), rgb.height());

    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .encode(rgb.as_raw(), sw, sh, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some((sw, sh, out))
}

/// Capture the primary display into a top-down BGRA byte buffer, returning its
/// pixel dimensions and the bytes. Returns `None` if any GDI step fails.
unsafe fn grab_primary_bgra() -> Option<(u32, u32, Vec<u8>)> {
    let width = GetSystemMetrics(SM_CXSCREEN);
    let height = GetSystemMetrics(SM_CYSCREEN);
    if width <= 0 || height <= 0 {
        return None;
    }

    let hdc_screen = GetDC(None);
    if hdc_screen.0.is_null() {
        return None;
    }
    let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
    let hbm = CreateCompatibleBitmap(hdc_screen, width, height);
    let old = SelectObject(hdc_mem, hbm.into());

    let blt_ok = BitBlt(hdc_mem, 0, 0, width, height, Some(hdc_screen), 0, 0, SRCCOPY).is_ok();

    let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
    // Negative biHeight requests top-down rows (origin at top-left).
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).unwrap(),
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let scanlines = GetDIBits(
        hdc_mem,
        hbm,
        0,
        height as u32,
        Some(buf.as_mut_ptr().cast()),
        &mut bmi,
        DIB_RGB_COLORS,
    );

    // Release GDI resources regardless of success.
    SelectObject(hdc_mem, old);
    let _ = DeleteObject(hbm.into());
    let _ = DeleteDC(hdc_mem);
    ReleaseDC(None, hdc_screen);

    if !blt_ok || scanlines == 0 {
        return None;
    }
    Some((width as u32, height as u32, buf))
}
