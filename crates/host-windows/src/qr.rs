//! The branded connection QR for the host window: a QR code at error-correction
//! level `H` (matching the Universal_QR app, which pins `ecLevel: 'H'` so a centre
//! logo never breaks scanning) with the UNI·SIM mark composited in the middle.
//!
//! The Universal_QR app itself is a browser tool (the `qr-code-styling` canvas
//! library), so it can't run inside this Rust window — this reproduces its key
//! branded result: black-on-white modules (most reliable to scan) + the centre
//! logo.

use eframe::egui;
use image::{imageops, Rgba, RgbaImage};

/// The UNI·SIM mark (globe + orbiting stars, 256×256), embedded at build time.
/// Used for the suite changelog mark and the centre of the connection QR.
const LOGO_PNG: &[u8] = include_bytes!("../assets/unisim-icon.png");
/// The Universal Screens app icon (laptop + phone, 256×256). See
/// `scripts/make-app-icon.py`. Used for the window/taskbar icon and the navbar
/// product logo.
const APP_ICON_PNG: &[u8] = include_bytes!("../assets/app-icon.png");

/// Build a black-on-white QR of `text` with the UNI·SIM logo centred, as an
/// `egui::ColorImage`. `None` if the text won't fit in a QR code.
///
/// Used for **Step 1** (the "get the app" suite-page QR) — a UNI·SIM-branded
/// link a phone camera can open.
pub fn branded_qr(text: &str) -> Option<egui::ColorImage> {
    branded_qr_with(text, LOGO_PNG)
}

/// Like [`branded_qr`] but with the **Universal Screens app icon** centred
/// instead of the generic UNI·SIM mark. Used for **Step 2** (the in-app connect
/// QRs) so the code reads as "this is for the Universal Screens app".
pub fn branded_qr_app(text: &str) -> Option<egui::ColorImage> {
    branded_qr_with(text, APP_ICON_PNG)
}

fn branded_qr_with(text: &str, centre_png: &[u8]) -> Option<egui::ColorImage> {
    let code =
        qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::H).ok()?;
    let modules = code.width();
    let colors = code.to_colors();

    let quiet = 4usize;
    let scale = 8usize;
    let dim = ((modules + quiet * 2) * scale) as u32;

    // White canvas with black modules (inside the quiet-zone border).
    let mut img = RgbaImage::from_pixel(dim, dim, Rgba([255, 255, 255, 255]));
    for y in 0..modules {
        for x in 0..modules {
            if colors[y * modules + x] != qrcode::Color::Dark {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let px = ((x + quiet) * scale + dx) as u32;
                    let py = ((y + quiet) * scale + dy) as u32;
                    img.put_pixel(px, py, Rgba([0, 0, 0, 255]));
                }
            }
        }
    }

    overlay_logo(&mut img, dim, centre_png);
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [dim as usize, dim as usize],
        &img.into_raw(),
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

/// Composite the logo in the centre over a white pad, clearing the modules behind
/// it (like the generator's `hideBackgroundDots`). EC level `H` keeps the code
/// scannable despite the occlusion.
fn overlay_logo(img: &mut RgbaImage, dim: u32, logo_png: &[u8]) {
    let Ok(logo) = image::load_from_memory(logo_png) else {
        return;
    };
    let logo_size = (dim as f32 * 0.26) as u32; // ~ the generator's logoSize 0.28
    let pad = (dim as f32 * 0.32) as u32;
    let logo = logo
        .resize_exact(logo_size, logo_size, imageops::FilterType::Lanczos3)
        .to_rgba8();

    // White pad behind the logo.
    let pad_x = (dim - pad) / 2;
    let pad_y = (dim - pad) / 2;
    for y in pad_y..pad_y + pad {
        for x in pad_x..pad_x + pad {
            img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
        }
    }

    // Alpha-composite the logo, centred.
    let lx = (dim - logo_size) / 2;
    let ly = (dim - logo_size) / 2;
    for (x, y, px) in logo.enumerate_pixels() {
        let a = f32::from(px.0[3]) / 255.0;
        if a <= 0.0 {
            continue;
        }
        let (dstx, dsty) = (lx + x, ly + y);
        let bg = img.get_pixel(dstx, dsty).0;
        let blend = |fg: u8, bg: u8| (f32::from(fg) * a + f32::from(bg) * (1.0 - a)) as u8;
        img.put_pixel(
            dstx,
            dsty,
            Rgba([blend(px.0[0], bg[0]), blend(px.0[1], bg[1]), blend(px.0[2], bg[2]), 255]),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branded_qr_builds_and_composites_the_logo() {
        let img = branded_qr("10.0.0.5:9100").expect("QR should build");
        // Square and a sensible pixel size.
        assert_eq!(img.size[0], img.size[1]);
        assert!(img.size[0] > 100);

        // The embedded (coloured) logo should land in the centre: scan the middle
        // for a pixel that is neither pure white nor pure black, proving the
        // overlay ran (a logo-less QR centre would be only black/white).
        let w = img.size[0];
        let lo = w * 2 / 5;
        let hi = w * 3 / 5;
        let coloured = (lo..hi).any(|y| {
            (lo..hi).any(|x| {
                let p = img.pixels[y * w + x];
                let (r, g, b) = (p.r(), p.g(), p.b());
                !(r == 255 && g == 255 && b == 255) && !(r == 0 && g == 0 && b == 0)
            })
        });
        assert!(coloured, "expected the UNI·SIM logo in the centre");
    }

    #[test]
    fn branded_qr_app_builds_with_app_icon_centre() {
        let img = branded_qr_app("unisimscreens://connect?host=10.0.0.5&port=9100&pin=1234")
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
