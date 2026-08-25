//! X11 screen capture — Stage 2 of `docs/LINUX-HOST.md`.
//!
//! Produces the same two things the Windows host's `snapshot.rs` does, with the
//! same signatures, so the shared consumers (slide previews now, `stream.rs`
//! later) need no Linux-specific code: a top-down BGRA buffer, and a downscaled
//! JPEG of the primary screen.
//!
//! **X11 only, deliberately.** Wayland capture is Stage 3 — a different job
//! (`ashpd` portal + PipeWire, plus a consent dialog each session), not a
//! variation on this one. Under XWayland `DISPLAY` is set and a connection
//! succeeds, but the X root window contains only X clients, so a native Wayland
//! window captures as background: see [`Backend::describe`], which says so out
//! loud rather than shipping a black rectangle.
//!
//! ## Why `x11rb`, and why no C library is linked
//!
//! `x11rb` speaks the X protocol over the socket itself, so there is no
//! `libX11`/`libxcb` to find at runtime — measured: `ldd` on the built binary
//! lists `libgcc_s`, `libc` and the loader, nothing else. That matters here for
//! the same reason the `native-tls` → rustls swap did: an AppImage carries no
//! distro packages with it.
//!
//! ## Why both MIT-SHM and `GetImage`
//!
//! Measured on this workspace's only Linux (a container running Xvfb at
//! 1920×1080): **`GetImage` 10.7 ms/frame, MIT-SHM 0.93 ms/frame** — 11× — and
//! **SHM needs neither `x11rb/allow-unsafe-code` nor `libxcb1-dev`**, which was
//! the reason to think it might not be worth having. So SHM is preferred.
//!
//! ⚠️ The fallback is not belt-and-braces: **shared memory requires the X server
//! to be on this machine**, so a remote display (`ssh -X`, X over the network)
//! has no SHM extension or fails to attach. `GetImage` is what makes those work
//! at all, and at 10.7 ms it is far inside a slide preview's budget anyway.

use std::sync::{Mutex, OnceLock};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageBuffer, Rgb};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shm::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat, ImageOrder, Screen, Setup, Window};
use x11rb::rust_connection::RustConnection;

/// Which grab path a live capturer settled on, for the log line and the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// MIT-SHM: the server writes into a shared segment; no pixels cross the socket.
    Shm,
    /// Plain `GetImage`: the whole frame comes back over the socket.
    GetImage,
}

impl Backend {
    /// A one-line human description, including the XWayland caveat when it applies.
    pub fn describe(self) -> String {
        let path = match self {
            Self::Shm => "X11 (MIT-SHM)",
            Self::GetImage => "X11 (GetImage — no shared memory; a remote display?)",
        };
        if is_wayland_session() {
            format!("{path} via XWayland — native Wayland windows will NOT appear")
        } else {
            path.to_owned()
        }
    }
}

/// True when the session is Wayland, so any X11 capture is going through
/// XWayland and cannot see native Wayland clients. Detected the way toolkits do,
/// and *not* by the absence of `DISPLAY` — XWayland sets that too.
pub fn is_wayland_session() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t.eq_ignore_ascii_case("wayland"))
        || std::env::var("WAYLAND_DISPLAY").is_ok_and(|d| !d.is_empty())
}

/// How to turn one stored pixel into R, G and B, read from the server's own
/// declared format rather than assumed.
///
/// ⚠️ The near-universal desktop layout is little-endian 32-bpp with
/// `0x00FF0000`/`0x0000FF00`/`0x000000FF` masks — i.e. the bytes are already
/// B, G, R, X and the "conversion" is a copy. Assuming it is still wrong: the
/// masks are a property of the *visual*, so a 16-bit visual or a big-endian
/// server would silently produce colour-swapped or darkened screenshots. That is
/// precisely the class of bug a synthetic test never catches, because such a
/// test generates its pixels through the same assumption it is checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelLayout {
    /// Bytes per stored pixel (4 for the usual depth-24-in-32 case).
    pub bytes_per_pixel: usize,
    /// Bits a row is padded up to.
    pub scanline_pad: usize,
    /// True when the server sends least-significant byte first.
    pub lsb_first: bool,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
}

