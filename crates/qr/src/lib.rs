//! Branded, styled QR codes for the host windows.
//!
//! One implementation for all three hosts. Windows, macOS and Linux each used
//! to carry their own `qr.rs` with the same code drifting apart in it (228,
//! 175 and 133 lines by the time this crate was written); the rendering lives
//! here now and each host keeps only the thin wrapper that turns the pixels
//! into whatever its UI toolkit wants.
//!
//! ── Styles ───────────────────────────────────────────────────────────────────
//!
//! Each code is drawn in one of several styles, picked at random (James,
//! 2026-11-05). The styles are Universal QR's presets — see
//! `backoffice/universal-platform/packages/qr/src/design.ts`, the suite's
//! single source of truth for QR design, which the web apps share as
//! `@unisim/qr`.
//!
//! ⚠️ THIS IS A SECOND RENDERER, NOT A SHARED ONE, AND THAT IS THE RISK.
//! `@unisim/qr` is TypeScript built on `qr-code-styling`, a DOM library; this
//! is a Rust painter writing pixels. The values below are copied, so they can
//! drift from the originals without anything failing — the suite has had that
//! exact bug before (Universal PDF copied Universal QR's model, and a field one
//! side did not know about was dropped in silence, producing a different
//! picture rather than an error). Two rules follow from that:
//!
//!   1. Copy VALUES, never invent them. If a preset here disagrees with
//!      design.ts, design.ts is right.
//!   2. Say what is NOT ported, rather than approximating it quietly. See
//!      "What is missing" below.
//!
//! ── Scannability ─────────────────────────────────────────────────────────────
//!
//! ⚠️ The guarantee does NOT travel with the values. design.ts can say "every
//! preset here has been decoded through this pipeline, so a random one is
//! always scannable" because it was measured against `qr-code-styling` with
//! `@zxing/library`. None of that applies to the painter below, and this code
//! is the PAIRING path: a phone that cannot read it leaves the user typing an
//! address and a PIN by hand. So every preset is decoded by this crate's own
//! tests, at several sizes, with the centre logo in place. A style that will
//! not decode does not ship — that is what `every_preset_decodes` is for.
//!
//! Error correction is pinned at `H` for the same reason Universal QR pins it:
//! the centre logo covers real modules.
//!
//! ── What is missing ──────────────────────────────────────────────────────────
//!
//! Deliberately not ported, and therefore deliberately absent from `PRESETS`:
//!
//!   * `decorStyle: 'burst'` — the decorative marks Universal QR scatters
//!     around a shaped code.
//!   * `frameShape: 'circle'` — the circular plate.
//!
//! Those two are the whole of the **Radial** preset, so Radial is not offered
//! here rather than being offered as something that looks nothing like it.
//! **Star** IS offered, because its `starPlacement: 'behind'` only paints a
//! backdrop in the margin — it never touches a module, so it costs nothing in
//! legibility. Adding Radial means implementing the plate and the burst, and
//! then adding it to the decode tests like everything else.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// How a data module is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DotShape {
    /// `dotType: 'square'`
    Square,
    /// `dotType: 'rounded'`
    Rounded,
    /// `dotType: 'extra-rounded'`
    ExtraRounded,
    /// `dotType: 'dots'`
    Dot,
}

/// How a finder pattern's outer ring, or its centre, is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EyeShape {
    Square,
    Rounded,
    Dot,
}

