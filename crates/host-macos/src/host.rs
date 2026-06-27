//! macOS capture, encode, and input-injection logic.
//! Adapted from `crates/host/src/main.rs`; split into library-style entry points
//! so the GUI can spawn sessions on a background thread.

use std::ffi::{c_char, CString};
use std::io::{BufWriter, Write};
use std::net::TcpStream;
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

use apple_cf::iosurface::IOSurface;
use apple_cf::raw;
use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use extender_protocol::{
    self as protocol, Button, CaptureMode, Codec as WireCodec, Gesture, Input, Message, TouchPhase,
};
use screencapturekit::prelude::*;
use videotoolbox::prelude::*;

const FPS: i32 = 60;
const BITRATE: i32 = 40_000_000;

extern "C" {
    fn extender_vdisplay_create(width: u32, height: u32, name: *const c_char) -> u32;
    /// Release a virtual display so the window server removes it. Returns 1 if a
    /// display with that id was held, else 0.
    fn extender_vdisplay_destroy(id: u32) -> u32;
}

/// Global bounds of a display: origin x/y and width/height in points.
type Bounds = (f64, f64, f64, f64);

/// A live virtual display kept alive across client reconnects.
#[derive(Clone)]
pub(crate) struct Display {
    pub(crate) id: u32,
    pub(crate) size: (u32, u32),
    bounds: Bounds,
    /// The descriptor name the display was created with. A `CGVirtualDisplay`
    /// can't be renamed in place, so a differing name forces a recreate — this is
    /// how the label follows whichever device is currently connected (or the
    /// user's friendly-name override, when set).
    pub(crate) name: String,
}

/// Shared registry of virtual displays, so the GUI can list / rename / remove the
/// displays the server thread creates. There's at most one live virtual display
/// in the current single-session host, but this is a `Vec` so the list UI and a
/// future multi-display host need no reshaping.
#[derive(Default)]
pub(crate) struct VDisplays {
    /// Displays currently registered with the window server.
    pub(crate) entries: Vec<Display>,
    /// User-set friendly name. When present it overrides the connecting device's
    /// name on the next (re)create, so the display stops being relabelled per
    /// device and reads as whatever the user chose.
    pub(crate) friendly_name: Option<String>,
}

/// Remove (tear down) the virtual display with `id`, dropping it from the
/// registry. Safe to call from the GUI thread; the shim guards its own table. If
/// a session is actively streaming this display the stream will end — that's the
/// intended "remove it" behaviour.
pub(crate) fn remove_display(state: &Arc<Mutex<VDisplays>>, id: u32) {
    unsafe { extender_vdisplay_destroy(id) };
    let mut s = state.lock().unwrap();
    s.entries.retain(|d| d.id != id);
}

/// Set (or clear, with `None`) the user's friendly-name override. Applies on the
/// next connect / display (re)create; existing live displays keep their current
/// name until then.
pub(crate) fn set_friendly_name(state: &Arc<Mutex<VDisplays>>, name: Option<String>) {
    state.lock().unwrap().friendly_name = name.filter(|n| !n.trim().is_empty());
}

/// Serve a video/control session. Dispatches on `mode`: extend creates (or
/// reuses) a virtual display, mirror captures the primary, control-only streams
/// no video. The virtual display slot is kept alive in `display` across calls.
pub(crate) fn serve_session(
    stream: TcpStream,
    mode: CaptureMode,
    target_w: u32,
    target_h: u32,
    name: &str,
    vdisplays: &Arc<Mutex<VDisplays>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_nodelay(true);

    let (id, size, bounds) = match mode {
        CaptureMode::VirtualDisplay => ensure_display(vdisplays, target_w, target_h, name)?,
        CaptureMode::MirrorPrimary | CaptureMode::ControlOnly => primary_display()?,
    };

    match mode {
        CaptureMode::ControlOnly => {
            receive_and_inject(stream, bounds);
            Ok(())
        }
        _ => serve_video(stream, id, size, bounds),
    }
}

/// Control-only: inject the client's input into the real desktop, stream no video.
pub(crate) fn serve_control_only(
    stream: TcpStream,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_nodelay(true);
    let (_, _, bounds) = primary_display()?;
    receive_and_inject(stream, bounds);
    Ok(())
}

