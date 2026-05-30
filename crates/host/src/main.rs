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
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;

use apple_cf::iosurface::IOSurface;
use apple_cf::raw;
use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use extender_protocol::{self as protocol, Button, Codec as WireCodec, Input, Message};
use screencapturekit::prelude::*;
use videotoolbox::prelude::*;

const FPS: i32 = 60;
const BITRATE: i32 = 20_000_000;
const DEFAULT_ADDR: &str = "0.0.0.0:9000";
const VDISPLAY_WIDTH: u32 = 1920;
const VDISPLAY_HEIGHT: u32 = 1080;

extern "C" {
    /// Create a virtual display (Objective-C shim) at the given pixel size;
    /// returns its CGDirectDisplayID (0 on failure). Retained for the process lifetime.
    fn extender_vdisplay_create(width: u32, height: u32) -> u32;
}

/// A display's global bounds: origin x/y and width/height, in points.
type Bounds = (f64, f64, f64, f64);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_ADDR.to_string());
    // Optional 2nd arg: the virtual display's logical size, e.g. "2560x1440".
    let (logical_w, logical_h) = std::env::args()
        .nth(2)
        .and_then(|s| parse_resolution(&s))
        .unwrap_or((VDISPLAY_WIDTH, VDISPLAY_HEIGHT));

    // Create the virtual display we extend onto and wait for the window server to
    // register it, so capture can find it and input can target its bounds.
    let virtual_id = unsafe { extender_vdisplay_create(logical_w, logical_h) };
    if virtual_id == 0 {
        return Err("failed to create the virtual display (CGVirtualDisplay rejected it)".into());
    }
    let bounds = wait_for_display(virtual_id)
        .ok_or("virtual display did not register with the window server")?;
    let capture = (logical_w, logical_h);
    println!("created virtual display {virtual_id}: capturing {logical_w}x{logical_h} px");

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
        match serve(stream, virtual_id, capture, bounds) {
            Ok(()) => println!("client {peer} disconnected"),
            Err(e) => eprintln!("session with {peer} ended: {e}"),
        }
        println!("waiting for a client to connect...");
    }
    Ok(())
}

