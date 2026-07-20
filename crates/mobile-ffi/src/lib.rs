//! C ABI over [`extender_core::Session`], so a native iOS (Swift) or Android
//! (Kotlin/JNI) shell can drive a client session without reimplementing the
//! protocol: connect, pull encoded frames, and push touch/mouse/text input.
//!
//! Decoding stays on the platform side (VideoToolbox / MediaCodec) — every byte
//! buffer this layer hands out is **Annex-B** (start-code-delimited NAL units),
//! the form both platform decoders accept: a `Start` event carries the parameter
//! sets (SPS/PPS) to prime the decoder, and each `Frame` event carries that
//! frame's NAL units. On a keyframe the consumer should prepend the stored
//! parameter sets (the `keyframe` flag says when).
//!
//! Threading contract (the C side must uphold it — Rust can't here): call
//! [`extender_session_next_event`] from a single consumer thread; the
//! `extender_send_*` calls may come from any thread. Every non-null
//! `*mut ExtenderEvent` must be released with [`extender_event_free`], and the
//! session with [`extender_session_free`].

use std::ffi::{c_char, CStr};
use std::ptr;
use std::sync::mpsc::{self, Sender};

use extender_core::protocol::{
    self, Button, CaptureMode, ClientHello, Codec, Gesture, Input, TouchPhase,
};
use extender_core::{Session, StreamEvent};

/// Opaque session handle returned by [`extender_session_connect`].
pub struct ExtenderSession {
    session: Session,
    input_tx: Sender<Input>,
}

/// The kind tag on an [`ExtenderEvent`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtenderEventKind {
    /// Stream start: `width`/`height`/`codec` valid; `data` = Annex-B param sets.
    Start = 0,
    /// One encoded frame: `pts_*`/`keyframe`/`data` valid; `data` = Annex-B NALs.
    Frame = 1,
    /// Slide preview (clicker): `width`/`height`/`slot` valid; `data` = JPEG. `slot`
    /// is 0 = current, -1 = previous, +1 = next (empty `data` = no slide there).
    Snapshot = 2,
    /// Host identity: `data` = UTF-8 `"os\nname"`.
    HostInfo = 3,
    /// Open windows: `data` = UTF-8, one `"id\ttitle"` line per window.
    WindowList = 4,
}

/// Opaque downstream event returned by [`extender_session_next_event`]. Field
/// accessors below read it; [`extender_event_free`] releases it. The `data`
/// pointer stays valid until the event is freed.
pub struct ExtenderEvent {
    kind: ExtenderEventKind,
    width: u32,
    height: u32,
    /// 0 = H.264, 1 = HEVC.
    codec: u32,
    /// Slide-preview slot for a Snapshot event: 0 current, -1 previous, +1 next.
    slot: i32,
    pts_value: i64,
    pts_timescale: i32,
    keyframe: bool,
    /// Payload bytes, owned by this event (Annex-B for Start/Frame, JPEG for
    /// Snapshot, UTF-8 text for HostInfo/WindowList).
    data: Vec<u8>,
}

/// Touch lifecycle phase, matching [`protocol::TouchPhase`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum ExtenderTouchPhase {
    Began = 0,
    Moved = 1,
    Ended = 2,
    Cancelled = 3,
}

/// Mouse button, matching [`protocol::Button`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum ExtenderMouseButton {
    Left = 0,
    Right = 1,
    Middle = 2,
}

// ---- session lifecycle ---------------------------------------------------

