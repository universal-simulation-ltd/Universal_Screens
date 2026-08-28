//! Screen mirroring for the Mirror / Remote-control modes: capture the X11 root
//! window (via [`crate::capture`]), H.264-encode it with openh264 (software), and
//! send `StreamStart` + `Frame` messages down the same socket the clicker uses.
//!
//! Stage 2b of `docs/LINUX-HOST.md`. The capture half was built in Stage 2a and
//! its `grab_primary_bgra` deliberately matches the Windows host's signature, so
//! what is left here is the encoder and the frame loop — and the wire framing
//! isn't even that, since it is shared with the Windows host in
//! [`extender_h264`]. A mirrored frame from this host and one from the Windows
//! host are byte-identical in shape; only the pixels differ.
//!
//! ⚠️ **X11 only.** There is no Wayland path here and this module does not
//! pretend otherwise: [`crate::main`] checks [`crate::capture::is_available`]
//! before choosing to mirror at all, and falls back to serving the client as a
//! clicker when there is no X server to photograph. A Wayland mirror is Stage 3
//! (portal + PipeWire), and shares nothing with this file.
//!
//! **The second screen is X11 too.** A client that asks to *extend* rather than
//! mirror gets a real extra display: [`crate::vdisplay`] grows the root
//! framebuffer and declares the new area a RandR monitor, and this module
//! captures that rectangle instead of the whole desktop. Everything downstream —
//! encoder, framing, wire format — is identical, which is the point: a second
//! screen is a different *region*, not a different stream.
//!
//! ⚠️ **It still degrades to a mirror**, and the reasons are ordinary rather than
//! exotic: a Wayland session (no X server to extend), a fixed-size headless
//! server such as Xvfb, or a driver whose framebuffer cannot grow that far.
//! [`crate::vdisplay::VirtualScreen::create`] returns the reason and [`run`]
//! logs it — a phone that has already drawn its mode picker gets the desktop
//! mirrored rather than a dead session.

use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use extender_protocol::{self as protocol, Codec, Message};
use crate::capture::{self, Region};
use crate::vdisplay::VirtualScreen;
use extender_transport::Conn;
use openh264::encoder::{
    BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Profile,
    RateControlMode, UsageType,
};
use openh264::formats::{BgraSliceU8, YUVBuffer};
use openh264::OpenH264API;

/// Target frame rate. The same 30 fps the Windows host targets, so a client sees
/// one stream shape whichever machine it is driving.
const FPS: u32 = 30;
/// Target H.264 bitrate.
const BITRATE_BPS: u32 = 12_000_000;
/// Cap the encoded long side (see [`extender_h264::encode_dims`]). Shared value
/// with the Windows host: a phone doesn't need a full desktop's worth of pixels,
/// and a smaller frame is what keeps a *software* encoder real-time.
const MAX_ENCODE_LONG_SIDE: u32 = 1280;
/// Give up after this many consecutive failed grabs.
///
/// ⚠️ The Windows twin loops forever on a failing capture. That is survivable
/// there because GDI failing is transient; here the usual cause is the **X
/// server going away** (a session logout, or `DISPLAY` pointing somewhere that
/// stopped existing), which never recovers — and a silent forever-loop sending
/// no frames is indistinguishable, from the phone, from a frozen desktop.
/// Ending the stream at least lets the client say the mirror stopped.
const MAX_CONSECUTIVE_GRAB_FAILURES: u32 = 90; // ~3 s at 30 fps

/// What the client asked the stream for, as far as this module needs it.
///
/// A struct rather than more positional arguments: `run(conn, stop, true, 1179,
/// 2556, name)` is exactly the call site where a width and a height get swapped
/// and nobody notices until a phone shows a sideways desktop.
#[derive(Debug, Clone)]
pub(crate) struct StreamRequest {
    /// The client wants to *extend* the desktop, not mirror it.
    pub second_screen: bool,
    /// The client's panel size in physical pixels, from its hello — the size the
    /// second screen is made, so the phone shows it 1:1 rather than scaled.
    pub client_size: (u32, u32),
    /// The client's device name, for the log line ("James's iPhone").
    pub label: String,
}

impl StreamRequest {
    /// A plain mirror of this desktop, with no client size to honour.
    #[cfg(test)]
    pub(crate) fn mirror() -> Self {
        Self { second_screen: false, client_size: (0, 0), label: String::new() }
    }
}