/// A named style. Fields map one-to-one onto `design.ts`; the doc comment on
/// each says which.
#[derive(Clone, Copy, Debug)]
pub struct Preset {
    /// Matches the `name` in design.ts, so the two can be compared by eye.
    pub name: &'static str,
    /// `dotType`
    pub dot: DotShape,
    /// `cornerSquareType`.
    ///
    /// ⚠️ ALWAYS `Square` here, which is a DELIBERATE divergence from
    /// design.ts — Rounded and Dots ask for 'extra-rounded' and 'dot'. A
    /// decoder finds a QR by scanning for the finder pattern's 1:1:3:1:1 run,
    /// and rounding the outer ring changes that run. Swept against this
    /// crate's decoder at corner radii 0.08/0.12/0.16/0.20/0.25/0.30 of the
    /// eye, the results were decodes/FAILS/decodes/decodes/FAILS/FAILS — not a
    /// threshold, just marginal. Marginal is the one thing a PAIRING code must
    /// not be, so the styles differ in their modules, colours, gradient and
    /// backdrop instead. The inner dot is free: it sits inside the 3 of the
    /// run, and `cornerDotType: 'dot'` decodes everywhere.
    pub eye_outer: EyeShape,
    /// `cornerDotType`
    pub eye_dot: EyeShape,
    /// `fgColor`
    pub fg: [u8; 3],
    /// `bgColor`
    pub bg: [u8; 3],
    /// `cornerColor`, and only when `matchCornerColor` is false. `None` means
    /// the eyes take `fg`, which is what matching does.
    pub eye_color: Option<[u8; 3]>,
    /// `useGradient`: the far end of the ramp (`gradientColor`) and
    /// `gradientRotation` in degrees. `None` when `useGradient` is false.
    pub gradient: Option<([u8; 3], f32)>,
    /// `frameShape: 'star'` with `starPlacement: 'behind'`, painted in
    /// `starColor`. Margin only — see the note on scannability above.
    pub star_backdrop: Option<[u8; 3]>,
    /// Corner radius of a rounded eye, as a fraction of its side.
    pub eye_radius: f32,
}

/// The styles, copied field for field from `PRESETS` in design.ts.
///
/// Radial is absent on purpose — see "What is missing" in the module docs.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Classic",
        dot: DotShape::Square,
        eye_outer: EyeShape::Square,
        eye_dot: EyeShape::Square,
        fg: [0x00, 0x00, 0x00],
        bg: [0xff, 0xff, 0xff],
        eye_color: None, // matchCornerColor: true
        gradient: None,
        star_backdrop: None,
        eye_radius: 0.30,
    },
    Preset {
        name: "Rounded",
        dot: DotShape::Rounded,
        eye_outer: EyeShape::Square, // see the note on the field
        eye_dot: EyeShape::Dot,
        fg: [0x0f, 0x17, 0x2a],
        bg: [0xff, 0xff, 0xff],
        eye_color: None,
        gradient: None,
        star_backdrop: None,
        eye_radius: 0.30,
    },
    Preset {
        name: "Dots",
        dot: DotShape::Dot,
        eye_outer: EyeShape::Square, // see the note on the field
        eye_dot: EyeShape::Dot,
        fg: [0x1e, 0x29, 0x3b],
        bg: [0xff, 0xff, 0xff],
        eye_color: Some([0xe0, 0x55, 0x04]), // matchCornerColor: false
        gradient: None,
        star_backdrop: None,
        eye_radius: 0.30,
    },
    Preset {
        name: "Sunset",
        dot: DotShape::ExtraRounded,
        eye_outer: EyeShape::Square, // see the note on the field
        eye_dot: EyeShape::Dot,
        fg: [0xc2, 0x41, 0x0c],
        bg: [0xff, 0xf7, 0xed],
        eye_color: None,
        gradient: Some(([0x9f, 0x12, 0x39], 30.0)),
        star_backdrop: None,
        eye_radius: 0.30,
    },
    Preset {
        name: "Star",
        dot: DotShape::ExtraRounded,
        eye_outer: EyeShape::Square, // see the note on the field
        eye_dot: EyeShape::Dot,
        fg: [0x00, 0x00, 0x00],
        bg: [0xff, 0xff, 0xff],
        eye_color: None,
        gradient: None,
        star_backdrop: Some([0xe0, 0x55, 0x04]),
        eye_radius: 0.30,
    },
];

/// A style picked at random.
///
/// `exclude` is the style showing now, and it is dropped from the pool rather
/// than re-rolled — the same rule `randomPreset` follows in design.ts, for the
/// same reason: with a handful of styles, "random" that sometimes visibly
/// changes nothing reads as a bug rather than as chance.
///
/// Picking from `PRESETS` and nothing else is equally deliberate, and is the
/// other half of the scannability argument: a style generated from scratch —
/// random colours, random shapes — would cheerfully produce a light-on-dark or
/// thin-contrast code. Every entry here is decoded by this crate's tests.
pub fn random_preset(exclude: Option<&str>) -> &'static Preset {
    let pool: Vec<&Preset> = PRESETS.iter().filter(|p| Some(p.name) != exclude).collect();
    let pool = if pool.is_empty() {
        PRESETS.iter().collect()
    } else {
        pool
    };
    // No `rand` dependency for a cosmetic choice: RandomState is seeded by the
    // OS per process, and hashing a fresh counter off it gives a different
    // value each call. Good enough to shuffle a picture; not used for anything
    // that needs to be unguessable.
    let mut h = RandomState::new().build_hasher();
    h.write_usize(pool.len());
    let n = (h.finish() % pool.len() as u64) as usize;
    pool[n]
}

