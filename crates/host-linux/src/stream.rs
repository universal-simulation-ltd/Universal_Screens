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
//! ⚠️ **There is no extend/second-screen mode on Linux.** The Windows host
//! streams a secondary monitor that a virtual-display driver invents; the X11
//! equivalent is an `xrandr` VIRTUAL output, which is deferred (LINUX-HOST §7).
//! A client that asks for one is mirrored instead — see [`run`].

use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use extender_protocol::{self as protocol, Codec, Message};
use crate::capture;
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

/// Capture + encode + stream the primary display down `stream` until `stop` is
/// set or the client disconnects. Best-effort: logs and returns on any error.
///
/// `asked_for_second_screen` only affects the log line — this host has no
/// second-screen mode, so the client is mirrored either way. Refusing instead
/// would leave a phone that has already drawn its mode picker with a dead
/// session and no reason, which is the same call [`crate::serve`] makes about a
/// mirror on a machine with no X server.
pub(crate) fn run(stream: Conn, stop: &AtomicBool, asked_for_second_screen: bool) {
    if asked_for_second_screen {
        eprintln!(
            "client asked for a second screen; this host has no virtual display (see \
             docs/LINUX-HOST.md §7) - mirroring the primary instead"
        );
    }
    if let Err(e) = run_inner(stream, stop) {
        eprintln!("screen stream ended: {e}");
    }
}

fn run_inner(stream: Conn, stop: &AtomicBool) -> Result<(), Box<dyn std::error::Error>> {
    // Learn the display size from a first capture; H.264 needs even dimensions.
    let (cap_w, cap_h, _) = capture::grab_primary_bgra().ok_or("screen capture failed")?;
    let src_w = cap_w & !1;
    let src_h = cap_h & !1;
    if src_w == 0 || src_h == 0 {
        return Err("empty display".into());
    }
    let (width, height) = extender_h264::encode_dims(src_w, src_h, MAX_ENCODE_LONG_SIDE);
    println!(
        "mirror: {}x{} captured via {}, encoding at {width}x{height}",
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

        let Some((cw, ch, bgra)) = capture::grab_primary_bgra() else {
            misses += 1;
            if misses >= MAX_CONSECUTIVE_GRAB_FAILURES {
                return Err("screen capture stopped working (X server gone?)".into());
            }
            thread::sleep(frame_dur);
            continue;
        };
        misses = 0;
        // A resolution change would need a fresh StreamStart — end the stream.
        // `capture::grab_bgra` re-reads the root geometry every frame, so an
        // `xrandr` change is seen here rather than silently producing torn frames.
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