/// Capture + encode the target display and stream it to the client. A fresh
/// ScreenCaptureKit session and VideoToolbox encoder per client guarantees the
/// stream always opens on a keyframe.
fn serve_video(
    stream: TcpStream,
    target_id: u32,
    capture: (u32, u32),
    bounds: Bounds,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = stream.set_nodelay(true);
    let content = SCShareableContent::get()?;
    let display = content
        .displays()
        .into_iter()
        .find(|d| d.display_id() == target_id)
        .ok_or(
            "target display not in shareable content \
             (is Screen Recording permission granted?)",
        )?;

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

    let input_stream = stream.try_clone()?;
    let input_thread = thread::spawn(move || receive_and_inject(input_stream, bounds));

    sc.start_capture()?;
    let result = stream_to_client(stream, &rx, width, height);
    let _ = sc.stop_capture();
    let _ = input_thread.join();
    result
}

/// Read input events from the client and inject them until the client disconnects.
fn receive_and_inject(mut stream: TcpStream, bounds: Bounds) {
    let mut cursor = (bounds.0, bounds.1);
    while let Ok(input) = protocol::read_framed::<_, Input>(&mut stream) {
        inject(input, bounds, &mut cursor);
    }
}

fn inject(input: Input, bounds: Bounds, cursor: &mut (f64, f64)) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return;
    };
    match input {
        Input::MouseMove { x, y } => {
            *cursor = normalized_to_global(bounds, x, y);
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
            *cursor = normalized_to_global(bounds, x, y);
            let event_type = match phase {
                TouchPhase::Began => CGEventType::LeftMouseDown,
                TouchPhase::Moved => CGEventType::LeftMouseDragged,
                TouchPhase::Ended | TouchPhase::Cancelled => CGEventType::LeftMouseUp,
            };
            post_mouse(source, event_type, *cursor, CGMouseButton::Left);
        }
        Input::Gesture(Gesture::SecondaryClick { x, y }) => {
            *cursor = normalized_to_global(bounds, x, y);
            post_mouse(source, CGEventType::RightMouseDown, *cursor, CGMouseButton::Right);
            if let Ok(up_source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) {
                post_mouse(up_source, CGEventType::RightMouseUp, *cursor, CGMouseButton::Right);
            }
        }
        Input::Gesture(Gesture::Pinch { .. }) => {}
        Input::Text { text } => {
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
        // Slide deck / window picker are Windows-clicker-host features.
        Input::ScanDeck | Input::ListWindows | Input::FocusWindow { .. } => {}
    }
}

fn normalized_to_global(bounds: Bounds, x: f32, y: f32) -> (f64, f64) {
    (bounds.0 + f64::from(x) * bounds.2, bounds.1 + f64::from(y) * bounds.3)
}

fn post_mouse(
    source: CGEventSource,
    event_type: CGEventType,
    pos: (f64, f64),
    button: CGMouseButton,
) {
    if let Ok(event) =
        CGEvent::new_mouse_event(source, event_type, CGPoint::new(pos.0, pos.1), button)
    {
        event.post(CGEventTapLocation::HID);
    }
}

/// Get or create a virtual display of `(w, h)` pixels for the connecting
/// `device_name`. Reuses a live display that still matches (size + resolved
/// name); otherwise tears down the stale one(s) and creates fresh. The resolved
/// name is the user's friendly-name override when set, else the device name (a
/// `CGVirtualDisplay` can't be renamed in place, so a differing name forces a
/// recreate). Keeps the shared `VDisplays` registry in sync so the GUI list is
/// accurate.
pub(crate) fn ensure_display(
    state: &Arc<Mutex<VDisplays>>,
    w: u32,
    h: u32,
    device_name: &str,
) -> Result<(u32, (u32, u32), Bounds), Box<dyn std::error::Error>> {
    let desired_name = {
        let s = state.lock().unwrap();
        s.friendly_name
            .clone()
            .unwrap_or_else(|| device_name.to_string())
    };

    // Reconcile against what the window server actually has, reuse a match, and
    // tear down any stale/mismatched displays — all under the lock, but the
    // (potentially slow) create happens after we release it.
    {
        let active = CGDisplay::active_displays().unwrap_or_default();
        let mut s = state.lock().unwrap();
        s.entries.retain(|d| active.contains(&d.id));
        if let Some(d) = s
            .entries
            .iter()
            .find(|d| d.size == (w, h) && d.name == desired_name)
            .cloned()
        {
            return Ok((d.id, d.size, d.bounds));
        }
        let stale: Vec<u32> = s.entries.iter().map(|d| d.id).collect();
        for id in stale {
            println!("recreating virtual display as \"{desired_name}\" ({w}x{h})");
            unsafe { extender_vdisplay_destroy(id) };
        }
        s.entries.clear();
    }

    let c_name = CString::new(desired_name.as_str()).unwrap_or_default();
    let id = unsafe { extender_vdisplay_create(w, h, c_name.as_ptr()) };
    if id == 0 {
        return Err("CGVirtualDisplay rejected the requested size".into());
    }
    let bounds = wait_for_display(id)
        .ok_or("virtual display did not register with the window server")?;
    println!("created virtual display {id}: \"{desired_name}\" {w}x{h} px");
    let d = Display { id, size: (w, h), bounds, name: desired_name };
    state.lock().unwrap().entries.push(d.clone());
    Ok((d.id, d.size, d.bounds))
}

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
    Ok((id, size, bounds))
}

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

