//! The reminder pill drawn over the picture: "Press Esc to exit full screen",
//! "Press Esc to release the mouse". The window is otherwise only the other
//! computer's screen, and the keys that leave it were printed to a terminal
//! that nobody launching from the app ever sees.
//!
//! The pill is drawn here on the CPU, once per message, into a small straight
//! (not premultiplied) RGBA bitmap; the renderer blends it top-centre for
//! [`SHOW_FOR`], then fades it over [`FADE`]. The font is the one the host
//! window already uses (egui's Ubuntu Light, via `epaint_default_fonts`), so
//! the two windows read as one app.

use std::time::Duration;

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};

/// How long a reminder stays fully visible, counted from the first frame of the
/// picture it sits on (not from when it was asked for: a full-screen start is
/// black until the stream arrives, and a reminder on black is easily missed).
pub const SHOW_FOR: Duration = Duration::from_millis(3500);
/// How long it then takes to fade out.
pub const FADE: Duration = Duration::from_millis(600);

/// Text size and padding, in logical pixels (multiplied by the screen's scale).
const TEXT_PX: f32 = 18.0;
const PAD_X: f32 = 22.0;
const PAD_Y: f32 = 11.0;
/// Gap between the top of the window and the pill, in logical pixels.
pub const TOP_MARGIN: f32 = 40.0;

/// The pill's background: near-black, mostly opaque, so white text stays
/// readable over any wallpaper.
const BG: [u8; 3] = [24, 24, 24];
const BG_ALPHA: f32 = 0.82;

/// Straight-alpha RGBA, tightly packed (stride == width * 4).
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Bitmap {
    /// A copy with every pixel's alpha multiplied by `fade` (0.0..=1.0).
    pub fn faded(&self, fade: f32) -> Vec<u8> {
        let fade = fade.clamp(0.0, 1.0);
        let mut out = self.rgba.clone();
        for px in out.chunks_exact_mut(4) {
            px[3] = (f32::from(px[3]) * fade).round() as u8;
        }
        out
    }
}

/// How visible a reminder is `elapsed` after its picture first showed:
/// 1.0, then a linear fade to 0.0. `None` once it has gone.
pub fn visibility(elapsed: Duration) -> Option<f32> {
    if elapsed < SHOW_FOR {
        Some(1.0)
    } else if elapsed < SHOW_FOR + FADE {
        Some(1.0 - (elapsed - SHOW_FOR).as_secs_f32() / FADE.as_secs_f32())
    } else {
        None
    }
}

/// Draw `text` in a rounded pill at `scale` (the window's scale factor, 1.5 on
/// a screen set to 150%).
pub fn rasterise(text: &str, scale: f64) -> Bitmap {
    let font = FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).expect("bundled font parses");
    let s = scale.max(0.5) as f32;
    let px = PxScale::from(TEXT_PX * s);
    let scaled = font.as_scaled(px);

    // Lay the line out left to right, with kerning.
    let mut glyphs = Vec::new();
    let mut caret = 0.0f32;
    let mut last = None;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(prev) = last {
            caret += scaled.kern(prev, id);
        }
        glyphs.push(id.with_scale_and_position(px, point(caret, 0.0)));
        caret += scaled.h_advance(id);
        last = Some(id);
    }

    let pad_x = PAD_X * s;
    let pad_y = PAD_Y * s;
    let text_h = scaled.ascent() - scaled.descent();
    let width = (caret + 2.0 * pad_x).ceil().max(1.0) as u32;
    let height = (text_h + 2.0 * pad_y).ceil().max(1.0) as u32;
    let baseline = pad_y + scaled.ascent();

    // Background: a pill (radius = half the height), edges antialiased by
    // distance to the rounded rectangle.
    let (w, h) = (width as f32, height as f32);
    let radius = h / 2.0;
    let mut alpha_bg = vec![0f32; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (cx - cx.clamp(radius, w - radius)).abs();
            let dy = (cy - cy.clamp(radius, h - radius)).abs();
            let dist = (dx * dx + dy * dy).sqrt() - radius;
            let cover = (0.5 - dist).clamp(0.0, 1.0);
            alpha_bg[(y * width + x) as usize] = cover * BG_ALPHA;
        }
    }

    // Text coverage.
    let mut cover_text = vec![0f32; (width * height) as usize];
    for glyph in glyphs {
        let Some(outlined) = font.outline_glyph(glyph) else { continue };
        let bounds = outlined.px_bounds();
        let ox = bounds.min.x + pad_x;
        let oy = bounds.min.y + baseline;
        outlined.draw(|gx, gy, c| {
            let x = ox as i32 + gx as i32;
            let y = oy as i32 + gy as i32;
            if x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height {
                let i = (y as u32 * width + x as u32) as usize;
                cover_text[i] = (cover_text[i] + c).min(1.0);
            }
        });
    }

    // White text over the background, kept as straight alpha.
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for i in 0..(width * height) as usize {
        let (ab, c) = (alpha_bg[i], cover_text[i]);
        let a = c + ab * (1.0 - c);
        let px = &mut rgba[i * 4..i * 4 + 4];
        if a > 0.0 {
            for k in 0..3 {
                let v = (255.0 * c + f32::from(BG[k]) * ab * (1.0 - c)) / a;
                px[k] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
        px[3] = (a * 255.0).round() as u8;
    }

    Bitmap { width, height, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pill_has_white_text_on_a_dark_background() {
        let b = rasterise("Press Esc to exit full screen", 1.5);
        assert_eq!(b.rgba.len(), (b.width * b.height * 4) as usize);
        // Wide and short, sized for the scale.
        assert!(b.width > 300 && b.width < 700, "width {}", b.width);
        assert!(b.height > 40 && b.height < 80, "height {}", b.height);
        // Corners are transparent (it's a pill), the middle row is not.
        assert_eq!(b.rgba[3], 0);
        let mid = (b.height / 2 * b.width + 4) as usize * 4;
        assert!(b.rgba[mid + 3] > 150);
        // Somewhere there's near-white, fully covered text.
        assert!(b.rgba.chunks_exact(4).any(|p| p[0] > 240 && p[3] > 240));
    }

    #[test]
    fn it_is_bigger_on_a_bigger_scale() {
        let small = rasterise("Press Esc", 1.0);
        let big = rasterise("Press Esc", 2.0);
        assert!(big.width > small.width * 3 / 2);
    }

    #[test]
    fn it_shows_then_fades_then_goes() {
        assert_eq!(visibility(Duration::ZERO), Some(1.0));
        let mid = visibility(SHOW_FOR + FADE / 2).unwrap();
        assert!((mid - 0.5).abs() < 0.01);
        assert_eq!(visibility(SHOW_FOR + FADE), None);
    }

    #[test]
    fn fading_only_touches_alpha() {
        let b = rasterise("Esc", 1.0);
        let half = b.faded(0.5);
        for (a, h) in b.rgba.chunks_exact(4).zip(half.chunks_exact(4)) {
            assert_eq!(&a[..3], &h[..3]);
            assert_eq!(h[3], (f32::from(a[3]) * 0.5).round() as u8);
        }
    }
}
