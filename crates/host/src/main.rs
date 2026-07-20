//! M1d host: capture the main display, hardware-encode it to H.264, and stream
//! the encoded frames to a connected client over TCP. The sender half of the
//! network loopback — run it alongside `extender-client`.
//!
//! Run: cargo run -p extender-host [-- BIND_ADDR]   (default 0.0.0.0:9000)
//! Requires Screen Recording permission (System Settings > Privacy & Security).

use std::io::{BufWriter, Write};
use std::net::TcpListener;
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;

use apple_cf::dispatch_queue::dispatch_async_and_wait;
use apple_cf::iosurface::IOSurface;
use apple_cf::raw;
use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use extender_protocol::{
    self as protocol, Button, CaptureMode, ClientHello, Codec as WireCodec, Gesture, Input, Message,
    TouchPhase,
};
use extender_transport::{self as transport, Conn};
use screencapturekit::prelude::*;
use videotoolbox::prelude::*;

const FPS: i32 = 60;
const BITRATE: i32 = 40_000_000;
const DEFAULT_ADDR: &str = "0.0.0.0:9000";
/// Reject client-advertised sizes outside this range (defends the private
/// CGVirtualDisplay API against a garbled hello).
const MAX_DIMENSION: u32 = 16384;

extern "C" {
    /// Create a virtual display (Objective-C shim) at the given pixel size;
    /// returns its CGDirectDisplayID (0 on failure). Reassigns the shim's global,
    /// releasing any previously-created display.
    fn extender_vdisplay_create(width: u32, height: u32) -> u32;
}

/// A display's global bounds: origin x/y and width/height, in points.
type Bounds = (f64, f64, f64, f64);

/// A live virtual display: its id, pixel size, and global bounds (for input
/// mapping). Kept across reconnects and recreated only when the size changes.
struct Display {
    id: u32,
    size: (u32, u32),
    bounds: Bounds,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_ADDR.to_string());
    // Optional 2nd arg forces the virtual display size (e.g. "2560x1440"),
    // overriding the resolution the client advertises in its hello.
    let forced = std::env::args().nth(2).and_then(|s| parse_resolution(&s));
    if let Some((w, h)) = forced {
        println!("virtual display size forced to {w}x{h} (ignoring client hello)");
    }

    let listener = TcpListener::bind(&addr)?;
    println!(
        "extender-host listening on {addr} (protocol v{})",
        protocol::PROTOCOL_VERSION
    );
    println!("waiting for a client to connect...");

    // The virtual display is created lazily on the first connection, sized to the
    // client's hello (or `forced`), then reused unless a later client needs a
    // different size.
    let mut display: Option<Display> = None;

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

        // Transport encryption first: an encrypting native client opens with the
        // Noise preamble (run the responder handshake — this dev host doesn't pair,
        // so PIN 0); a legacy/loopback plaintext peer is passed through untouched.
        let mut conn = match transport::accept(stream, 0) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("handshake with {peer} failed: {e}");
                println!("waiting for a client to connect...");
                continue;
            }
        };
        if !conn.is_encrypted() {
            eprintln!("warning: client {peer} connected without transport encryption (plaintext)");
        }

        // Handshake: the client's first upstream message carries its panel
        // resolution and the capture mode it wants.
        let (mode, target_w, target_h) = match read_hello(&mut conn, &peer, forced) {
            Some(hello) => hello,
            None => {
                println!("waiting for a client to connect...");
                continue;
            }
        };

        // Pick what to capture: a virtual second screen sized to the client (the
        // "extend" default) or the host's real primary display (mirror / control).
        // Control-only needs the primary display only for its bounds (pointer
        // mapping) — it captures nothing.
        let active = match mode {
            CaptureMode::VirtualDisplay => ensure_display(&mut display, target_w, target_h),
            CaptureMode::MirrorPrimary | CaptureMode::ControlOnly => primary_display(),
        };
        let (id, size, bounds) = match active {
            Ok(active) => active,
            Err(e) => {
                eprintln!("could not provide a display for {peer}: {e}");
                println!("waiting for a client to connect...");
                continue;
            }
        };

        // Control-only (M6c): inject input but stream no video. Everything else
        // captures + encodes + streams the chosen display.
        let outcome = match mode {
            CaptureMode::ControlOnly => serve_control_only(conn, bounds),
            _ => serve(conn, id, size, bounds),
        };
        match outcome {
            Ok(()) => println!("client {peer} disconnected"),
            Err(e) => eprintln!("session with {peer} ended: {e}"),
        }
        println!("waiting for a client to connect...");
    }
    Ok(())
}