impl PixelLayout {
    /// Derive the layout for `screen`'s root visual from the connection setup.
    /// Returns `None` if the server declares no pixmap format for that depth, or
    /// the root visual is a palette one (no desktop uses those).
    pub(crate) fn from_setup(setup: &Setup, screen: &Screen) -> Option<Self> {
        let fmt = setup.pixmap_formats.iter().find(|f| f.depth == screen.root_depth)?;
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|d| &d.visuals)
            .find(|v| v.visual_id == screen.root_visual)?;
        if visual.red_mask == 0 || visual.green_mask == 0 || visual.blue_mask == 0 {
            return None;
        }
        Some(Self {
            bytes_per_pixel: usize::from(fmt.bits_per_pixel) / 8,
            scanline_pad: usize::from(fmt.scanline_pad),
            lsb_first: setup.image_byte_order == ImageOrder::LSB_FIRST,
            red_mask: visual.red_mask,
            green_mask: visual.green_mask,
            blue_mask: visual.blue_mask,
        })
    }

    /// Bytes per row, including the server's scanline padding.
    pub(crate) fn stride(&self, width: usize) -> usize {
        let pad = self.scanline_pad.max(8) / 8;
        let raw = width * self.bytes_per_pixel;
        raw.div_ceil(pad) * pad
    }

    /// True for the layout virtually every desktop actually uses, where stored
    /// bytes are already B, G, R (, X) and no per-pixel arithmetic is needed.
    pub(crate) fn is_native_bgrx(&self) -> bool {
        self.bytes_per_pixel == 4
            && self.lsb_first
            && self.red_mask == 0x00FF_0000
            && self.green_mask == 0x0000_FF00
            && self.blue_mask == 0x0000_00FF
    }

    /// Repack a server image into tightly packed top-down BGRA.
    pub(crate) fn to_bgra(self, data: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
        let stride = self.stride(width);
        if self.bytes_per_pixel == 0 || data.len() < stride * height {
            return None;
        }
        let (rs, gs, bs) = (
            self.red_mask.trailing_zeros(),
            self.green_mask.trailing_zeros(),
            self.blue_mask.trailing_zeros(),
        );
        // ⚠️ Masks narrower than 8 bits (a 16-bit visual) must be *scaled* up,
        // not just shifted, or every colour comes out dark: 5 bits of full red
        // is 31, and 31 as an 8-bit channel is almost black.
        let (rw, gw, bw) = (
            self.red_mask.count_ones(),
            self.green_mask.count_ones(),
            self.blue_mask.count_ones(),
        );
        let mut out = vec![0u8; width * height * 4];
        for y in 0..height {
            let row = &data[y * stride..y * stride + width * self.bytes_per_pixel];
            for x in 0..width {
                let px = &row[x * self.bytes_per_pixel..(x + 1) * self.bytes_per_pixel];
                let raw = read_pixel(px, self.lsb_first);
                let o = (y * width + x) * 4;
                out[o] = scale_to_8((raw & self.blue_mask) >> bs, bw);
                out[o + 1] = scale_to_8((raw & self.green_mask) >> gs, gw);
                out[o + 2] = scale_to_8((raw & self.red_mask) >> rs, rw);
                out[o + 3] = 0xFF;
            }
        }
        Some(out)
    }
}

/// Read one stored pixel (1–4 bytes) as a `u32` in the server's byte order.
fn read_pixel(bytes: &[u8], lsb_first: bool) -> u32 {
    let mut v = 0u32;
    if lsb_first {
        for (i, b) in bytes.iter().take(4).enumerate() {
            v |= u32::from(*b) << (8 * i);
        }
    } else {
        for b in bytes.iter().take(4) {
            v = (v << 8) | u32::from(*b);
        }
    }
    v
}

