//! Screen mirroring for the Mirror / Remote-control modes: capture the primary
//! display (GDI, via [`crate::snapshot::grab_primary_bgra`]), H.264-encode it with
//! openh264 (software), and send `StreamStart` + `Frame` messages in the
//! protocol's wire format — parameter sets as **raw** SPS/PPS NALs, frame data as
//! **AVCC** (4-byte big-endian length-prefixed NALs) — matching the macOS host so
//! the existing desktop/mobile clients decode it unchanged.

use std::io::{BufWriter, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use extender_protocol::{self as protocol, Codec, Message};
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

/// Capture + encode + stream a screen down `stream` until `stop` is set or the
/// client disconnects. `extend` streams a secondary/virtual monitor (the phone as
/// an extra display) instead of mirroring the primary. Best-effort: logs/returns
/// on any error.
pub(crate) fn run(stream: TcpStream, stop: &AtomicBool, extend: bool) {
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
    stream: TcpStream,
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
    let (width, height) = encode_dims(src_w, src_h);

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
    let mut packed: Vec<u8> = Vec::new();
    let mut scaled: Vec<u8> = Vec::new();

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
        pack_rows(&bgra, cw, src_w, src_h, &mut packed);
        let frame_bgra: &[u8] = if width != src_w || height != src_h {
            scaled = downscale(&packed, src_w, src_h, width, height);
            &scaled
        } else {
            &packed
        };

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

        let (parameter_sets, frame_data, keyframe) = split_annex_b(&annex_b);
        if !started {
            // Open the stream on the first keyframe, when SPS/PPS are present.
            if !keyframe || parameter_sets.is_empty() {
                continue;
            }
            protocol::write_framed(
                &mut out,
                &Message::StreamStart { width, height, codec: Codec::H264, parameter_sets },
            )?;
            started = true;
        }
        protocol::write_framed(
            &mut out,
            &Message::Frame { pts_value: pts, pts_timescale: FPS as i32, keyframe, data: frame_data },
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

/// The dimensions to *encode* at: the source, capped so the long side is at most
/// `MAX_LONG` (keeping aspect, rounded even). A phone doesn't need a full 1080p+
/// desktop, and a smaller frame keeps the software encoder real-time.
fn encode_dims(w: u32, h: u32) -> (u32, u32) {
    const MAX_LONG: u32 = 1280;
    let long = w.max(h);
    if long <= MAX_LONG {
        return (w, h);
    }
    let s = f64::from(MAX_LONG) / f64::from(long);
    let nw = ((f64::from(w) * s) as u32 & !1).max(2);
    let nh = ((f64::from(h) * s) as u32 & !1).max(2);
    (nw, nh)
}

/// Downscale a tightly-packed BGRA buffer from `sw`×`sh` to `dw`×`dh`. Channel
/// order is irrelevant to a per-channel resize, so the BGRA layout is preserved.
fn downscale(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    match image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(sw, sh, src) {
        Some(img) => {
            image::imageops::resize(&img, dw, dh, image::imageops::FilterType::Triangle).into_raw()
        }
        None => src.to_vec(),
    }
}

/// Copy the first `width*4` bytes of the first `height` rows out of a tightly-
/// packed BGRA buffer (cropping odd edges down to the even dimensions).
fn pack_rows(src: &[u8], src_w: u32, width: u32, height: u32, out: &mut Vec<u8>) {
    let row = (width * 4) as usize;
    let src_row = (src_w * 4) as usize;
    out.clear();
    out.reserve(row * height as usize);
    for y in 0..height as usize {
        let s = y * src_row;
        out.extend_from_slice(&src[s..s + row]);
    }
}

/// Split an Annex-B bitstream into the protocol's wire form: SPS (type 7) / PPS
/// (type 8) become `parameter_sets` (raw NALs, no start code); every other NAL is
/// concatenated as AVCC (each 4-byte big-endian length-prefixed) for `Frame.data`.
/// `keyframe` is true when an IDR slice (type 5) is present.
fn split_annex_b(data: &[u8]) -> (Vec<Vec<u8>>, Vec<u8>, bool) {
    let mut parameter_sets = Vec::new();
    let mut frame = Vec::new();
    let mut keyframe = false;
    for nal in annex_b_nals(data) {
        let Some(&first) = nal.first() else { continue };
        match first & 0x1F {
            7 | 8 => parameter_sets.push(nal.to_vec()),
            t => {
                if t == 5 {
                    keyframe = true;
                }
                frame.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                frame.extend_from_slice(nal);
            }
        }
    }
    (parameter_sets, frame, keyframe)
}

/// The NAL unit payloads in an Annex-B buffer (start codes `00 00 01` or
/// `00 00 00 01`), with the start codes stripped.
fn annex_b_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut nals = Vec::new();
    let mut start: Option<usize> = None;
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            if let Some(s) = start {
                // Trim the extra leading zero of a 4-byte start code.
                let end = if i > s && data[i - 1] == 0 { i - 1 } else { i };
                if end > s {
                    nals.push(&data[s..end]);
                }
            }
            start = Some(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        nals.push(&data[s..]);
    }
    nals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annexb(nals: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for nal in nals {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(nal);
        }
        out
    }

    #[test]
    fn split_sorts_params_and_builds_avcc_keyframe() {
        let sps: &[u8] = &[0x67, 0x42, 0x00];
        let pps: &[u8] = &[0x68, 0xce];
        let idr: &[u8] = &[0x65, 0x88, 0x84];
        let (params, frame, keyframe) = split_annex_b(&annexb(&[sps, pps, idr]));
        assert!(keyframe);
        assert_eq!(params, vec![sps.to_vec(), pps.to_vec()]);
        let mut expect = Vec::new();
        expect.extend_from_slice(&(idr.len() as u32).to_be_bytes());
        expect.extend_from_slice(idr);
        assert_eq!(frame, expect);
    }

    #[test]
    fn split_non_keyframe_has_no_params_or_idr() {
        let pslice: &[u8] = &[0x41, 0x9a, 0x00]; // type 1
        let (params, frame, keyframe) = split_annex_b(&annexb(&[pslice]));
        assert!(!keyframe);
        assert!(params.is_empty());
        assert_eq!(&frame[4..], pslice);
    }

    #[test]
    fn pack_rows_crops_to_even_width() {
        // 3x2 source (BGRA), crop to width 2, height 2.
        let src: Vec<u8> = (0..3 * 2 * 4).map(|b| b as u8).collect();
        let mut out = Vec::new();
        pack_rows(&src, 3, 2, 2, &mut out);
        assert_eq!(out.len(), 2 * 2 * 4);
        assert_eq!(&out[0..8], &src[0..8]); // first two pixels of row 0
        assert_eq!(&out[8..16], &src[12..20]); // first two pixels of row 1 (skips px 3)
    }
}