/// Parse a "WIDTHxHEIGHT" resolution string (e.g. "2560x1440").
fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Poll until display `id` is registered, returning its global bounds (origin
/// x/y, width, height — in points) for input mapping.
fn wait_for_display(id: u32) -> Option<Bounds> {
    for _ in 0..50 {
        if CGDisplay::active_displays()
            .unwrap_or_default()
            .contains(&id)
        {
            let b = CGDisplay::new(id).bounds();
            return Some((b.origin.x, b.origin.y, b.size.width, b.size.height));
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

/// Capture + encode the main display and stream it to one connected client until
/// it disconnects. A fresh capture session and encoder per client guarantees the
/// stream opens on a keyframe.
fn serve(
    stream: TcpStream,
    virtual_id: u32,
    capture: (u32, u32),
    bounds: Bounds,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_nodelay(true); // disable Nagle — low latency for video + input
    let content = SCShareableContent::get()?;
    let display = content
        .displays()
        .into_iter()
        .find(|d| d.display_id() == virtual_id)
        .ok_or("virtual display not in shareable content (is Screen Recording permission granted?)")?;
    // Capture at the display's native pixel size (2x logical on HiDPI) so the
    // streamed frame keeps Retina detail instead of being downsampled to points.
    let (width, height) = capture;
    println!("capturing virtual display {virtual_id} at {width}x{height} px");

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

    // Bounded so a slow network back-pressures the encoder (ScreenCaptureKit then
    // drops capture frames) instead of growing latency — we can't drop *encoded*
    // frames mid-GOP without breaking H.264 continuity.
    let (tx, rx) = mpsc::sync_channel::<EncodedFrame>(2);
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
    let input_thread = thread::spawn(move || receive_and_inject(input_stream, bounds));

    sc.start_capture()?;
    let result = stream_to_client(stream, &rx, width, height);
    let _ = sc.stop_capture();
    let _ = input_thread.join();
    result
}

/// Read input events from the client and inject them into the OS until the
/// client disconnects. Pointer coordinates map from normalized frame space to
/// the captured display's pixels.
fn receive_and_inject(mut stream: TcpStream, bounds: Bounds) {
    let mut cursor = (bounds.0, bounds.1);
    while let Ok(input) = protocol::read_framed::<_, Input>(&mut stream) {
        inject(input, bounds, &mut cursor);
    }
}

/// Inject one input event via CoreGraphics. `cursor` tracks the last pointer
/// position so button and scroll events fire where the pointer is. Normalized
/// pointer coords map into the virtual display's global `bounds`.
fn inject(input: Input, bounds: Bounds, cursor: &mut (f64, f64)) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return;
    };
    match input {
        Input::MouseMove { x, y } => {
            *cursor = (
                bounds.0 + f64::from(x) * bounds.2,
                bounds.1 + f64::from(y) * bounds.3,
            );
            post_mouse(source, CGEventType::MouseMoved, *cursor, CGMouseButton::Left);
        }
        Input::MouseMoveRelative { dx, dy } => {
            cursor.0 = (cursor.0 + f64::from(dx)).clamp(bounds.0, bounds.0 + bounds.2 - 1.0);
            cursor.1 = (cursor.1 + f64::from(dy)).clamp(bounds.1, bounds.1 + bounds.3 - 1.0);
            post_mouse(source, CGEventType::MouseMoved, *cursor, CGMouseButton::Left);
        }
        Input::MouseButton { button, pressed } => {
            let (event_type, cg_button) = match (button, pressed) {
                (Button::Left, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
                (Button::Left, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
                (Button::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
                (Button::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
                (Button::Middle, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
                (Button::Middle, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
            };
            post_mouse(source, event_type, *cursor, cg_button);
        }
        Input::Scroll { dx, dy } => {
            // wheel1 = vertical, wheel2 = horizontal, in line units.
            let (vertical, horizontal) = (dy.round() as i32, dx.round() as i32);
            if let Ok(event) =
                CGEvent::new_scroll_event(source, ScrollEventUnit::LINE, 2, vertical, horizontal, 0)
            {
                event.post(CGEventTapLocation::HID);
            }
        }
        Input::Key { code, pressed } => {
            if let Some(keycode) = hid_to_macos(code) {
                if let Ok(event) = CGEvent::new_keyboard_event(source, keycode, pressed) {
                    event.post(CGEventTapLocation::HID);
                }
            }
        }
    }
}

/// Post a mouse event of `event_type` at `pos` for `button`.
fn post_mouse(source: CGEventSource, event_type: CGEventType, pos: (f64, f64), button: CGMouseButton) {
    if let Ok(event) =
        CGEvent::new_mouse_event(source, event_type, CGPoint::new(pos.0, pos.1), button)
    {
        event.post(CGEventTapLocation::HID);
    }
}

/// Map a USB-HID keyboard usage id (the platform-neutral code carried on the
/// wire) to a macOS virtual keycode. Returns `None` for keys not yet mapped.
#[rustfmt::skip]
fn hid_to_macos(usage: u32) -> Option<u16> {
    let keycode: u16 = match usage {
        // Letters a–z.
        0x04 => 0x00, 0x05 => 0x0B, 0x06 => 0x08, 0x07 => 0x02, 0x08 => 0x0E, 0x09 => 0x03,
        0x0A => 0x05, 0x0B => 0x04, 0x0C => 0x22, 0x0D => 0x26, 0x0E => 0x28, 0x0F => 0x25,
        0x10 => 0x2E, 0x11 => 0x2D, 0x12 => 0x1F, 0x13 => 0x23, 0x14 => 0x0C, 0x15 => 0x0F,
        0x16 => 0x01, 0x17 => 0x11, 0x18 => 0x20, 0x19 => 0x09, 0x1A => 0x0D, 0x1B => 0x07,
        0x1C => 0x10, 0x1D => 0x06,
        // Digits 1–9, 0.
        0x1E => 0x12, 0x1F => 0x13, 0x20 => 0x14, 0x21 => 0x15, 0x22 => 0x17, 0x23 => 0x16,
        0x24 => 0x1A, 0x25 => 0x1C, 0x26 => 0x19, 0x27 => 0x1D,
        // Enter, Escape, Backspace, Tab, Space.
        0x28 => 0x24, 0x29 => 0x35, 0x2A => 0x33, 0x2B => 0x30, 0x2C => 0x31,
        // Punctuation: - = [ ] \ ; ' ` , . /  and CapsLock.
        0x2D => 0x1B, 0x2E => 0x18, 0x2F => 0x21, 0x30 => 0x1E, 0x31 => 0x2A, 0x33 => 0x29,
        0x34 => 0x27, 0x35 => 0x32, 0x36 => 0x2B, 0x37 => 0x2F, 0x38 => 0x2C, 0x39 => 0x39,
        // Arrows: right, left, down, up.
        0x4F => 0x7C, 0x50 => 0x7B, 0x51 => 0x7D, 0x52 => 0x7E,
        // Modifiers: L/R control, shift, alt(option), gui(command).
        0xE0 => 0x3B, 0xE1 => 0x38, 0xE2 => 0x3A, 0xE3 => 0x37,
        0xE4 => 0x3E, 0xE5 => 0x3C, 0xE6 => 0x3D, 0xE7 => 0x36,
        _ => return None,
    };
    Some(keycode)
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
    tx: &SyncSender<EncodedFrame>,
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