/// Widen an `n`-bit channel value to a full 8 bits (5-bit `0b11111` → 255, not 31).
pub(crate) fn scale_to_8(value: u32, width: u32) -> u8 {
    match width {
        0 => 0,
        w if w >= 8 => u8::try_from(value & 0xFF).unwrap_or(0),
        w => {
            let max = (1u32 << w) - 1;
            u8::try_from((value.min(max) * 255).div_ceil(max)).unwrap_or(255)
        }
    }
}

/// A live connection to the X server, plus whichever grab path it settled on.
pub struct Capturer {
    conn: RustConnection,
    root: Window,
    layout: PixelLayout,
    /// `Some` while an attached SHM segment is usable.
    shm: Option<ShmSegment>,
    backend: Backend,
}

/// An attached System-V shared memory segment the X server writes frames into.
struct ShmSegment {
    seg: shm::Seg,
    shmid: i32,
    addr: *mut u8,
    len: usize,
}

// The pointer is only read, from whichever thread holds the `Capturer`, which
// itself lives behind a `Mutex`.
unsafe impl Send for ShmSegment {}

impl Drop for ShmSegment {
    fn drop(&mut self) {
        unsafe {
            libc::shmdt(self.addr.cast());
            libc::shmctl(self.shmid, libc::IPC_RMID, std::ptr::null_mut());
        }
    }
}

impl Capturer {
    /// Connect to the X server named by `DISPLAY` and pick a grab path,
    /// preferring MIT-SHM unless `SCREENS_X11_NO_SHM` is set.
    ///
    /// # Errors
    /// Returns a human-readable reason if there is no X server to talk to (a
    /// pure Wayland session with no XWayland, or a headless machine), or if the
    /// root visual has no usable colour masks.
    pub fn new() -> Result<Self, String> {
        Self::open(!std::env::var("SCREENS_X11_NO_SHM").is_ok_and(|v| v != "0"))
    }

    /// As [`Capturer::new`], but says outright whether to try shared memory.
    ///
    /// The flag exists because the two paths must be *cross-checked*, not just
    /// benchmarked: the first version of this module measured SHM at 11× and
    /// never compared its pixels against `GetImage`'s, which is how a wrong
    /// frame would have shipped looking like a fast one.
    ///
    /// # Errors
    /// As [`Capturer::new`].
    pub fn open(prefer_shm: bool) -> Result<Self, String> {
        let (conn, screen_num) =
            x11rb::connect(None).map_err(|e| format!("cannot connect to an X server: {e}"))?;
        let setup = conn.setup().clone();
        let screen = setup.roots[screen_num].clone();
        let layout = PixelLayout::from_setup(&setup, &screen).ok_or_else(|| {
            "the X server reports no true-colour format for the root visual".to_owned()
        })?;
        let root = screen.root;

        let mut me = Self { conn, root, layout, shm: None, backend: Backend::GetImage };
        // Size the segment from the screen as the server reports it now; a later
        // resolution change reallocates (see `grab_bgra`).
        let want = usize::from(screen.width_in_pixels) * usize::from(screen.height_in_pixels) * 4;
        if prefer_shm && me.try_attach_shm(want) {
            me.backend = Backend::Shm;
        }
        Ok(me)
    }

    /// Which path this capturer is using.
    pub fn backend(&self) -> Backend {
        self.backend
    }