/// A rendered code: `dim` × `dim` pixels, RGBA8.
pub struct Rendered {
    pub dim: u32,
    pub rgba: Vec<u8>,
}

const QUIET: usize = 4;
const SCALE: usize = 8;
/// The code's side, as a fraction of the image, when the star is behind it.
///
/// Copied from `STAR_BEHIND_CODE_SCALE` in frames.ts, where it is bounded by
/// the star's own geometry rather than chosen by eye: the side points sit at
/// x = 0.976 and the bottom two at y = 0.905, so a centred square wider than
/// 0.81 swallows the bottom points and one wider than 0.95 swallows the sides.
/// 0.72 clears the nearest of those by 4.5% of the image — a visible spike
/// rather than a sliver.
const STAR_BEHIND_CODE_SCALE: f32 = 0.72;

/// The star's inner radius as a fraction of its outer one.
///
/// ⚠️ Copied from `STAR_INNER_RATIO` in frames.ts, and the comment there says
/// why it is 0.62 rather than the ~0.38 of a classic thin star: a generous
/// inner radius keeps the shape broad. I first wrote 0.42 here from memory,
/// and the result read as five orange arrowheads rather than as a star —
/// which is exactly the failure this module's header warns about two
/// paragraphs in. Copy values; never invent them.
const STAR_INNER_RATIO: f32 = 0.62;

