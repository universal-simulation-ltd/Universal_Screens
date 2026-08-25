//! Screen mirroring for the Mirror / Remote-control modes: capture the primary
//! display (GDI, via [`crate::snapshot::grab_primary_bgra`]), H.264-encode it with
//! openh264 (software), and send `StreamStart` + `Frame` messages in the
//! protocol's wire format — parameter sets as **raw** SPS/PPS NALs, frame data as
//! **AVCC** (4-byte big-endian length-prefixed NALs) — matching the macOS host so
//! the existing desktop/mobile clients decode it unchanged.
//!
//! The framing and frame-sizing maths are **not** here: they are the client's
//! contract rather than anything about Windows, so they live in
//! [`extender_h264`] and the Linux host uses the same copy.

use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use extender_protocol::{self as protocol, Codec, Message};
use extender_transport::Conn;
use openh264::encoder::{
    BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Profile,
    RateControlMode, UsageType,
};
use openh264::formats::{BgraSliceU8, YUVBuffer};
use openh264::OpenH264API;
use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};

/// `MONITORINFOF_PRIMARY` — the `dwFlags` bit marking the primary monitor.
const MONITORINFOF_PRIMARY: u32 = 1;

/// Target frame rate.
const FPS: u32 = 30;
/// Target H.264 bitrate.
const BITRATE_BPS: u32 = 12_000_000;
/// Cap the encoded long side: a phone doesn't need a full 1080p+ desktop, and a
/// smaller frame keeps the software encoder real-time.
const MAX_ENCODE_LONG_SIDE: u32 = 1280;

/// Capture + encode + stream a screen down `stream` until `stop` is set or the
/// client disconnects. `extend` streams a secondary/virtual monitor (the phone as
/// an extra display) instead of mirroring the primary. Best-effort: logs/returns
/// on any error.
pub(crate) fn run(stream: Conn, stop: &AtomicBool, extend: bool) {
    if let Err(e) = run_inner(stream, stop, extend) {
        eprintln!("screen stream ended: {e}");
    }
}

/// The virtual-screen region to capture: a secondary monitor when extending (the
/// virtual display), else the whole primary. `None` means "use the primary".
fn capture_region(extend: bool) -> Option<(i32, i32, i32, i32)> {
    if !extend {
        return None;
    }
    match first_secondary_monitor() {
        Some(r) => Some(r),
        None => {
            eprintln!("extend: no secondary/virtual monitor found — mirroring the primary");
            None
        }
    }
}

/// Grab the configured region (or the primary when `region` is `None`).
fn capture(region: Option<(i32, i32, i32, i32)>) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        match region {
            Some((l, t, w, h)) => crate::snapshot::grab_region_bgra(l, t, w, h),
            None => crate::snapshot::grab_primary_bgra(),
        }
    }
}

fn run_inner(
    stream: Conn,
    stop: &AtomicBool,
    extend: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let region = capture_region(extend);
    // Learn the display size from a first capture; H.264 needs even dimensions.
    let (cap_w, cap_h, _) = capture(region).ok_or("screen capture failed")?;
    let src_w = cap_w & !1;
    let src_h = cap_h & !1;
    if src_w == 0 || src_h == 0 {
        return Err("empty display".into());
    }
    // Downscale large desktops before encoding — a phone screen doesn't need full
    // res, and it keeps the software encoder smooth. The virtual desktop itself is
    // unchanged; only the stream is scaled.
    let (width, height) = extender_h264::encode_dims(src_w, src_h, MAX_ENCODE_LONG_SIDE);

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

    while !stop.load(Ordering::Relaxed) {
        let t0 = Instant::now();

        let Some((cw, ch, bgra)) = capture(region) else {
            thread::sleep(frame_dur);
            continue;
        };
        // A resolution change would need a fresh StreamStart — end the stream.
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

/// The virtual-screen rect `(left, top, width, height)` of the first non-primary
/// monitor (the virtual display, when one exists), or `None`.
fn first_secondary_monitor() -> Option<(i32, i32, i32, i32)> {
    let mut rects: Vec<(i32, i32, i32, i32)> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_proc),
            LPARAM(std::ptr::addr_of_mut!(rects) as isize),
        );
    }
    rects.into_iter().next()
}

/// `EnumDisplayMonitors` callback: append each *non-primary* monitor's rect to the
/// `Vec<(i32,i32,i32,i32)>` passed via `data`.
unsafe extern "system" fn monitor_proc(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let rects = &mut *(data.0 as *mut Vec<(i32, i32, i32, i32)>);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(hmon, &mut info).as_bool() && info.dwFlags & MONITORINFOF_PRIMARY == 0 {
        let r = info.rcMonitor;
        rects.push((r.left, r.top, r.right - r.left, r.bottom - r.top));
    }
    BOOL(1) // keep enumerating
}
