//! Branded connection QR for the host window: a QR code at error-correction
//! level `H` (matching the Universal_QR app, which pins `ecLevel: 'H'` so a centre
//! logo never breaks scanning) with the Universal Screens app icon composited in
//! the middle.

use eframe::egui;
use image::{imageops};

/// The UNI·SIM mark (globe + orbiting stars, 256×256), embedded at build time.
/// Used for the suite changelog mark and the centre of the connection QR.
const LOGO_PNG: &[u8] = include_bytes!("../assets/unisim-icon.png");
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
/// The UNI·SIM mark as an `egui::ColorImage` (square, `size` px), for the suite
/// changelog mark. `None` if the embedded PNG can't be decoded.
pub fn logo_image(size: u32) -> Option<egui::ColorImage> {
    decode_square(LOGO_PNG, size)
}

/// The Universal Screens app icon as an `egui::ColorImage` (square, `size` px), for
/// the navbar product logo. `None` if the embedded PNG can't be decoded.
pub fn app_icon_image(size: u32) -> Option<egui::ColorImage> {
    decode_square(APP_ICON_PNG, size)
}

/// The Universal Screens app icon as raw RGBA bytes (square, `size` px), for the
/// window/taskbar icon. `None` if the embedded PNG can't be decoded.
/// The signed-out profile avatar: the SDK's brand-orange disc with a simple
/// head-and-shoulders figure on it, replacing the word "Profile" in the navbar
/// so the host's chrome reads like `UserProfile` does in the web apps.
///
/// Painted in code rather than shipped as an asset. The SDK's artwork is four
/// SVG figures (`images/profile-icon/*.svg` on the marketing site) and this
/// binary has no SVG rasteriser -- `image` decodes PNG/JPEG only. A single
/// figure is drawn instead: the rotation those four exist for needs somewhere
/// to persist a counter across launches, which is a web-page idea, not a
/// desktop-app one.
///
/// Supersampled 4x and box-filtered down, the same trick scripts/make-app-icon.py
/// uses -- at 24px a hard-edged disc is visibly jagged next to the antialiased
/// text beside it.
pub fn profile_avatar_image(size: u32) -> egui::ColorImage {
    const K: u32 = 4;
    let s = size * K;
    let sf = s as f32;
    // Brand orange, matching BRAND in gui.rs and the suite's chrome.
    let (br, bg, bb) = (0xe0u32, 0x55u32, 0x04u32);
    let mut buf = vec![0u8; (size * size * 4) as usize];

    let centre = sf / 2.0;
    let disc_r = sf / 2.0;
    // Head and shoulders, as fractions of the disc -- tuned so the figure reads
    // at 24px rather than turning into a blob.
    let head_c = (centre, sf * 0.38);
    let head_r = sf * 0.16;
    let sh_c = (centre, sf * 0.86);
    let sh_r = sf * 0.30;

    for y in 0..size {
        for x in 0..size {
            let (mut a_sum, mut fig_sum) = (0u32, 0u32);
            for sy in 0..K {
                for sx in 0..K {
                    let px = (x * K + sx) as f32 + 0.5;
                    let py = (y * K + sy) as f32 + 0.5;
                    let d = ((px - centre).powi(2) + (py - centre).powi(2)).sqrt();
                    if d > disc_r {
                        continue;
                    }
                    a_sum += 1;
                    let in_head =
                        ((px - head_c.0).powi(2) + (py - head_c.1).powi(2)).sqrt() <= head_r;
                    // Shoulders are the top half of a disc, so the figure sits on
                    // the avatar's lower edge the way the SDK's do.
                    let in_sh = py >= sh_c.1 - sh_r
                        && ((px - sh_c.0).powi(2) + (py - sh_c.1).powi(2)).sqrt() <= sh_r;
                    if in_head || in_sh {
                        fig_sum += 1;
                    }
                }
            }
            let total = (K * K) as f32;
            let alpha = a_sum as f32 / total;
            let fig = fig_sum as f32 / total;
            // Figure is white over the orange, both weighted by coverage.
            let mix = |c: u32| -> u8 {
                let base = c as f32;
                (base + (255.0 - base) * (if alpha > 0.0 { fig / alpha } else { 0.0 })).round() as u8
            };
            let i = ((y * size + x) * 4) as usize;
            buf[i] = mix(br);
            buf[i + 1] = mix(bg);
            buf[i + 2] = mix(bb);
            buf[i + 3] = (alpha * 255.0).round() as u8;
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &buf)
}

pub fn app_icon_rgba(size: u32) -> Option<Vec<u8>> {
    let logo = image::load_from_memory(APP_ICON_PNG)
        .ok()?
        .resize_exact(size, size, imageops::FilterType::Lanczos3)
        .to_rgba8();
    Some(logo.into_raw())
}

/// Decode `png` to a square `egui::ColorImage` of `size` px.
fn decode_square(png: &[u8], size: u32) -> Option<egui::ColorImage> {
    let rgba = image::load_from_memory(png)
        .ok()?
        .resize_exact(size, size, imageops::FilterType::Lanczos3)
        .to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [size as usize, size as usize],
        &rgba.into_raw(),
    ))
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