/// Render `text` as a branded QR in `preset`'s style, with `logo_png`
/// composited over the middle. `None` if the text will not fit in a QR code.
pub fn render(text: &str, preset: &Preset, logo_png: Option<&[u8]>) -> Option<Rendered> {
    // EcLevel::H because the logo covers real modules — same reason design.ts
    // pins ecLevel and refuses to expose it.
    let code =
        qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::H).ok()?;
    let modules = code.width();
    let colors = code.to_colors();
    let dark = |x: usize, y: usize| -> bool {
        x < modules && y < modules && colors[y * modules + x] == qrcode::Color::Dark
    };

    // The code plus its own quiet zone — the white square the star sits
    // behind. Everything beyond it is the star's to show in.
    let code_square = ((modules + QUIET * 2) * SCALE) as u32;
    let dim = if preset.star_backdrop.is_some() {
        (code_square as f32 / STAR_BEHIND_CODE_SCALE).round() as u32
    } else {
        code_square
    };
    let mut rgba = vec![0u8; (dim * dim * 4) as usize];

    // Background first, everywhere.
    for px in rgba.chunks_exact_mut(4) {
        px[0] = preset.bg[0];
        px[1] = preset.bg[1];
        px[2] = preset.bg[2];
        px[3] = 255;
    }

    if let Some(star) = preset.star_backdrop {
        paint_star(&mut rgba, dim, star);
        // ⚠️ Then put the quiet zone back. A QR's quiet zone must be LIGHT —
        // it is how a decoder knows where the code ends — and the star is a
        // mid-tone (orange is luma ~117 against 0 and 255). Painting it across
        // the quiet zone made the Star style undecodable; measured, not
        // guessed. Repainting the code plus its full 4-module margin in the
        // background colour leaves the star showing only OUTSIDE that, which
        // is what 'behind' means in design.ts anyway: a backdrop with its
        // points around the code, never under it.
        let lo = (dim - code_square) / 2;
        let hi = lo + code_square;
        for y in lo..hi {
            for x in lo..hi {
                let i = ((y * dim + x) * 4) as usize;
                rgba[i] = preset.bg[0];
                rgba[i + 1] = preset.bg[1];
                rgba[i + 2] = preset.bg[2];
                rgba[i + 3] = 255;
            }
        }
    }

    // Centre the modules in whatever canvas we ended up with.
    let origin = (dim as f32 - (modules * SCALE) as f32) / 2.0;
    let span = (modules * SCALE) as f32;

    // The three finder patterns, drawn as whole shapes rather than as modules,
    // so `cornerSquareType` / `cornerDotType` mean something.
    let eyes = [(0usize, 0usize), (modules - 7, 0), (0, modules - 7)];
    let eye_rgb = preset.eye_color.unwrap_or(preset.fg);
    for (ex, ey) in eyes {
        let x0 = origin + (ex * SCALE) as f32;
        let y0 = origin + (ey * SCALE) as f32;
        let s = (7 * SCALE) as f32;
        // Outer ring: a 7x7 frame one module thick.
        fill_shape(
            &mut rgba,
            dim,
            x0,
            y0,
            s,
            preset.eye_outer_shape(),
            eye_rgb,
            Some(s - 2.0 * SCALE as f32),
        );
        // Centre: 3x3.
        let c = SCALE as f32 * 2.0;
        fill_shape(
            &mut rgba,
            dim,
            x0 + c,
            y0 + c,
            (3 * SCALE) as f32,
            preset.eye_dot_shape(),
            eye_rgb,
            None,
        );
    }

    for my in 0..modules {
        for mx in 0..modules {
            if !dark(mx, my) || in_eye(mx, my, modules) {
                continue;
            }
            let x0 = origin + (mx * SCALE) as f32;
            let y0 = origin + (my * SCALE) as f32;
            let rgb = match preset.gradient {
                // A linear ramp across the code at `deg`, matching
                // gradientRotation. Computed per module, not per pixel: a
                // module is one flat colour, which is what qr-code-styling
                // does and what keeps each module's edge crisp for a decoder.
                Some((to, deg)) => {
                    let a = deg.to_radians();
                    let (cx, cy) = (x0 + SCALE as f32 / 2.0, y0 + SCALE as f32 / 2.0);
                    let u = ((cx - origin) * a.cos() + (cy - origin) * a.sin()) / span;
                    lerp(preset.fg, to, u.clamp(0.0, 1.0))
                }
                None => preset.fg,
            };
            // A corner is rounded only when both modules it lies between
            // are light; otherwise the run must stay joined.
            let (l, u, r_, d) = (
                mx > 0 && dark(mx - 1, my),
                my > 0 && dark(mx, my - 1),
                dark(mx + 1, my),
                dark(mx, my + 1),
            );
            let corners = [!l && !u, !r_ && !u, !r_ && !d, !l && !d];
            fill_shape(
                &mut rgba,
                dim,
                x0,
                y0,
                SCALE as f32,
                preset.dot_shape(corners),
                rgb,
                None,
            );
        }
    }

    if let Some(png) = logo_png {
        overlay_logo(&mut rgba, dim, span, png, preset.bg);
    }
    Some(Rendered { dim, rgba })
}

impl Preset {
    /// `corners` says which of this module's corners are free to be rounded:
    /// a corner is rounded only where BOTH the modules it sits between are
    /// light, so a run stays one unbroken bar. See `Shape::RoundRect`.
    fn dot_shape(&self, corners: [bool; 4]) -> Shape {
        match self.dot {
            DotShape::Square => Shape::Square,
            // qr-code-styling's 'rounded' is a gentle corner, 'extra-rounded'
            // a strong one. Expressed here as a fraction of the module.
            DotShape::Rounded => Shape::RoundRect(0.25, corners),
            DotShape::ExtraRounded => Shape::RoundRect(0.45, corners),
            DotShape::Dot => Shape::Circle,
        }
    }
    fn eye_outer_shape(&self) -> Shape {
        match self.eye_outer {
            EyeShape::Square => Shape::Square,
            EyeShape::Rounded => Shape::RoundRect(self.eye_radius, [true; 4]),
            EyeShape::Dot => Shape::Circle,
        }
    }
    fn eye_dot_shape(&self) -> Shape {
        match self.eye_dot {
            EyeShape::Square => Shape::Square,
            EyeShape::Rounded => Shape::RoundRect(0.30, [true; 4]),
            EyeShape::Dot => Shape::Circle,
        }
    }
}