/// Capture + encode + stream a screen down `stream` until `stop` is set or the
/// client disconnects. Best-effort: logs and returns on any error.
///
/// For [`StreamRequest::second_screen`] this first tries to *make* the screen
/// (see [`crate::vdisplay`]) and streams that region; the virtual screen lives
/// exactly as long as this call, so the desktop is back to its normal size by
/// the time the client is disconnected. When it cannot be made, the reason is
/// logged and the primary display is mirrored instead.
pub(crate) fn run(stream: Conn, stop: &AtomicBool, req: &StreamRequest) {
    // ⚠️ Bound to a named local, not a temporary: dropping the `VirtualScreen`
    // is what shrinks the desktop again, so it has to outlive the whole stream
    // rather than the `if` that made it.
    let virtual_screen = if req.second_screen {
        match VirtualScreen::create(req.client_size.0, req.client_size.1, &req.label) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!(
                    "client asked for a second screen, but {e} - mirroring the primary instead \
                     (see docs/LINUX-HOST.md §7)"
                );
                None
            }
        }
    } else {
        None
    };
    let area = virtual_screen.as_ref().map(VirtualScreen::region);

    if let Err(e) = run_inner(stream, stop, area) {
        eprintln!("screen stream ended: {e}");
    }
}

fn run_inner(
    stream: Conn,
    stop: &AtomicBool,
    area: Option<Region>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Learn the display size from a first capture; H.264 needs even dimensions.
    let (cap_w, cap_h, _) = capture::grab_bgra_of(area).ok_or("screen capture failed")?;
    let src_w = cap_w & !1;
    let src_h = cap_h & !1;
    if src_w == 0 || src_h == 0 {
        return Err("empty display".into());
    }
    let (width, height) = extender_h264::encode_dims(src_w, src_h, MAX_ENCODE_LONG_SIDE);
    println!(
        "{}: {}x{} captured via {}, encoding at {width}x{height}",
        if area.is_some() { "second screen" } else { "mirror" },
        cap_w,
        cap_h,
        capture::status().unwrap_or_else(|e| e)
    );

    let config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(BITRATE_BPS))
        .max_frame_rate(FrameRate::from_hz(FPS as f32))
        .rate_control_mode(RateControlMode::Bitrate)
        // Tuned for real-time desktop streaming: screen-content mode, fastest
        // complexity, and a few encode threads so software keeps up at 30 fps.
        .usage_type(UsageType::ScreenContentRealTime)
        .complexity(Complexity::Low)
        .num_threads(4)
        // Baseline decodes everywhere (MediaCodec / VideoToolbox / openh264).
        .profile(Profile::Baseline)
        // Keyframe every ~2s so a client locks on (and recovers) quickly.
        .intra_frame_period(IntraFramePeriod::from_num_frames(FPS * 2));
    let mut encoder = Encoder::with_api_config(OpenH264API::from_source(), config)?;

    let mut out = BufWriter::new(stream);
    let mut started = false;
    let mut pts: i64 = 0;
    let frame_dur = Duration::from_millis(u64::from(1000 / FPS));
    let mut scratch = extender_h264::Scratch::new();
    let mut misses: u32 = 0;

    while !stop.load(Ordering::Relaxed) {
        let t0 = Instant::now();

        let Some((cw, ch, bgra)) = capture::grab_bgra_of(area) else {
            misses += 1;
            if misses >= MAX_CONSECUTIVE_GRAB_FAILURES {
                return Err("screen capture stopped working (X server gone?)".into());
            }
            thread::sleep(frame_dur);
            continue;
        };
        misses = 0;
        // A resolution change would need a fresh StreamStart — end the stream.
        // `capture::grab_area` re-reads the root geometry every frame, so an
        // `xrandr` change is seen here rather than silently producing torn
        // frames. For a second screen the region is fixed, so this fires only
        // when the desktop shrank underneath it and the region got clipped.
        if cw & !1 != src_w || ch & !1 != src_h {
            break;
        }
        let frame_bgra = scratch.fit(&bgra, cw, (src_w, src_h), (width, height));

        let yuv = YUVBuffer::from_rgb_source(BgraSliceU8::new(
            frame_bgra,
            (width as usize, height as usize),
        ));
        let annex_b = encoder.encode(&yuv)?.to_vec();
        if annex_b.is_empty() {
            // Encoder skipped this frame (rate control) — pace and continue.
            if let Some(rem) = frame_dur.checked_sub(t0.elapsed()) {
                thread::sleep(rem);
            }
            continue;
        }

        let split = extender_h264::split_annex_b(&annex_b);
        let keyframe = split.keyframe;
        if !started {
            // Open the stream on the first keyframe, when SPS/PPS are present.
            if !keyframe || split.parameter_sets.is_empty() {
                continue;
            }
            protocol::write_framed(
                &mut out,
                &Message::StreamStart {
                    width,
                    height,
                    codec: Codec::H264,
                    parameter_sets: split.parameter_sets,
                },
            )?;
            started = true;
        }
        protocol::write_framed(
            &mut out,
            &Message::Frame {
                pts_value: pts,
                pts_timescale: FPS as i32,
                keyframe,
                data: split.frame_data,
            },
        )?;
        out.flush()?;
        pts += 1;

        if let Some(rem) = frame_dur.checked_sub(t0.elapsed()) {
            thread::sleep(rem);
        }
    }
    Ok(())
}