/// Connect to `addr` (a NUL-terminated `"host:port"` string) and start a session
/// advertising a `width`x`height` panel. `capture_mode`: 0 = virtual second
/// screen (extend), 1 = mirror the host's primary display (remote control),
/// 2 = control-only (input only, no video — the clicker). Unknown values fall
/// back to virtual. `pin` is the host's 4-digit pairing code (0 = none).
/// `device_name` is an optional human-readable name (e.g. "James's iPhone") the
/// host can use to label the virtual display; pass null or "" for none.
///
/// Returns an opaque session pointer, or null on a null/invalid `addr` or a
/// connection/handshake failure. Free it with [`extender_session_free`].
///
/// # Safety
/// `addr` must be a valid pointer to a NUL-terminated C string (or null).
/// `device_name`, if non-null, must likewise be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn extender_session_connect(
    addr: *const c_char,
    width: u32,
    height: u32,
    capture_mode: u32,
    pin: u32,
    device_name: *const c_char,
) -> *mut ExtenderSession {
    if addr.is_null() {
        return ptr::null_mut();
    }
    let Ok(addr) = unsafe { CStr::from_ptr(addr) }.to_str() else {
        return ptr::null_mut();
    };
    // Optional: null or invalid UTF-8 → no name (host falls back to a platform label).
    let device_name = if device_name.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(device_name) }
            .to_str()
            .unwrap_or("")
            .to_string()
    };
    let hello = ClientHello {
        protocol_version: protocol::PROTOCOL_VERSION,
        width,
        height,
        capture_mode: match capture_mode {
            1 => CaptureMode::MirrorPrimary,
            2 => CaptureMode::ControlOnly,
            _ => CaptureMode::VirtualDisplay,
        },
        platform: protocol::ClientPlatform::current(),
        pin,
        device_name,
    };
    let (input_tx, input_rx) = mpsc::channel();
    match Session::connect(addr, &hello, input_rx) {
        Ok(session) => Box::into_raw(Box::new(ExtenderSession { session, input_tx })),
        Err(_) => ptr::null_mut(),
    }
}

/// Block until the next stream event, returning an owned event pointer, or null
/// once the stream ends (host disconnected) or `session` is null. Call from a
/// single consumer thread. Release each event with [`extender_event_free`].
///
/// # Safety
/// `session` must be a pointer from [`extender_session_connect`] that hasn't been
/// freed.
#[no_mangle]
pub unsafe extern "C" fn extender_session_next_event(
    session: *mut ExtenderSession,
) -> *mut ExtenderEvent {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return ptr::null_mut();
    };
    match session.session.next_event() {
        Some(event) => Box::into_raw(Box::new(ffi_event(event))),
        None => ptr::null_mut(),
    }
}

/// Disconnect and free a session. The background threads stop and are joined.
///
/// # Safety
/// `session` must be a pointer from [`extender_session_connect`] (or null), freed
/// at most once.
#[no_mangle]
pub unsafe extern "C" fn extender_session_free(session: *mut ExtenderSession) {
    if !session.is_null() {
        drop(unsafe { Box::from_raw(session) });
    }
}

// ---- event accessors -----------------------------------------------------

/// # Safety
/// `event` must be a non-null pointer from [`extender_session_next_event`].
#[no_mangle]
pub unsafe extern "C" fn extender_event_kind(event: *const ExtenderEvent) -> ExtenderEventKind {
    unsafe { &*event }.kind
}

/// Frame width in pixels (meaningful on a `Start` event).
///
/// # Safety
/// As [`extender_event_kind`].
#[no_mangle]
pub unsafe extern "C" fn extender_event_width(event: *const ExtenderEvent) -> u32 {
    unsafe { &*event }.width
}

/// Frame height in pixels (meaningful on a `Start` event).
///
/// # Safety
/// As [`extender_event_kind`].
#[no_mangle]
pub unsafe extern "C" fn extender_event_height(event: *const ExtenderEvent) -> u32 {
    unsafe { &*event }.height
}

/// Codec: 0 = H.264, 1 = HEVC (meaningful on a `Start` event).
///
/// # Safety
/// As [`extender_event_kind`].
#[no_mangle]
pub unsafe extern "C" fn extender_event_codec(event: *const ExtenderEvent) -> u32 {
    unsafe { &*event }.codec
}

/// Slide-preview slot of a Snapshot event: 0 = current, -1 = previous, +1 = next.
///
/// # Safety
/// As [`extender_event_kind`].
#[no_mangle]
pub unsafe extern "C" fn extender_event_slot(event: *const ExtenderEvent) -> i32 {
    unsafe { &*event }.slot
}

/// Whether a `Frame` event is a keyframe (prepend the parameter sets if so).
///
/// # Safety
/// As [`extender_event_kind`].
#[no_mangle]
pub unsafe extern "C" fn extender_event_keyframe(event: *const ExtenderEvent) -> bool {
    unsafe { &*event }.keyframe
}

/// Presentation timestamp value of a `Frame` event.
///
/// # Safety
/// As [`extender_event_kind`].
#[no_mangle]
pub unsafe extern "C" fn extender_event_pts_value(event: *const ExtenderEvent) -> i64 {
    unsafe { &*event }.pts_value
}