fn stream_to_client(
    stream: TcpStream,
    rx: &std::sync::mpsc::Receiver<EncodedFrame>,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = BufWriter::new(stream);
    let mut started = false;
    let mut count = 0u64;

    while let Ok(frame) = rx.recv() {
        if frame.data.is_empty() {
            continue;
        }
        let keyframe = protocol::is_keyframe(WireCodec::H264, &frame.data);

        if !started {
            if !keyframe {
                continue;
            }
            let parameter_sets =
                h264_parameter_sets(&frame).ok_or("could not extract SPS/PPS from encoder")?;
            let start =
                Message::StreamStart { width, height, codec: WireCodec::H264, parameter_sets };
            if protocol::write_framed(&mut out, &start).is_err() {
                return Ok(());
            }
            started = true;
            println!("stream started: {width}x{height} H.264");
        }

        let (pts_value, pts_timescale) = frame.presentation_time;
        let msg = Message::Frame { pts_value, pts_timescale, keyframe, data: frame.data };
        if protocol::write_framed(&mut out, &msg).is_err() || out.flush().is_err() {
            return Ok(());
        }

        count += 1;
        if count.is_multiple_of(120) {
            println!("streamed {count} frames");
        }
    }
    Ok(())
}

fn capture_and_encode(
    sample: &CMSampleBuffer,
    encoder: &CompressionSession,
    frame_no: &AtomicI64,
    tx: &SyncSender<EncodedFrame>,
) {
    let Some(pixel_buffer) = sample.image_buffer() else { return };
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

fn h264_parameter_sets(frame: &EncodedFrame) -> Option<Vec<Vec<u8>>> {
    let format = frame.cm_sample_buffer()?.format_description()?;
    let desc = format.as_ptr();

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

#[rustfmt::skip]
fn hid_to_macos(usage: u32) -> Option<u16> {
    let keycode: u16 = match usage {
        0x04 => 0x00, 0x05 => 0x0B, 0x06 => 0x08, 0x07 => 0x02, 0x08 => 0x0E, 0x09 => 0x03,
        0x0A => 0x05, 0x0B => 0x04, 0x0C => 0x22, 0x0D => 0x26, 0x0E => 0x28, 0x0F => 0x25,
        0x10 => 0x2E, 0x11 => 0x2D, 0x12 => 0x1F, 0x13 => 0x23, 0x14 => 0x0C, 0x15 => 0x0F,
        0x16 => 0x01, 0x17 => 0x11, 0x18 => 0x20, 0x19 => 0x09, 0x1A => 0x0D, 0x1B => 0x07,
        0x1C => 0x10, 0x1D => 0x06,
        0x1E => 0x12, 0x1F => 0x13, 0x20 => 0x14, 0x21 => 0x15, 0x22 => 0x17, 0x23 => 0x16,
        0x24 => 0x1A, 0x25 => 0x1C, 0x26 => 0x19, 0x27 => 0x1D,
        0x28 => 0x24, 0x29 => 0x35, 0x2A => 0x33, 0x2B => 0x30, 0x2C => 0x31,
        0x2D => 0x1B, 0x2E => 0x18, 0x2F => 0x21, 0x30 => 0x1E, 0x31 => 0x2A, 0x33 => 0x29,
        0x34 => 0x27, 0x35 => 0x32, 0x36 => 0x2B, 0x37 => 0x2F, 0x38 => 0x2C, 0x39 => 0x39,
        0x4F => 0x7C, 0x50 => 0x7B, 0x51 => 0x7D, 0x52 => 0x7E,
        0x4B => 0x74, 0x4E => 0x79, 0x4A => 0x73, 0x4D => 0x77, 0x49 => 0x72, 0x4C => 0x75,
        0x3A => 0x7A, 0x3B => 0x78, 0x3C => 0x63, 0x3D => 0x76, 0x3E => 0x60, 0x3F => 0x61,
        0x40 => 0x62, 0x41 => 0x64, 0x42 => 0x65, 0x43 => 0x6D, 0x44 => 0x67, 0x45 => 0x6F,
        0xE0 => 0x3B, 0xE1 => 0x38, 0xE2 => 0x3A, 0xE3 => 0x37,
        0xE4 => 0x3E, 0xE5 => 0x3C, 0xE6 => 0x3D, 0xE7 => 0x36,
        _ => return None,
    };
    Some(keycode)
}