#[derive(Clone, Copy)]
enum Shape {
    Square,
    /// Corner radius as a fraction of the side, and WHICH corners get it, in
    /// the order top-left, top-right, bottom-right, bottom-left.
    ///
    /// ⚠️ Per-corner, not per-module, and that is the whole point. Rounding
    /// every module independently turns a run of dark modules into a string of
    /// separated lozenges: it looks nothing like `qr-code-styling`'s 'rounded'
    /// (which keeps a run as one continuous bar) and it throws away ink a
    /// decoder needs. Measured, not guessed — with naive rounding only the
    /// square style decoded at all; see `every_preset_decodes`.
    RoundRect(f32, [bool; 4]),
    Circle,
}

/// Is this module part of a finder pattern (which is drawn as a whole shape)?
fn in_eye(x: usize, y: usize, modules: usize) -> bool {
    (y < 7 && (x < 7 || x >= modules - 7)) || (x < 7 && y >= modules - 7)
}

fn lerp(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let m = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    [m(0), m(1), m(2)]
}

/// Paint `shape` of side `s` at (`x0`,`y0`) in `rgb`, antialiased.
///
/// `hollow` makes it a ring: the same shape of that side, centred, is left
/// untouched. That is how a finder pattern's outer frame is drawn.
#[allow(clippy::too_many_arguments)] // a painter's arguments: canvas, rect, shape, ink
fn fill_shape(
    rgba: &mut [u8],
    dim: u32,
    x0: f32,
    y0: f32,
    s: f32,
    shape: Shape,
    rgb: [u8; 3],
    hollow: Option<f32>,
) {
    const K: u32 = 3; // supersampling, as the avatar painter does
    let (ix0, iy0) = (x0.floor() as i64, y0.floor() as i64);
    let n = s.ceil() as i64;
    for py in iy0..iy0 + n {
        for px in ix0..ix0 + n {
            if px < 0 || py < 0 || px >= dim as i64 || py >= dim as i64 {
                continue;
            }
            let mut hits = 0u32;
            for sy in 0..K {
                for sx in 0..K {
                    let fx = px as f32 + (sx as f32 + 0.5) / K as f32;
                    let fy = py as f32 + (sy as f32 + 0.5) / K as f32;
                    if !inside(fx, fy, x0, y0, s, shape) {
                        continue;
                    }
                    if let Some(h) = hollow {
                        let off = (s - h) / 2.0;
                        if inside(fx, fy, x0 + off, y0 + off, h, shape) {
                            continue;
                        }
                    }
                    hits += 1;
                }
            }
            if hits == 0 {
                continue;
            }
            let a = hits as f32 / (K * K) as f32;
            let i = ((py as u32 * dim + px as u32) * 4) as usize;
            for c in 0..3 {
                let base = rgba[i + c] as f32;
                rgba[i + c] = (base + (rgb[c] as f32 - base) * a).round() as u8;
            }
        }
    }
}

fn inside(fx: f32, fy: f32, x0: f32, y0: f32, s: f32, shape: Shape) -> bool {
    let (lx, ly) = (fx - x0, fy - y0);
    if lx < 0.0 || ly < 0.0 || lx > s || ly > s {
        return false;
    }
    match shape {
        Shape::Square => true,
        Shape::Circle => {
            let r = s / 2.0;
            ((lx - r).powi(2) + (ly - r).powi(2)).sqrt() <= r
        }
        Shape::RoundRect(frac, corners) => {
            let r = (s * frac).min(s / 2.0);
            // Only a corner that is actually rounded cuts anything away.
            let (near_l, near_t) = (lx < r, ly < r);
            let (near_r, near_b) = (lx > s - r, ly > s - r);
            let (cut, cx, cy) = match (near_l, near_t, near_r, near_b) {
                (true, true, _, _) => (corners[0], r, r),
                (_, true, true, _) => (corners[1], s - r, r),
                (_, _, true, true) => (corners[2], s - r, s - r),
                (true, _, _, true) => (corners[3], r, s - r),
                _ => (false, 0.0, 0.0),
            };
            !cut || ((lx - cx).powi(2) + (ly - cy).powi(2)).sqrt() <= r
        }
    }
}

