//! M1d host: capture the main display, hardware-encode it to H.264, and stream
//! the encoded frames to a connected client over TCP. The sender half of the
//! network loopback — run it alongside `extender-client`.
//!
//! Run: cargo run -p extender-host [-- BIND_ADDR]   (default 0.0.0.0:9000)
//! Requires Screen Recording permission (System Settings > Privacy & Security).

use std::io::{BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use apple_cf::iosurface::IOSurface;
use apple_cf::raw;
use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use extender_protocol::{self as protocol, Button, Codec as WireCodec, Input, Message};
use screencapturekit::prelude::*;
use videotoolbox::prelude::*;

const FPS: i32 = 60;
const BITRATE: i32 = 20_000_000;
const DEFAULT_ADDR: &str = "0.0.0.0:9000";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_ADDR.to_string());
    let listener = TcpListener::bind(&addr)?;
    println!(
        "extender-host listening on {addr} (protocol v{})",
        protocol::PROTOCOL_VERSION
    );
    println!("waiting for a client to connect...");

    for incoming in listener.incoming() {
        let stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                eprintln!("accept failed: {e}");
                continue;
            }
        };
        let peer = stream
            .peer_addr()
            .map_or_else(|_| "?".to_string(), |a| a.to_string());
        println!("client connected: {peer}");
        match serve(stream) {
            Ok(()) => println!("client {peer} disconnected"),
            Err(e) => eprintln!("session with {peer} ended: {e}"),
        }
        println!("waiting for a client to connect...");
    }
    Ok(())
}

/// Capture + encode the main display and stream it to one connected client until
/// it disconnects. A fresh capture session and encoder per client guarantees the
/// stream opens on a keyframe.
fn serve(stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let content = SCShareableContent::get()?;
    let display = content
        .displays()
        .into_iter()
        .next()
        .ok_or("no displays available (is Screen Recording permission granted?)")?;
    let (width, height) = (display.width(), display.height());
    println!("capturing display {} at {width}x{height}", display.display_id());

    let encoder = Arc::new(
        CompressionSession::builder(width as i32, height as i32, Codec::H264)
            .with_real_time(true)
            .with_allow_frame_reordering(false)
            .with_average_bit_rate(BITRATE)
            .with_expected_frame_rate(f64::from(FPS))
            .with_max_keyframe_interval(FPS)
            .build()?,
    );

    let filter = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_windows(&[])
        .build();
    let config = SCStreamConfiguration::new()
        .with_width(width)
        .with_height(height)
        .with_fps(FPS as u32);

    let (tx, rx) = mpsc::channel::<EncodedFrame>();
    let frame_no = Arc::new(AtomicI64::new(0));

    let mut sc = SCStream::new(&filter, &config);
    {
        let encoder = encoder.clone();
        let frame_no = frame_no.clone();
        sc.add_output_handler(
            move |sample: CMSampleBuffer, _ty: SCStreamOutputType| {
                capture_and_encode(&sample, &encoder, &frame_no, &tx);
            },
            SCStreamOutputType::Screen,
        );
    }

    // A second handle on the same socket carries client -> host input, injected
    // on its own thread for as long as the client stays connected.
    let input_stream = stream.try_clone()?;
    let input_thread = thread::spawn(move || receive_and_inject(input_stream, width, height));

    sc.start_capture()?;
    let result = stream_to_client(stream, &rx, width, height);
    let _ = sc.stop_capture();
    let _ = input_thread.join();
    result
}

/// Read input events from the client and inject them into the OS until the
/// client disconnects. Pointer coordinates map from normalized frame space to
/// the captured display's pixels.
fn receive_and_inject(mut stream: TcpStream, width: u32, height: u32) {
    let (w, h) = (f64::from(width), f64::from(height));
    let mut cursor = (0.0_f64, 0.0_f64);
    while let Ok(input) = protocol::read_framed::<_, Input>(&mut stream) {
        inject(input, w, h, &mut cursor);
    }
}

