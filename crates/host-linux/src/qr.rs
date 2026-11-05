//! Branded QR codes for the host window: a QR at error-correction level `H`
//! (matching the Universal_QR app, which pins `ecLevel: 'H'` so a centre logo
//! never breaks scanning) with the Universal Screens app icon composited in the
//! middle.
//!
//! Trimmed from the Windows host's copy: the navbar helpers (suite mark, product
//! logo, profile avatar) went with the navbar this window doesn't have. See the
//! note at the top of `gui.rs` on why that navbar is missing.

use eframe::egui;
use image::{imageops};

/// The Universal Screens app icon (laptop + phone, 256×256). See
/// `scripts/make-app-icon.py`. Used for the window/taskbar icon and the navbar
/// product logo.
const APP_ICON_PNG: &[u8] = include_bytes!("../assets/app-icon.png");

/// Build a QR of `text` with the Universal Screens app icon in the middle, as
/// an `egui::ColorImage`. Scanned *in the app* it deep-links to connect;
/// scanned with a plain camera it lands on the friendly `/screens/connect`
/// page. `None` if the text will not fit in a QR code.
///
/// The drawing lives in `extender-qr`, shared by all three hosts — this used
/// to be three copies of the same renderer, quietly drifting. Each code is
/// dealt a style at random from Universal QR's presets (James, 2026-11-05);
/// every one of them is decode-tested in that crate, because a style nobody
/// can scan would leave the user typing an address and a PIN by hand.
pub fn branded_qr_app(text: &str) -> Option<egui::ColorImage> {
    // The style showing now, so the next code is visibly a different one
    // rather than the same style re-dealt. Process-wide: there is one host
    // window, and the alternative is threading state through the UI for a
    // purely cosmetic choice.
    static LAST_STYLE: std::sync::Mutex<Option<&'static str>> = std::sync::Mutex::new(None);
    let last = LAST_STYLE.lock().ok().and_then(|g| *g);
    let preset = extender_qr::random_preset(last);
    if let Ok(mut g) = LAST_STYLE.lock() {
        *g = Some(preset.name);
    }
    let r = extender_qr::render(text, preset, Some(APP_ICON_PNG))?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [r.dim as usize, r.dim as usize],
        &r.rgba,
    ))
}
/// The Universal Screens app icon as raw RGBA bytes (square, `size` px), for the
/// window/taskbar icon. `None` if the embedded PNG can't be decoded.
pub fn app_icon_rgba(size: u32) -> Option<Vec<u8>> {
    let logo = image::load_from_memory(APP_ICON_PNG)
        .ok()?
        .resize_exact(size, size, imageops::FilterType::Lanczos3)
        .to_rgba8();
    Some(logo.into_raw())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branded_qr_app_builds_with_app_icon_centre() {
        let img = branded_qr_app(
            "https://opensource.unisim.co.uk/screens/connect?host=10.0.0.5&port=9100&pin=1234",
        )
        .expect("app QR should build");
        assert_eq!(img.size[0], img.size[1]);
        let w = img.size[0];
        let (lo, hi) = (w * 2 / 5, w * 3 / 5);
        let coloured = (lo..hi).any(|y| {
            (lo..hi).any(|x| {
                let p = img.pixels[y * w + x];
                let (r, g, b) = (p.r(), p.g(), p.b());
                !(r == 255 && g == 255 && b == 255) && !(r == 0 && g == 0 && b == 0)
            })
        });
        assert!(coloured, "expected the app icon in the centre");
    }
}