/// A five-pointed star filling the canvas, behind everything.
///
/// `starPlacement: 'behind'` in design.ts: the star is a backdrop with its
/// points showing around the code, not a plate the code is cut into. It is
/// painted before any module and sized so its points live in the quiet zone
/// the code square leaves — no module is ever drawn on top of it, which keeps
/// this style as readable as the others.
fn paint_star(rgba: &mut [u8], dim: u32, rgb: [u8; 3]) {
    let c = dim as f32 / 2.0;
    let outer = dim as f32 * 0.5;
    let inner = outer * STAR_INNER_RATIO;
    let mut pts = [(0.0f32, 0.0f32); 10];
    for (i, p) in pts.iter_mut().enumerate() {
        let r = if i % 2 == 0 { outer } else { inner };
        // Start at twelve o'clock so the star sits upright.
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * (i as f32) / 5.0;
        *p = (c + r * a.cos(), c + r * a.sin());
    }
    const K: u32 = 3;
    for py in 0..dim {
        for px in 0..dim {
            let mut hits = 0u32;
            for sy in 0..K {
                for sx in 0..K {
                    let fx = px as f32 + (sx as f32 + 0.5) / K as f32;
                    let fy = py as f32 + (sy as f32 + 0.5) / K as f32;
                    if point_in_poly(fx, fy, &pts) {
                        hits += 1;
                    }
                }
            }
            if hits == 0 {
                continue;
            }
            let a = hits as f32 / (K * K) as f32;
            let i = ((py * dim + px) * 4) as usize;
            for ch in 0..3 {
                let base = rgba[i + ch] as f32;
                rgba[i + ch] = (base + (rgb[ch] as f32 - base) * a).round() as u8;
            }
        }
    }
}