/// Inject one input event via CoreGraphics. `cursor` tracks the last pointer
/// position so button events fire where the pointer is.
fn inject(input: Input, w: f64, h: f64, cursor: &mut (f64, f64)) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return;
    };
    let (event_type, button) = match input {
        Input::MouseMove { x, y } => {
            *cursor = (f64::from(x) * w, f64::from(y) * h);
            (CGEventType::MouseMoved, CGMouseButton::Left)
        }
        Input::MouseButton { button, pressed } => match (button, pressed) {
            (Button::Left, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            (Button::Left, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
            (Button::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
            (Button::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
            (Button::Middle, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
            (Button::Middle, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
        },
        // Scroll and keyboard arrive in M2d.
        Input::Scroll { .. } | Input::Key { .. } => return,
    };
    if let Ok(event) =
        CGEvent::new_mouse_event(source, event_type, CGPoint::new(cursor.0, cursor.1), button)
    {
        event.post(CGEventTapLocation::HID);
    }
}

/// Drain encoded frames and write them to the client. Returns `Ok` on a clean
/// client disconnect (any socket write error means the client went away).
fn stream_to_client(
    stream: TcpStream,
    rx: &Receiver<EncodedFrame>,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = BufWriter::new(stream);
    let mut started = false;
    let mut count = 0u64;

    while let Ok(frame) = rx.recv() {
        if frame.data.is_empty() {
            continue; // encoder dropped this frame
        }
        let keyframe = protocol::is_keyframe(WireCodec::H264, &frame.data);

        if !started {
            if !keyframe {
                continue; // open the stream on the first keyframe (SPS/PPS available)
            }
            let parameter_sets =
                h264_parameter_sets(&frame).ok_or("could not extract SPS/PPS from encoder")?;
            let start = Message::StreamStart {
                width,
                height,
                codec: WireCodec::H264,
                parameter_sets,
            };
            if protocol::write_framed(&mut out, &start).is_err() {
                return Ok(()); // client gone
            }
            started = true;
            println!("stream started: {width}x{height} H.264, streaming frames...");
        }

        let (pts_value, pts_timescale) = frame.presentation_time;
        let msg = Message::Frame { pts_value, pts_timescale, keyframe, data: frame.data };
        if protocol::write_framed(&mut out, &msg).is_err() || out.flush().is_err() {
            return Ok(()); // client gone
        }

        count += 1;
        if count.is_multiple_of(120) {
            println!("streamed {count} frames");
        }
    }
    Ok(())
}

/// Capture-callback body: bridge the captured frame's `IOSurface` into the
/// encoder and forward the encoded result to the writer thread.
fn capture_and_encode(
    sample: &CMSampleBuffer,
    encoder: &CompressionSession,
    frame_no: &AtomicI64,
    tx: &Sender<EncodedFrame>,
) {
    let Some(pixel_buffer) = sample.image_buffer() else {
        return; // ScreenCaptureKit emits occasional status frames with no image
    };
    // Borrow the IOSurface backing the captured frame and take our own retained
    // reference, so the wrapper's Drop balances exactly one release. (The
    // captured buffer stays valid for the duration of this synchronous encode.)
    let surface = unsafe {
        let raw_surface = raw::CVPixelBufferGetIOSurface(pixel_buffer.as_ptr() as _);
        if raw_surface.is_null() {
            return;
        }
        raw::CFRetain(raw_surface as _);
        IOSurface::from_ptr(raw_surface as _)
    };

    let pts = frame_no.fetch_add(1, Ordering::Relaxed);
    match encoder.encode(&surface, (pts, FPS)) {
        Ok(frame) => {
            let _ = tx.send(frame);
        }
        Err(e) => eprintln!("encode failed: {e}"),
    }
}

/// Extract the H.264 parameter sets (SPS, PPS, ...) from an encoded frame's
/// format description, for the `StreamStart` message.
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