/// Presentation timestamp timescale of a `Frame` event.
///
/// # Safety
/// As [`extender_event_kind`].
#[no_mangle]
pub unsafe extern "C" fn extender_event_pts_timescale(event: *const ExtenderEvent) -> i32 {
    unsafe { &*event }.pts_timescale
}

/// The event's Annex-B byte buffer; writes its length to `len`. The pointer is
/// valid until [`extender_event_free`]. Returns null (and `len` = 0) if `event`
/// or `len` is null.
///
/// # Safety
/// `event` must be a non-null event pointer; `len` must be a valid `usize` out-pointer.
#[no_mangle]
pub unsafe extern "C" fn extender_event_data(
    event: *const ExtenderEvent,
    len: *mut usize,
) -> *const u8 {
    if event.is_null() || len.is_null() {
        if !len.is_null() {
            unsafe { *len = 0 };
        }
        return ptr::null();
    }
    let data = &unsafe { &*event }.data;
    unsafe { *len = data.len() };
    data.as_ptr()
}

/// Release an event from [`extender_session_next_event`].
///
/// # Safety
/// `event` must be such a pointer (or null), freed at most once.
#[no_mangle]
pub unsafe extern "C" fn extender_event_free(event: *mut ExtenderEvent) {
    if !event.is_null() {
        drop(unsafe { Box::from_raw(event) });
    }
}

// ---- upstream input ------------------------------------------------------

/// Send an absolute pointer move; `x`/`y` are normalized `[0, 1]` from top-left.
///
/// # Safety
/// `session` must be a live session pointer.
#[no_mangle]
pub unsafe extern "C" fn extender_send_mouse_move(session: *mut ExtenderSession, x: f32, y: f32) {
    send(session, Input::MouseMove { x, y });
}

/// Move the cursor by a relative delta in pixels (the trackpad). Needs no display
/// geometry, so it works against an input-only host.
///
/// # Safety
/// As [`extender_send_mouse_move`].
#[no_mangle]
pub unsafe extern "C" fn extender_send_mouse_move_relative(
    session: *mut ExtenderSession,
    dx: f32,
    dy: f32,
) {
    send(session, Input::MouseMoveRelative { dx, dy });
}

/// Send a mouse button state change.
///
/// # Safety
/// As [`extender_send_mouse_move`].
#[no_mangle]
pub unsafe extern "C" fn extender_send_mouse_button(
    session: *mut ExtenderSession,
    button: ExtenderMouseButton,
    pressed: bool,
) {
    let button = match button {
        ExtenderMouseButton::Left => Button::Left,
        ExtenderMouseButton::Right => Button::Right,
        ExtenderMouseButton::Middle => Button::Middle,
    };
    send(session, Input::MouseButton { button, pressed });
}

/// Send a wheel scroll in lines (positive `dy` up, positive `dx` right).
///
/// # Safety
/// As [`extender_send_mouse_move`].
#[no_mangle]
pub unsafe extern "C" fn extender_send_scroll(session: *mut ExtenderSession, dx: f32, dy: f32) {
    send(session, Input::Scroll { dx, dy });
}

/// Send a touch contact update; `x`/`y` are normalized `[0, 1]`.
///
/// # Safety
/// As [`extender_send_mouse_move`].
#[no_mangle]
pub unsafe extern "C" fn extender_send_touch(
    session: *mut ExtenderSession,
    id: u32,
    phase: ExtenderTouchPhase,
    x: f32,
    y: f32,
) {
    let phase = match phase {
        ExtenderTouchPhase::Began => TouchPhase::Began,
        ExtenderTouchPhase::Moved => TouchPhase::Moved,
        ExtenderTouchPhase::Ended => TouchPhase::Ended,
        ExtenderTouchPhase::Cancelled => TouchPhase::Cancelled,
    };
    send(session, Input::Touch { id, phase, x, y });
}

/// Send a secondary-click (right-click) request at a normalized point.
///
/// # Safety
/// As [`extender_send_mouse_move`].
#[no_mangle]
pub unsafe extern "C" fn extender_send_secondary_click(
    session: *mut ExtenderSession,
    x: f32,
    y: f32,
) {
    send(session, Input::Gesture(Gesture::SecondaryClick { x, y }));
}

