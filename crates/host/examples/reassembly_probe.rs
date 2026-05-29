//! M1d reassembly probe: validate the receiver-side reconstruction in isolation.
//!
//! Encodes a synthetic surface to H.264, serialises it through the real wire
//! protocol (`StreamStart` + `Frame`) into an in-memory buffer, then rebuilds a
//! decodable `CMSampleBuffer` purely from the bytes that crossed the "wire" — a
//! `CMVideoFormatDescription` rebuilt from the SPS/PPS, plus each frame's AVCC
//! data wrapped in a `CMBlockBuffer` — and decodes it. No capture and no Screen
//! Recording permission needed, so it is fully self-verifiable. This proves the
//! hardest part of M1d (receiver reconstruction) before TCP is wired up.
//!
//! Run: cargo run -p extender-host --example reassembly_probe

use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use apple_cf::cm::{CMBlockBuffer, CMFormatDescription, CMSampleBuffer};
use apple_cf::iosurface::IOSurface;
use apple_cf::raw;
use extender_protocol::{self as protocol, Codec as WireCodec, Message};
use videotoolbox::prelude::*;
use videotoolbox::{DecodedFrame, DecompressionSession};

const WIDTH: i32 = 1920;
const HEIGHT: i32 = 1080;
const FPS: i32 = 60;
const FRAME_COUNT: i64 = 30;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (wire, sent) = encode_to_wire()?;
    let decoded = decode_from_wire(&wire)?;
    println!("roundtrip OK: reconstructed + decoded {decoded}/{sent} frames straight off the wire");
    assert_eq!(decoded, sent, "every transmitted frame should decode");
    Ok(())
}

/// Sender side: encode a synthetic surface and serialise every frame through the
/// real protocol into an in-memory "wire" buffer. Returns the buffer and the
/// number of `Frame` messages written.
fn encode_to_wire() -> Result<(Vec<u8>, usize), Box<dyn std::error::Error>> {
    let surface =
        IOSurface::create(WIDTH as usize, HEIGHT as usize, u32::from_be_bytes(*b"BGRA"), 4)
            .ok_or("failed to allocate IOSurface")?;
    let encoder = CompressionSession::builder(WIDTH, HEIGHT, Codec::H264)
        .with_real_time(true)
        .with_allow_frame_reordering(false)
        .with_average_bit_rate(8_000_000)
        .with_expected_frame_rate(f64::from(FPS))
        .with_max_keyframe_interval(FPS)
        .build()?;

    let mut wire = Vec::new();
    let mut started = false;
    let mut sent = 0usize;
    for i in 0..FRAME_COUNT {
        let frame = encoder.encode(&surface, (i, FPS))?;
        if frame.data.is_empty() {
            continue; // encoder dropped this frame
        }
        let keyframe = protocol::is_keyframe(WireCodec::H264, &frame.data);
        if !started {
            if !keyframe {
                continue; // begin the stream on the first keyframe, where SPS/PPS exist
            }
            let parameter_sets =
                h264_parameter_sets(&frame).ok_or("could not extract SPS/PPS from encoder")?;
            protocol::write_message(
                &mut wire,
                &Message::StreamStart {
                    width: WIDTH as u32,
                    height: HEIGHT as u32,
                    codec: WireCodec::H264,
                    parameter_sets,
                },
            )?;
            started = true;
        }
        let (pts_value, pts_timescale) = frame.presentation_time;
        protocol::write_message(
            &mut wire,
            &Message::Frame { pts_value, pts_timescale, keyframe, data: frame.data },
        )?;
        sent += 1;
    }
    println!("sender: encoded + serialised {sent} frames into {} wire bytes", wire.len());
    Ok((wire, sent))
}

/// Receiver side: parse the wire and reconstruct decodable sample buffers from
/// nothing but the transmitted bytes. Returns how many frames the decoder emitted.
fn decode_from_wire(wire: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
    let decoded = Arc::new(AtomicUsize::new(0));
    let mut format: Option<CMFormatDescription> = None;
    let mut decoder: Option<DecompressionSession> = None;

    let mut cursor = std::io::Cursor::new(wire);
    loop {
        let message = match protocol::read_message(&mut cursor) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        match message {
            Message::StreamStart { codec, parameter_sets, .. } => {
                let fmt = build_format_description(codec, &parameter_sets)
                    .ok_or("failed to rebuild format description from parameter sets")?;
                let counter = decoded.clone();
                let session = DecompressionSession::new(&fmt, move |f: DecodedFrame| {
                    if let Some(img) = f.image_buffer {
                        let n = counter.fetch_add(1, Ordering::Relaxed);
                        if n < 3 {
                            println!(
                                "receiver: decoded frame {n}: {}x{} pixfmt=0x{:08X} status={}",
                                img.width(),
                                img.height(),
                                img.pixel_format(),
                                f.status,
                            );
                        }
                    } else {
                        eprintln!("receiver: empty decode callback (status={})", f.status);
                    }
                })?;
                format = Some(fmt);
                decoder = Some(session);
            }
            Message::Frame { pts_value, pts_timescale, data, .. } => {
                let format = format.as_ref().ok_or("Frame arrived before StreamStart")?;
                let decoder = decoder.as_ref().ok_or("Frame arrived before StreamStart")?;
                let sample = reassemble_sample(format, &data, (pts_value, pts_timescale))
                    .ok_or("failed to reassemble CMSampleBuffer")?;
                decoder.decode(&sample)?;
            }
        }
    }

    if let Some(decoder) = &decoder {
        decoder.wait_for_async_frames()?;
    }
    Ok(decoded.load(Ordering::Relaxed))
}