/// Read the client's [`ClientHello`] and resolve the capture mode plus the
/// virtual-display size to use: `forced` if the operator passed a CLI size,
/// otherwise the client's advertised resolution. The size is only used in
/// [`CaptureMode::VirtualDisplay`] (mirror mode captures the real display as-is).
/// Returns `None` (and logs) on a missing/garbled hello or an implausible size,
/// so the caller skips this client.
fn read_hello(
    stream: &mut Conn,
    peer: &str,
    forced: Option<(u32, u32)>,
) -> Option<(CaptureMode, u32, u32)> {
    let hello: ClientHello = match protocol::read_framed(stream) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("client {peer} sent no valid hello: {e}");
            return None;
        }
    };
    if hello.protocol_version != protocol::PROTOCOL_VERSION {
        eprintln!(
            "warning: client {peer} protocol v{} != host v{} — proceeding anyway",
            hello.protocol_version,
            protocol::PROTOCOL_VERSION
        );
    }
    if hello.width == 0
        || hello.height == 0
        || hello.width > MAX_DIMENSION
        || hello.height > MAX_DIMENSION
    {
        eprintln!(
            "client {peer} hello has implausible size {}x{}; skipping",
            hello.width, hello.height
        );
        return None;
    }
    let size = forced.unwrap_or((hello.width, hello.height));
    println!(
        "client {peer} hello: {}x{}, mode {:?}; using {}x{} (mode-dependent)",
        hello.width, hello.height, hello.capture_mode, size.0, size.1
    );
    Some((hello.capture_mode, size.0, size.1))
}

/// Resolve the host's primary display for [`CaptureMode::MirrorPrimary`]: its
/// id, native pixel size (so a Retina display streams at full resolution), and
/// global bounds in points (for input mapping). Creates no virtual display.
fn primary_display() -> Result<(u32, (u32, u32), Bounds), Box<dyn std::error::Error>> {
    let display = CGDisplay::main();
    let id = display.id;
    let b = display.bounds();
    let bounds = (b.origin.x, b.origin.y, b.size.width, b.size.height);
    let size = (
        u32::try_from(display.pixels_wide()).unwrap_or(0),
        u32::try_from(display.pixels_high()).unwrap_or(0),
    );
    if size.0 == 0 || size.1 == 0 {
        return Err("primary display reported a zero pixel size".into());
    }
    println!("mirroring primary display {id}: {}x{} px", size.0, size.1);
    Ok((id, size, bounds))
}

/// Ensure a virtual display of `(w, h)` exists, (re)creating it if absent or a
/// different size, and return its id, size, and global bounds. Recreating
/// reassigns the shim's global, releasing the previous display.
fn ensure_display(
    slot: &mut Option<Display>,
    w: u32,
    h: u32,
) -> Result<(u32, (u32, u32), Bounds), Box<dyn std::error::Error>> {
    let needs_create = match slot.as_ref() {
        Some(d) => d.size != (w, h),
        None => true,
    };
    if needs_create {
        if slot.is_some() {
            println!("resizing virtual display to {w}x{h} (recreating)");
        }
        let id = unsafe { extender_vdisplay_create(w, h) };
        if id == 0 {
            return Err("CGVirtualDisplay rejected the requested size".into());
        }
        let bounds = wait_for_display(id)
            .ok_or("virtual display did not register with the window server")?;
        println!("created virtual display {id}: {w}x{h} px");
        *slot = Some(Display { id, size: (w, h), bounds });
    }
    let d = slot.as_ref().expect("display set above");
    Ok((d.id, d.size, d.bounds))
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

/// Serve a [`CaptureMode::ControlOnly`] client (M6c): inject its input into the
/// real desktop and stream **no** video. `bounds` is the primary display's global
/// geometry, used to map normalized pointer coordinates. Returns when the client
/// disconnects.
fn serve_control_only(stream: Conn, bounds: Bounds) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_nodelay(true); // disable Nagle — low latency for input
    println!("control-only: injecting input, streaming no video");
    receive_and_inject(stream, bounds);
    Ok(())
}