    /// Attach a SHM segment of at least `len` bytes, replacing any existing one.
    /// Returns false — leaving the capturer on `GetImage` — for any reason at
    /// all, since every one of them is a working configuration, just a slower one.
    fn try_attach_shm(&mut self, len: usize) -> bool {
        self.shm = None; // detach the old one first; Drop does the cleanup
        if self.conn.extension_information(shm::X11_EXTENSION_NAME).ok().flatten().is_none() {
            return false;
        }
        // Round up so a small screen change doesn't reallocate every frame.
        let len = len.next_multiple_of(1 << 20);
        let shmid = unsafe { libc::shmget(libc::IPC_PRIVATE, len, libc::IPC_CREAT | 0o600) };
        if shmid < 0 {
            return false;
        }
        let raw = unsafe { libc::shmat(shmid, std::ptr::null(), 0) };
        if raw == usize::MAX as *mut libc::c_void {
            unsafe { libc::shmctl(shmid, libc::IPC_RMID, std::ptr::null_mut()) };
            return false;
        }
        let addr = raw.cast::<u8>();
        let cleanup = || unsafe {
            libc::shmdt(raw);
            libc::shmctl(shmid, libc::IPC_RMID, std::ptr::null_mut());
        };
        let Ok(seg) = self.conn.generate_id() else {
            cleanup();
            return false;
        };
        // ⚠️ `.check()` matters. Attach failure is exactly what a *remote* X
        // server reports, and without waiting for that reply the first grab
        // would read an unwritten segment — a black screenshot rather than an
        // error, which is the silent-wrong-answer failure this whole module is
        // written to avoid.
        let attached = self
            .conn
            .shm_attach(seg, u32::try_from(shmid).unwrap_or(0), false)
            .ok()
            .and_then(|c| c.check().ok())
            .is_some();
        if !attached {
            cleanup();
            return false;
        }
        self.shm = Some(ShmSegment { seg, shmid, addr, len });
        true
    }

    /// Capture the whole root window into tightly packed top-down BGRA.
    ///
    /// Returns `(width, height, bytes)`. The geometry is re-read every grab, so
    /// an `xrandr` resolution change mid-session produces a correctly sized
    /// frame rather than a torn one.
    pub fn grab_bgra(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        let geom = self.conn.get_geometry(self.root).ok()?.reply().ok()?;
        let (w, h) = (geom.width, geom.height);
        if w == 0 || h == 0 {
            return None;
        }
        let (uw, uh) = (usize::from(w), usize::from(h));
        let needed = self.layout.stride(uw) * uh;

        if self.shm.as_ref().is_some_and(|s| s.len < needed) {
            let ok = self.try_attach_shm(needed);
            self.backend = if ok { Backend::Shm } else { Backend::GetImage };
        }
        if let Some(seg) = self.shm.as_ref() {
            let got = self
                .conn
                .shm_get_image(self.root, 0, 0, w, h, !0, ImageFormat::Z_PIXMAP.into(), seg.seg, 0)
                .ok()
                .and_then(|c| c.reply().ok());
            if got.is_some() {
                // SAFETY: the server has just finished writing `needed` bytes
                // into a segment of at least that size (checked above), and
                // nothing else writes to it while this capturer holds it.
                let data = unsafe { std::slice::from_raw_parts(seg.addr, needed) };
                return self.pack(data, uw, uh).map(|b| (u32::from(w), u32::from(h), b));
            }
            // A grab that fails after a successful attach (the server dropped
            // the segment) is not fatal — fall to GetImage for good.
            self.shm = None;
            self.backend = Backend::GetImage;
        }

        let reply = self
            .conn
            .get_image(ImageFormat::Z_PIXMAP, self.root, 0, 0, w, h, !0)
            .ok()?
            .reply()
            .ok()?;
        self.pack(&reply.data, uw, uh).map(|b| (u32::from(w), u32::from(h), b))
    }

    /// Server bytes → tightly packed BGRA, taking the copy-only fast path when
    /// the layout is the usual little-endian BGRX (see [`PixelLayout`]).
    fn pack(&self, data: &[u8], w: usize, h: usize) -> Option<Vec<u8>> {
        if !self.layout.is_native_bgrx() {
            return self.layout.to_bgra(data, w, h);
        }
        let stride = self.layout.stride(w);
        if data.len() < stride * h {
            return None;
        }
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            let src = &data[y * stride..y * stride + w * 4];
            let dst = &mut out[y * w * 4..(y + 1) * w * 4];
            dst.copy_from_slice(src);
            // X leaves the 4th byte undefined at depth 24. JPEG ignores it, but
            // the mirror encoder will not, so make it opaque now.
            for alpha in dst.iter_mut().skip(3).step_by(4) {
                *alpha = 0xFF;
            }
        }
        Some(out)
    }
}