/// Extract H.264 parameter sets (SPS, PPS, ...) from an encoded frame's format
/// description — the host's job when populating `StreamStart`.
fn h264_parameter_sets(frame: &EncodedFrame) -> Option<Vec<Vec<u8>>> {
    let format = frame.cm_sample_buffer()?.format_description()?;
    let desc = format.as_ptr();

    // First query reports the total parameter-set count.
    let mut count: usize = 0;
    let status = unsafe {
        raw::CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
            desc as _,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut count,
            ptr::null_mut(),
        )
    };
    if status != 0 || count == 0 {
        return None;
    }

    let mut sets = Vec::with_capacity(count);
    for index in 0..count {
        let mut data: *const u8 = ptr::null();
        let mut size: usize = 0;
        let status = unsafe {
            raw::CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                desc as _,
                index,
                &mut data,
                &mut size,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if status != 0 || data.is_null() {
            return None;
        }
        sets.push(unsafe { std::slice::from_raw_parts(data, size) }.to_vec());
    }
    Some(sets)
}

/// Rebuild a `CMVideoFormatDescription` from parameter sets — the client's first
/// step after receiving `StreamStart`.
fn build_format_description(
    codec: WireCodec,
    parameter_sets: &[Vec<u8>],
) -> Option<CMFormatDescription> {
    if parameter_sets.is_empty() {
        return None;
    }
    let pointers: Vec<*const u8> = parameter_sets.iter().map(|s| s.as_ptr()).collect();
    let sizes: Vec<usize> = parameter_sets.iter().map(Vec::len).collect();
    let mut out: raw::CMFormatDescriptionRef = ptr::null();
    // VideoToolbox emits AVCC with 4-byte NAL length prefixes; the decoder must
    // be told the same so it can find NAL boundaries in each sample.
    let status = unsafe {
        match codec {
            WireCodec::H264 => raw::CMVideoFormatDescriptionCreateFromH264ParameterSets(
                ptr::null(),
                pointers.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                4,
                &mut out,
            ),
            WireCodec::Hevc => raw::CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                ptr::null(),
                pointers.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                4,
                ptr::null(),
                &mut out,
            ),
        }
    };
    if status != 0 || out.is_null() {
        return None;
    }
    CMFormatDescription::from_raw(out as *mut _)
}

/// Wrap AVCC frame bytes in a `CMBlockBuffer` and assemble a ready
/// `CMSampleBuffer` the decoder can consume.
fn reassemble_sample(
    format: &CMFormatDescription,
    data: &[u8],
    pts: (i64, i32),
) -> Option<CMSampleBuffer> {
    let block = CMBlockBuffer::create(data)?;
    let timing = raw::CMSampleTimingInfo {
        duration: cm_time(1, pts.1),
        presentationTimeStamp: cm_time(pts.0, pts.1),
        // Invalid DTS (flags = 0) tells CoreMedia to treat decode order = PTS order.
        decodeTimeStamp: raw::CMTime { value: 0, timescale: 0, flags: 0, epoch: 0 },
    };
    let size = data.len();
    let mut out: raw::CMSampleBufferRef = ptr::null_mut();
    let status = unsafe {
        raw::CMSampleBufferCreateReady(
            ptr::null(),
            block.as_ptr() as _,
            format.as_ptr() as _,
            1,
            1,
            &timing,
            1,
            &size,
            &mut out,
        )
    };
    if status != 0 || out.is_null() {
        return None;
    }
    CMSampleBuffer::from_raw(out.cast())
}

/// Construct a valid `CMTime` (the `kCMTimeFlags_Valid` bit set).
const fn cm_time(value: i64, timescale: i32) -> raw::CMTime {
    raw::CMTime { value, timescale, flags: 1, epoch: 0 }
}