/// Capture + encode the main display and stream it to one connected client until
/// it disconnects. A fresh capture session and encoder per client guarantees the
/// stream opens on a keyframe.
fn serve(
    stream: Conn,
    target_id: u32,
    capture: (u32, u32),
    bounds: Bounds,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_nodelay(true); // disable Nagle — low latency for video + input
    let content = SCShareableContent::get()?;
    let display = content
        .displays()
        .into_iter()
        .find(|d| d.display_id() == target_id)
        .ok_or("target display not in shareable content (is Screen Recording permission granted?)")?;
    // Capture at the display's native pixel size (2x logical on HiDPI) so the
    // streamed frame keeps Retina detail instead of being downsampled to points.
    let (width, height) = capture;
    println!("capturing display {target_id} at {width}x{height} px");

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

    // Deliver capture callbacks on our own *serial* queue so teardown can drain
    // it. ScreenCaptureKit can run one more `did_output_sample_buffer` after
    // `stop_capture()` returns, but the handler closure (which owns `tx`) is
    // freed when `sc` drops — a late callback would then use-after-free the
    // channel and segfault. Owning the queue lets us post a barrier that waits
    // for any in-flight callback before anything is dropped (see below).
    let capture_queue =
        DispatchQueue::new("uk.co.unisim.screens.capture", DispatchQoS::UserInteractive);

    let mut sc = SCStream::new(&filter, &config);
    {
        let encoder = encoder.clone();
        let frame_no = frame_no.clone();
        sc.add_output_handler_with_queue(
            move |sample: CMSampleBuffer, _ty: SCStreamOutputType| {
                capture_and_encode(&sample, &encoder, &frame_no, &tx);
            },
            SCStreamOutputType::Screen,
            Some(&capture_queue),
        );
    }

    // A second handle on the same socket carries client -> host input, injected
    // on its own thread for as long as the client stays connected.
    let input_stream = stream.try_clone()?;
    let input_thread = thread::spawn(move || receive_and_inject(input_stream, bounds));

    sc.start_capture()?;
    let result = stream_to_client(stream, &rx, width, height);
    let _ = sc.stop_capture();
    // Barrier: an empty block on the serial capture queue can only run once
    // every already-queued sample callback has finished, so after this returns
    // nothing can touch `tx` (or the closure) — safe to drop `sc` and the
    // channel below without the use-after-free that crashed the host on
    // disconnect.
    dispatch_async_and_wait(&capture_queue, || {});
    let _ = input_thread.join();
    result
}

/// Read input events from the client and inject them into the OS until the
/// client disconnects. Pointer coordinates map from normalized frame space to
/// the captured display's pixels.
fn receive_and_inject(mut stream: Conn, bounds: Bounds) {
    let mut cursor = (bounds.0, bounds.1);
    // Track whether the left button is held so moves can be posted as drags —
    // Quartz only treats LeftMouseDragged (not MouseMoved) as a drag, so a
    // held-button move otherwise wouldn't select text / drag windows.
    let mut left_down = false;
    while let Ok(input) = protocol::read_framed::<_, Input>(&mut stream) {
        inject(input, bounds, &mut cursor, &mut left_down);
    }
}