/// The process-wide capturer, opened on first use.
///
/// One connection is reused across grabs: opening an X connection costs a
/// handshake and several round trips, which would dwarf the 1 ms grab it exists
/// to serve. An `Err` is cached too, so a machine with no X server explains
/// itself once rather than on every page turn.
static CAPTURER: OnceLock<Mutex<Result<Capturer, String>>> = OnceLock::new();

fn with_capturer<T>(f: impl FnOnce(&mut Capturer) -> Option<T>) -> Option<T> {
    let cell = CAPTURER.get_or_init(|| Mutex::new(Capturer::new()));
    let mut guard = cell.lock().ok()?;
    f(guard.as_mut().ok()?)
}

/// Which backend is in use, or why capture is unavailable — for the startup log
/// and the GUI, in the same "say it once, up front" spirit as the uinput check.
///
/// # Errors
/// Returns the reason capture is off, phrased for a person rather than a log.
pub fn status() -> Result<String, String> {
    let cell = CAPTURER.get_or_init(|| Mutex::new(Capturer::new()));
    let guard = cell.lock().map_err(|_| "capture state poisoned".to_owned())?;
    match guard.as_ref() {
        Ok(c) => Ok(c.backend().describe()),
        Err(e) if is_wayland_session() => Err(format!(
            "{e} — this is a Wayland session, where capture is Stage 3 of \
             docs/LINUX-HOST.md; slide previews are off"
        )),
        Err(e) => Err(format!("{e} — slide previews are off")),
    }
}

/// True when slide previews can actually be produced on this machine.
pub fn is_available() -> bool {
    status().is_ok()
}

/// Capture the primary display into top-down BGRA. Mirrors the Windows host's
/// `snapshot::grab_primary_bgra`, so `stream.rs` can consume either.
pub fn grab_primary_bgra() -> Option<(u32, u32, Vec<u8>)> {
    with_capturer(Capturer::grab_bgra)
}

/// Capture the primary display, downscale so its longest side is at most
/// `max_dim` px, and JPEG-encode at `quality`. Identical signature and semantics
/// to the Windows and macOS hosts' `capture_primary_jpeg`.
pub fn capture_primary_jpeg(max_dim: u32, quality: u8) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h, bgra) = grab_primary_bgra()?;
    bgra_to_jpeg(w, h, &bgra, max_dim, quality)
}