/// Send a pinch gesture; `scale` is relative to the gesture start (`1.0` = none).
///
/// # Safety
/// As [`extender_send_mouse_move`].
#[no_mangle]
pub unsafe extern "C" fn extender_send_pinch(session: *mut ExtenderSession, scale: f32) {
    send(session, Input::Gesture(Gesture::Pinch { scale }));
}

/// Send a key event by USB-HID keyboard usage id, e.g. `0x4E` = Page Down
/// ("next slide"), `0x4B` = Page Up, `0x29` = Escape, `0x3E` = F5. `pressed` is
/// true for key-down, false for key-up; send a down then an up for a tap. This
/// is the basis of the presentation-clicker controls.
///
/// # Safety
/// `session` must be a live session pointer.
#[no_mangle]
pub unsafe extern "C" fn extender_send_key(
    session: *mut ExtenderSession,
    hid_code: u32,
    pressed: bool,
) {
    send(session, Input::Key { code: hid_code, pressed });
}

/// Send committed Unicode text (a NUL-terminated UTF-8 string) from a soft
/// keyboard / IME. A null or invalid-UTF-8 `text` is ignored.
///
/// # Safety
/// `session` must be a live session pointer; `text` a valid C string or null.
#[no_mangle]
pub unsafe extern "C" fn extender_send_text(session: *mut ExtenderSession, text: *const c_char) {
    if text.is_null() {
        return;
    }
    if let Ok(text) = unsafe { CStr::from_ptr(text) }.to_str() {
        send(session, Input::Text { text: text.to_string() });
    }
}

/// Ask a clicker host to pre-scan the open document for next-slide look-ahead
/// (the host pages through it once; Snapshot events with slots follow).
///
/// # Safety
/// `session` must be a live session pointer.
#[no_mangle]
pub unsafe extern "C" fn extender_send_scan_deck(session: *mut ExtenderSession) {
    send(session, Input::ScanDeck);
}

/// Ask the host to (re)send its open-window list (a WindowList event follows).
///
/// # Safety
/// As [`extender_send_scan_deck`].
#[no_mangle]
pub unsafe extern "C" fn extender_send_list_windows(session: *mut ExtenderSession) {
    send(session, Input::ListWindows);
}

/// Bring the host window with `id` (from a WindowList event) to the foreground;
/// `start_show` also starts its slideshow (F5).
///
/// # Safety
/// As [`extender_send_scan_deck`].
#[no_mangle]
pub unsafe extern "C" fn extender_send_focus_window(
    session: *mut ExtenderSession,
    id: i64,
    start_show: bool,
) {
    send(session, Input::FocusWindow { id, start_show });
}

// ---- internals -----------------------------------------------------------

/// Convert a core [`StreamEvent`] into the FFI event. Start/Frame byte buffers are
/// normalized to Annex-B; Snapshot carries JPEG with its `slot`; HostInfo and
/// WindowList carry UTF-8 text (`"os\nname"` and `"id\ttitle"` lines).
fn ffi_event(event: StreamEvent) -> ExtenderEvent {
    let base = ExtenderEvent {
        kind: ExtenderEventKind::Start,
        width: 0,
        height: 0,
        codec: 0,
        slot: 0,
        pts_value: 0,
        pts_timescale: 0,
        keyframe: false,
        data: Vec::new(),
    };
    match event {
        StreamEvent::Start { width, height, codec, parameter_sets } => ExtenderEvent {
            kind: ExtenderEventKind::Start,
            width,
            height,
            codec: codec_tag(codec),
            data: protocol::annex_b_parameter_sets(&parameter_sets),
            ..base
        },
        StreamEvent::Frame { pts_value, pts_timescale, keyframe, data } => {
            let mut annex_b = Vec::new();
            protocol::append_annex_b(&mut annex_b, &data);
            ExtenderEvent {
                kind: ExtenderEventKind::Frame,
                pts_value,
                pts_timescale,
                keyframe,
                data: annex_b,
                ..base
            }
        }
        StreamEvent::Snapshot { width, height, slot, data } => ExtenderEvent {
            kind: ExtenderEventKind::Snapshot,
            width,
            height,
            slot,
            data,
            ..base
        },
        StreamEvent::HostInfo { os, name } => ExtenderEvent {
            kind: ExtenderEventKind::HostInfo,
            data: format!("{os}\n{name}").into_bytes(),
            ..base
        },
        StreamEvent::WindowList { windows } => {
            let lines: Vec<String> =
                windows.iter().map(|(id, title)| format!("{id}\t{title}")).collect();
            ExtenderEvent {
                kind: ExtenderEventKind::WindowList,
                data: lines.join("\n").into_bytes(),
                ..base
            }
        }
    }
}