/// Inject one input event via CoreGraphics. `cursor` tracks the last pointer
/// position so button and scroll events fire where the pointer is. Normalized
/// pointer coords map into the captured display's global `bounds` (the virtual
/// display in extend mode, or the primary display in mirror mode). `left_down`
/// tracks the held left button so moves drag rather than hover.
fn inject(input: Input, bounds: Bounds, cursor: &mut (f64, f64), left_down: &mut bool) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return;
    };
    // While the left button is held, a move is a drag, not a hover.
    let move_type = |down: bool| {
        if down { CGEventType::LeftMouseDragged } else { CGEventType::MouseMoved }
    };
    match input {
        Input::MouseMove { x, y } => {
            *cursor = normalized_to_global(bounds, x, y);
            post_mouse(source, move_type(*left_down), *cursor, CGMouseButton::Left);
        }
        Input::MouseMoveRelative { dx, dy } => {
            cursor.0 = (cursor.0 + f64::from(dx)).clamp(bounds.0, bounds.0 + bounds.2 - 1.0);
            cursor.1 = (cursor.1 + f64::from(dy)).clamp(bounds.1, bounds.1 + bounds.3 - 1.0);
            post_mouse(source, move_type(*left_down), *cursor, CGMouseButton::Left);
        }
        Input::MouseButton { button, pressed } => {
            if button == Button::Left {
                *left_down = pressed;
            }
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
        Input::Touch { phase, x, y, .. } => {
            // A single contact drives the pointer: begin = press, move = drag,
            // end/cancel = release — all at the contact point. A tap (begin then
            // end with no move) is therefore a left click.
            *cursor = normalized_to_global(bounds, x, y);
            let event_type = match phase {
                TouchPhase::Began => CGEventType::LeftMouseDown,
                TouchPhase::Moved => CGEventType::LeftMouseDragged,
                TouchPhase::Ended | TouchPhase::Cancelled => CGEventType::LeftMouseUp,
            };
            post_mouse(source, event_type, *cursor, CGMouseButton::Left);
        }
        Input::Gesture(Gesture::SecondaryClick { x, y }) => {
            // Long-press / two-finger tap → a full right-click (down then up).
            *cursor = normalized_to_global(bounds, x, y);
            post_mouse(source, CGEventType::RightMouseDown, *cursor, CGMouseButton::Right);
            if let Ok(up_source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) {
                post_mouse(up_source, CGEventType::RightMouseUp, *cursor, CGMouseButton::Right);
            }
        }
        Input::Gesture(Gesture::Pinch { .. }) => {
            // Pinch-to-zoom has no portable CGEvent injection path (the magnify
            // gesture events are private), so it's a no-op until a mobile client
            // actually sends it and we pick a mapping (e.g. zoom shortcut).
        }
        Input::Text { text } => {
            // Soft-keyboard / IME text can't be expressed as physical scancodes:
            // post a keystroke carrying the Unicode string (keycode 0 — the string
            // is what gets inserted), down then up.
            if let Ok(event) = CGEvent::new_keyboard_event(source, 0, true) {
                event.set_string(&text);
                event.post(CGEventTapLocation::HID);
            }
            if let Ok(up_source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) {
                if let Ok(event) = CGEvent::new_keyboard_event(up_source, 0, false) {
                    event.set_string(&text);
                    event.post(CGEventTapLocation::HID);
                }
            }
        }
        // Look-ahead pre-scan and the window picker are Windows-clicker-host
        // features; this streaming host implements neither.
        Input::ScanDeck | Input::ListWindows | Input::FocusWindow { .. } => {}
    }
}

/// Map a normalized frame coordinate (`[0, 1]` from the top-left) into the
/// captured display's global point coordinates, using its `bounds`.
fn normalized_to_global(bounds: Bounds, x: f32, y: f32) -> (f64, f64) {
    (
        bounds.0 + f64::from(x) * bounds.2,
        bounds.1 + f64::from(y) * bounds.3,
    )
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
        // Navigation: PageUp/PageDown (slide back/forward), Home, End, Insert(=Help), Delete(fwd).
        0x4B => 0x74, 0x4E => 0x79, 0x4A => 0x73, 0x4D => 0x77, 0x49 => 0x72, 0x4C => 0x75,
        // Function keys F1–F12.
        0x3A => 0x7A, 0x3B => 0x78, 0x3C => 0x63, 0x3D => 0x76, 0x3E => 0x60, 0x3F => 0x61,
        0x40 => 0x62, 0x41 => 0x64, 0x42 => 0x65, 0x43 => 0x6D, 0x44 => 0x67, 0x45 => 0x6F,
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
    stream: Conn,
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