/// BGRA → downscaled JPEG. Split out from the grab so it can be exercised on
/// plain bytes with no X server present.
pub(crate) fn bgra_to_jpeg(
    w: u32,
    h: u32,
    bgra: &[u8],
    max_dim: u32,
    quality: u8,
) -> Option<(u32, u32, Vec<u8>)> {
    if w == 0 || h == 0 || bgra.len() < (w as usize) * (h as usize) * 4 {
        return None;
    }
    let mut rgb: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(w, h);
    for (i, px) in rgb.pixels_mut().enumerate() {
        let o = i * 4;
        *px = Rgb([bgra[o + 2], bgra[o + 1], bgra[o]]);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout every real desktop reports, for the tests below.
    fn bgrx() -> PixelLayout {
        PixelLayout {
            bytes_per_pixel: 4,
            scanline_pad: 32,
            lsb_first: true,
            red_mask: 0x00FF_0000,
            green_mask: 0x0000_FF00,
            blue_mask: 0x0000_00FF,
        }
    }

    #[test]
    fn native_bgrx_is_recognised_and_anything_else_is_not() {
        assert!(bgrx().is_native_bgrx());
        // A big-endian server sends the same masks with the bytes reversed.
        assert!(!PixelLayout { lsb_first: false, ..bgrx() }.is_native_bgrx());
        // An RGBX server (masks swapped) must NOT take the copy fast path.
        assert!(!PixelLayout { red_mask: 0x0000_00FF, blue_mask: 0x00FF_0000, ..bgrx() }
            .is_native_bgrx());
        // 16-bit visual.
        assert!(!PixelLayout { bytes_per_pixel: 2, ..bgrx() }.is_native_bgrx());
    }

    #[test]
    fn stride_rounds_up_to_the_scanline_pad() {
        // 32bpp is always already aligned, whatever the width.
        assert_eq!(bgrx().stride(1919), 1919 * 4);
        // 24bpp packed is the case that actually pads: 3 bytes * 5 px = 15 -> 16.
        let packed = PixelLayout { bytes_per_pixel: 3, ..bgrx() };
        assert_eq!(packed.stride(5), 16);
        assert_eq!(packed.stride(8), 24); // exactly aligned, no padding added
    }

    /// ⚠️ The bug this guards: a "conversion" that only shifts. Full red in a
    /// 5-bit channel is 31, and 31 in an 8-bit channel is near black.
    #[test]
    fn narrow_channels_are_scaled_not_just_shifted() {
        assert_eq!(scale_to_8(31, 5), 255);
        assert_eq!(scale_to_8(0, 5), 0);
        assert_eq!(scale_to_8(63, 6), 255);
        assert_eq!(scale_to_8(255, 8), 255);
        assert_eq!(scale_to_8(128, 8), 128);
    }

    /// A 16-bit RGB565 server: the slow path has to read the masks, not guess.
    #[test]
    fn rgb565_decodes_to_saturated_colour() {
        let rgb565 = PixelLayout {
            bytes_per_pixel: 2,
            scanline_pad: 16,
            lsb_first: true,
            red_mask: 0xF800,
            green_mask: 0x07E0,
            blue_mask: 0x001F,
        };
        // One pure-red pixel, little-endian.
        let px = 0xF800u16.to_le_bytes();
        let out = rgb565.to_bgra(&px, 1, 1).expect("one pixel decodes");
        assert_eq!(out, vec![0, 0, 255, 255], "B,G,R,A of pure red");
    }

    /// A big-endian server must not come out with its channels reversed.
    #[test]
    fn big_endian_pixels_are_read_most_significant_byte_first() {
        let be = PixelLayout { lsb_first: false, ..bgrx() };
        // 0x00FF0000 = pure red, sent MSB first.
        let px = 0x00FF_0000u32.to_be_bytes();
        let out = be.to_bgra(&px, 1, 1).expect("one pixel decodes");
        assert_eq!(out, vec![0, 0, 255, 255], "B,G,R,A of pure red");
    }

    #[test]
    fn to_bgra_refuses_a_short_buffer_rather_than_panicking() {
        assert!(bgrx().to_bgra(&[0u8; 4], 2, 2).is_none());
    }

    #[test]
    fn jpeg_downscales_only_when_over_the_cap_and_keeps_aspect() {
        let bgra = vec![0x80u8; 400 * 200 * 4];
        let (w, h, data) = bgra_to_jpeg(400, 200, &bgra, 100, 70).expect("encodes");
        assert_eq!((w, h), (100, 50), "longest side capped, aspect kept");
        assert!(data.starts_with(&[0xFF, 0xD8]), "JPEG SOI marker");

        let (w, h, _) = bgra_to_jpeg(400, 200, &bgra, 4000, 70).expect("encodes");
        assert_eq!((w, h), (400, 200), "under the cap, untouched");
    }

    #[test]
    fn jpeg_rejects_a_buffer_that_is_too_small_for_its_dimensions() {
        assert!(bgra_to_jpeg(400, 200, &[0u8; 16], 100, 70).is_none());
        assert!(bgra_to_jpeg(0, 0, &[], 100, 70).is_none());
    }
}