fn codec_tag(codec: Codec) -> u32 {
    match codec {
        Codec::H264 => 0,
        Codec::Hevc => 1,
    }
}

/// Forward an input event, ignoring it if the session pointer is null or the
/// host has gone away (the channel send fails).
fn send(session: *mut ExtenderSession, input: Input) {
    if let Some(session) = unsafe { session.as_ref() } {
        let _ = session.input_tx.send(input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::Message;
    use std::ffi::CString;
    use std::io::BufReader;
    use std::net::TcpListener;
    use std::thread;

    /// Drive the C ABI end to end against a fake host: connect, read a Start +
    /// one Frame through the opaque-event accessors (asserting the Annex-B
    /// conversion), send a touch, and confirm the host received it.
    #[test]
    fn ffi_round_trips_through_the_c_abi() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            // The client encrypts by default, so the fake host runs the Noise
            // responder handshake (PIN 0) before any framing, like the real host.
            let conn = extender_transport::accept(sock, 0).unwrap();
            let mut sock = conn.try_clone().unwrap();
            let mut r = BufReader::new(conn);
            let _hello: ClientHello = protocol::read_framed(&mut r).unwrap();
            protocol::write_framed(
                &mut sock,
                &Message::StreamStart {
                    width: 1280,
                    height: 720,
                    codec: Codec::H264,
                    parameter_sets: vec![vec![0x67, 0x42], vec![0x68, 0xce]],
                },
            )
            .unwrap();
            // One frame: a single AVCC NAL (4-byte big-endian length prefix).
            protocol::write_framed(
                &mut sock,
                &Message::Frame {
                    pts_value: 7,
                    pts_timescale: 60,
                    keyframe: true,
                    data: vec![0, 0, 0, 2, 0x65, 0x88],
                },
            )
            .unwrap();
            let got: Input = protocol::read_framed(&mut r).unwrap();
            got
        });

        let c_addr = CString::new(addr).unwrap();
        let session =
            unsafe { extender_session_connect(c_addr.as_ptr(), 1280, 720, 0, 0, ptr::null()) };
        assert!(!session.is_null());

        // Start event: codec/geometry + Annex-B parameter sets.
        let start = unsafe { extender_session_next_event(session) };
        assert!(!start.is_null());
        assert_eq!(unsafe { extender_event_kind(start) }, ExtenderEventKind::Start);
        assert_eq!(unsafe { extender_event_width(start) }, 1280);
        assert_eq!(unsafe { extender_event_codec(start) }, 0);
        let mut len = 0usize;
        let ptr = unsafe { extender_event_data(start, &mut len) };
        let param_bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        // SPS + PPS each prefixed with the 00 00 00 01 start code.
        assert_eq!(param_bytes, &[0, 0, 0, 1, 0x67, 0x42, 0, 0, 0, 1, 0x68, 0xce]);
        unsafe { extender_event_free(start) };

        // Frame event: keyframe + Annex-B NAL.
        let frame = unsafe { extender_session_next_event(session) };
        assert_eq!(unsafe { extender_event_kind(frame) }, ExtenderEventKind::Frame);
        assert!(unsafe { extender_event_keyframe(frame) });
        assert_eq!(unsafe { extender_event_pts_value(frame) }, 7);
        let ptr = unsafe { extender_event_data(frame, &mut len) };
        let frame_bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert_eq!(frame_bytes, &[0, 0, 0, 1, 0x65, 0x88]);
        unsafe { extender_event_free(frame) };

        // Send a touch-began and confirm the host got the mapped Input.
        unsafe { extender_send_touch(session, 1, ExtenderTouchPhase::Began, 0.5, 0.25) };
        assert_eq!(
            host.join().unwrap(),
            Input::Touch { id: 1, phase: TouchPhase::Began, x: 0.5, y: 0.25 }
        );

        // Stream ends after the host closes; next event is null.
        assert!(unsafe { extender_session_next_event(session) }.is_null());
        unsafe { extender_session_free(session) };
    }

    /// A presentation-clicker keypress (Page Down) reaches the host as an
    /// `Input::Key` with the right HID usage id.
    #[test]
    fn ffi_send_key_reaches_host() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            let conn = extender_transport::accept(sock, 0).unwrap();
            let mut r = BufReader::new(conn);
            let _hello: ClientHello = protocol::read_framed(&mut r).unwrap();
            let key: Input = protocol::read_framed(&mut r).unwrap();
            key
        });

        let c_addr = CString::new(addr).unwrap();
        // 2 = control-only (the clicker's mode); 0 = no pairing PIN.
        let session =
            unsafe { extender_session_connect(c_addr.as_ptr(), 1920, 1080, 2, 0, ptr::null()) };
        assert!(!session.is_null());

        // 0x4E = Page Down (next slide).
        unsafe { extender_send_key(session, 0x4E, true) };
        assert_eq!(host.join().unwrap(), Input::Key { code: 0x4E, pressed: true });
        unsafe { extender_session_free(session) };
    }

    /// The clicker events (HostInfo, WindowList, Snapshot) surface through the C
    /// ABI with the right kinds/payloads, and the scan/focus sends reach the host.
    #[test]
    fn ffi_surfaces_clicker_events_and_sends() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            let conn = extender_transport::accept(sock, 0).unwrap();
            let mut sock = conn.try_clone().unwrap();
            let mut r = BufReader::new(conn);
            let _hello: ClientHello = protocol::read_framed(&mut r).unwrap();
            protocol::write_framed(&mut sock, &Message::HostInfo { os: "windows".into(), name: "PC".into() }).unwrap();
            protocol::write_framed(&mut sock, &Message::WindowList { windows: vec![(42, "slides.pdf".into())] }).unwrap();
            protocol::write_framed(&mut sock, &Message::Snapshot { width: 960, height: 540, slot: 1, data: vec![0xFF, 0xD8, 1, 2] }).unwrap();
            let scan: Input = protocol::read_framed(&mut r).unwrap();
            let focus: Input = protocol::read_framed(&mut r).unwrap();
            (scan, focus)
        });

        let c_addr = CString::new(addr).unwrap();
        let session =
            unsafe { extender_session_connect(c_addr.as_ptr(), 1920, 1080, 2, 0, ptr::null()) };
        assert!(!session.is_null());
        let mut len = 0usize;

        // HostInfo -> "os\nname".
        let e = unsafe { extender_session_next_event(session) };
        assert_eq!(unsafe { extender_event_kind(e) }, ExtenderEventKind::HostInfo);
        let p = unsafe { extender_event_data(e, &mut len) };
        assert_eq!(unsafe { std::slice::from_raw_parts(p, len) }, b"windows\nPC");
        unsafe { extender_event_free(e) };

        // WindowList -> "id\ttitle".
        let e = unsafe { extender_session_next_event(session) };
        assert_eq!(unsafe { extender_event_kind(e) }, ExtenderEventKind::WindowList);
        let p = unsafe { extender_event_data(e, &mut len) };
        assert_eq!(unsafe { std::slice::from_raw_parts(p, len) }, b"42\tslides.pdf");
        unsafe { extender_event_free(e) };

        // Snapshot -> slot + JPEG bytes.
        let e = unsafe { extender_session_next_event(session) };
        assert_eq!(unsafe { extender_event_kind(e) }, ExtenderEventKind::Snapshot);
        assert_eq!(unsafe { extender_event_width(e) }, 960);
        assert_eq!(unsafe { extender_event_slot(e) }, 1);
        let p = unsafe { extender_event_data(e, &mut len) };
        assert_eq!(unsafe { std::slice::from_raw_parts(p, len) }, &[0xFF, 0xD8, 1, 2]);
        unsafe { extender_event_free(e) };

        // Scan + focus reach the host with the right Input variants.
        unsafe { extender_send_scan_deck(session) };
        unsafe { extender_send_focus_window(session, 42, true) };
        let (scan, focus) = host.join().unwrap();
        assert_eq!(scan, Input::ScanDeck);
        assert_eq!(focus, Input::FocusWindow { id: 42, start_show: true });

        unsafe { extender_session_free(session) };
    }
}