fn point_in_poly(x: f32, y: f32, pts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Composite the centre logo over a pad of background, clearing the modules
/// behind it. EcLevel H is what makes this safe.
fn overlay_logo(rgba: &mut [u8], dim: u32, span: f32, png: &[u8], bg: [u8; 3]) {
    // ⚠️ Sized against the CODE, not the canvas. Sizing it off `dim` looks
    // identical for every square style and then quietly breaks the moment a
    // style adds margin: Star widens the quiet zone for its points, so the
    // same fraction of a bigger canvas ate enough extra modules to stop it
    // decoding, while every other style was fine. Caught by the logo test, not
    // by eye. `span` is the code itself, which is what logoSize means in
    // design.ts.
    let target = (span * 0.25).round() as u32;
    let Ok(img) = image::load_from_memory(png) else {
        return;
    };
    let logo = img
        .resize_exact(target, target, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let pad = (span * 0.035).round() as i64;
    let x0 = ((dim - target) / 2) as i64;
    let y0 = ((dim - target) / 2) as i64;

    // The pad: background, so no module shows through behind the mark.
    for py in (y0 - pad)..(y0 + target as i64 + pad) {
        for px in (x0 - pad)..(x0 + target as i64 + pad) {
            if px < 0 || py < 0 || px >= dim as i64 || py >= dim as i64 {
                continue;
            }
            let i = ((py as u32 * dim + px as u32) * 4) as usize;
            rgba[i] = bg[0];
            rgba[i + 1] = bg[1];
            rgba[i + 2] = bg[2];
            rgba[i + 3] = 255;
        }
    }
    for (lx, ly, p) in logo.enumerate_pixels() {
        let px = x0 + lx as i64;
        let py = y0 + ly as i64;
        if px < 0 || py < 0 || px >= dim as i64 || py >= dim as i64 {
            continue;
        }
        let a = p.0[3] as f32 / 255.0;
        if a <= 0.0 {
            continue;
        }
        let i = ((py as u32 * dim + px as u32) * 4) as usize;
        for c in 0..3 {
            let base = rgba[i + c] as f32;
            rgba[i + c] = (base + (p.0[c] as f32 - base) * a).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str =
        "https://opensource.unisim.co.uk/screens/connect?host=10.229.171.35&port=9000&pin=4821";

    /// Several payloads, because one proves very little: the module pattern
    /// differs for every string and for every size band the encoder picks, so
    /// a style that decodes for exactly one of them is marginal rather than
    /// working. The long one carries Wi-Fi details, the biggest payload the
    /// host ever shows.
    const PAYLOADS: &[&str] = &[
        "https://unisim.co.uk",
        PAYLOAD,
        "https://opensource.unisim.co.uk/screens/connect?host=192.168.1.144&port=9000&pin=0042#ssid=A%20Rather%20Long%20Network%20Name&auth=WPA&pass=correct-horse-battery-staple-9271",
    ];

    fn decode(r: &Rendered) -> Option<String> {
        let mut gray = image::GrayImage::new(r.dim, r.dim);
        for (i, px) in r.rgba.chunks_exact(4).enumerate() {
            let l = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as u8;
            gray.put_pixel(i as u32 % r.dim, i as u32 / r.dim, image::Luma([l]));
        }
        let mut img = rqrr::PreparedImage::prepare(gray);
        let grids = img.detect_grids();
        grids.first().and_then(|g| g.decode().ok()).map(|(_, s)| s)
    }

    /// The one that matters. A style is chosen at RANDOM for a code the user
    /// has to scan to pair a phone, so an unreadable style is not a cosmetic
    /// bug — it is a dead end with no fallback but typing an address and PIN.
    /// Universal QR's presets were decoded through ITS pipeline; this is a
    /// different renderer, so the guarantee has to be re-earned here.
    #[test]
    fn every_preset_decodes() {
        // Collect, don't stop at the first: which styles fail is the useful
        // answer, and asserting inside the loop hides every one after it.
        let mut failed = Vec::new();
        for preset in PRESETS {
            for payload in PAYLOADS {
                let r = render(payload, preset, None).expect("render");
                if decode(&r).as_deref() != Some(*payload) {
                    failed.push(format!(
                        "{} at {}px, {} chars",
                        preset.name,
                        r.dim,
                        payload.len()
                    ));
                }
                // QR_DUMP=<dir> writes each rendered style out to look at.
                if let Some(dir) = std::env::var_os("QR_DUMP") {
                    let mut img = image::RgbaImage::new(r.dim, r.dim);
                    img.copy_from_slice(&r.rgba);
                    let path = std::path::Path::new(&dir).join(format!("qr-{}.png", preset.name));
                    img.save(&path)
                        .unwrap_or_else(|e| panic!("dump {path:?}: {e}"));
                }
            }
        }
        assert!(failed.is_empty(), "these styles did not decode: {failed:?}");
    }

    /// And with the centre logo covering modules, which is the real picture.
    #[test]
    fn every_preset_decodes_with_the_centre_logo() {
        // A plain opaque square is harsher than the real mark: it has no
        // transparent edge, so it covers every pixel of the pad.
        let mut logo = image::RgbaImage::new(96, 96);
        for p in logo.pixels_mut() {
            *p = image::Rgba([0x0d, 0x60, 0x66, 255]);
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(logo)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode");
        let png = png.into_inner();

        let mut failed = Vec::new();
        for preset in PRESETS {
            for payload in PAYLOADS {
                let r = render(payload, preset, Some(&png)).expect("render");
                if decode(&r).as_deref() != Some(*payload) {
                    failed.push(format!("{} ({} chars)", preset.name, payload.len()));
                }
            }
        }
        assert!(
            failed.is_empty(),
            "these did not decode under a logo: {failed:?}"
        );
    }

    /// `random_preset` must never hand back the style already on screen, or a
    /// regenerate that visibly does nothing reads as a broken button.
    #[test]
    fn random_never_repeats_the_excluded_style() {
        for preset in PRESETS {
            for _ in 0..64 {
                assert_ne!(random_preset(Some(preset.name)).name, preset.name);
            }
        }
    }

    /// And it must actually vary. A hasher that returned a constant would pass
    /// every other test here.
    #[test]
    fn random_reaches_every_style() {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..600 {
            seen.insert(random_preset(None).name);
        }
        assert_eq!(seen.len(), PRESETS.len(), "only ever saw {seen:?}");
    }

    /// Excluding a name that is not a style must not empty the pool.
    #[test]
    fn an_unknown_exclusion_still_returns_a_style() {
        assert!(!random_preset(Some("Radial")).name.is_empty());
    }

    /// The star is a backdrop, not a plate: it must widen the quiet zone
    /// rather than crowd the code, or it would be eating modules.
    #[test]
    fn the_star_only_uses_the_margin() {
        let star = PRESETS.iter().find(|p| p.name == "Star").unwrap();
        let plain = PRESETS.iter().find(|p| p.name == "Classic").unwrap();
        let s = render(PAYLOAD, star, None).expect("star");
        let c = render(PAYLOAD, plain, None).expect("classic");
        assert!(
            s.dim > c.dim,
            "the star must add margin for its points ({} vs {})",
            s.dim,
            c.dim,
        );
    }
}
