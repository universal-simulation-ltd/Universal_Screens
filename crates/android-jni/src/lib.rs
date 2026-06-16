//! JNI bridge for the Android app: thin glue over [`extender_core::Session`] so
//! Kotlin can connect, pull encoded frames, and push input. The companion to the
//! C ABI in `extender-mobile-ffi` (which iOS uses); both wrap the same core.
//!
//! Frames are surfaced **Annex-B** so they feed `MediaCodec` directly: a `Start`
//! event (kind 0) carries the parameter sets as codec-specific data, each `Frame`
//! event (kind 1) carries that frame's NAL units. The event model is stateful to
//! keep the JNI surface small: [`nativeNextEvent`] advances to the next event and
//! returns its kind (or -1 at end of stream), then the `nativeEvent*` accessors
//! read the current event's fields.
//!
//! The Kotlin side is `com.universalsim.extender.ExtenderNative` — its package +
//! class name are encoded in every exported symbol below, so keep them in sync.

use std::sync::mpsc::{self, Sender};

use extender_core::protocol::{self, Button, CaptureMode, Gesture, Input, TouchPhase};
use extender_core::{ClientHello, Codec, Session, StreamEvent};

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jbyteArray, jfloat, jint, jlong};
use jni::JNIEnv;

/// Behind the `jlong` handle Kotlin holds: the session, the input sender, and the
/// "current" event most recently fetched by [`nativeNextEvent`].
struct AndroidSession {
    session: Session,
    input_tx: Sender<Input>,
    kind: i32,
    width: i32,
    height: i32,
    codec: i32,
    keyframe: bool,
    pts_value: i64,
    data: Vec<u8>,
}

/// Borrow the session behind a handle, or `None` if the handle is 0/null.
unsafe fn session<'a>(handle: jlong) -> Option<&'a mut AndroidSession> {
    unsafe { (handle as *mut AndroidSession).as_mut() }
}

fn codec_tag(codec: Codec) -> i32 {
    match codec {
        Codec::H264 => 0,
        Codec::Hevc => 1,
    }
}

fn send_input(handle: jlong, input: Input) {
    if let Some(s) = unsafe { session(handle) } {
        let _ = s.input_tx.send(input);
    }
}

// ---- session lifecycle ---------------------------------------------------

/// Connect and return a handle (0 on failure). `capture_mode`: 0 = virtual
/// second screen, 1 = mirror the host's primary display.
#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeConnect<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    addr: JString<'local>,
    width: jint,
    height: jint,
    capture_mode: jint,
) -> jlong {
    let Ok(addr) = env.get_string(&addr) else {
        return 0;
    };
    let addr: String = addr.into();
    let hello = ClientHello {
        protocol_version: protocol::PROTOCOL_VERSION,
        width: width as u32,
        height: height as u32,
        capture_mode: if capture_mode == 1 {
            CaptureMode::MirrorPrimary
        } else {
            CaptureMode::VirtualDisplay
        },
    };
    let (input_tx, input_rx) = mpsc::channel();
    match Session::connect(&addr, &hello, input_rx) {
        Ok(session) => Box::into_raw(Box::new(AndroidSession {
            session,
            input_tx,
            kind: -1,
            width: 0,
            height: 0,
            codec: 0,
            keyframe: false,
            pts_value: 0,
            data: Vec::new(),
        })) as jlong,
        Err(_) => 0,
    }
}

/// Disconnect and free the session.
#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        drop(unsafe { Box::from_raw(handle as *mut AndroidSession) });
    }
}

// ---- downstream events ---------------------------------------------------

/// Advance to the next event, storing it on the handle; returns its kind
/// (0 = Start, 1 = Frame) or -1 once the stream ends. Call from one thread.
#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeNextEvent(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    let Some(s) = (unsafe { session(handle) }) else {
        return -1;
    };
    match s.session.next_event() {
        Some(StreamEvent::Start { width, height, codec, parameter_sets }) => {
            s.kind = 0;
            s.width = width as i32;
            s.height = height as i32;
            s.codec = codec_tag(codec);
            s.keyframe = false;
            s.pts_value = 0;
            s.data = protocol::annex_b_parameter_sets(&parameter_sets);
            0
        }
        Some(StreamEvent::Frame { pts_value, keyframe, data, .. }) => {
            s.kind = 1;
            s.keyframe = keyframe;
            s.pts_value = pts_value;
            let mut annex_b = Vec::new();
            protocol::append_annex_b(&mut annex_b, &data);
            s.data = annex_b;
            1
        }
        None => {
            s.kind = -1;
            -1
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeEventWidth(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    unsafe { session(handle) }.map_or(0, |s| s.width)
}

#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeEventHeight(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    unsafe { session(handle) }.map_or(0, |s| s.height)
}

/// 0 = H.264, 1 = HEVC.
#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeEventCodec(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    unsafe { session(handle) }.map_or(0, |s| s.codec)
}

#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeEventKeyframe(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jboolean {
    u8::from(unsafe { session(handle) }.is_some_and(|s| s.keyframe))
}

#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeEventPts(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jlong {
    unsafe { session(handle) }.map_or(0, |s| s.pts_value)
}

/// The current event's Annex-B bytes as a fresh `byte[]` (parameter sets for a
/// Start event, NAL units for a Frame); null on a bad handle.
#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeEventData<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jbyteArray {
    let Some(s) = (unsafe { session(handle) }) else {
        return std::ptr::null_mut();
    };
    match env.byte_array_from_slice(&s.data) {
        Ok(arr) => arr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

// ---- upstream input ------------------------------------------------------

/// Key by USB-HID usage id (e.g. 0x4E = Page Down). The clicker's core call.
#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeSendKey(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    hid_code: jint,
    pressed: jboolean,
) {
    send_input(handle, Input::Key { code: hid_code as u32, pressed: pressed != 0 });
}

#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeSendMouseMove(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    x: jfloat,
    y: jfloat,
) {
    send_input(handle, Input::MouseMove { x, y });
}

/// `button`: 0 = left, 1 = right, 2 = middle.
#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeSendMouseButton(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    button: jint,
    pressed: jboolean,
) {
    let button = match button {
        1 => Button::Right,
        2 => Button::Middle,
        _ => Button::Left,
    };
    send_input(handle, Input::MouseButton { button, pressed: pressed != 0 });
}

#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeSendScroll(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    dx: jfloat,
    dy: jfloat,
) {
    send_input(handle, Input::Scroll { dx, dy });
}

/// `phase`: 0 = began, 1 = moved, 2 = ended, 3 = cancelled.
#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeSendTouch(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    id: jint,
    phase: jint,
    x: jfloat,
    y: jfloat,
) {
    let phase = match phase {
        1 => TouchPhase::Moved,
        2 => TouchPhase::Ended,
        3 => TouchPhase::Cancelled,
        _ => TouchPhase::Began,
    };
    send_input(handle, Input::Touch { id: id as u32, phase, x, y });
}

#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeSendSecondaryClick(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    x: jfloat,
    y: jfloat,
) {
    send_input(handle, Input::Gesture(Gesture::SecondaryClick { x, y }));
}

#[no_mangle]
pub extern "system" fn Java_com_universalsim_extender_ExtenderNative_nativeSendText<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    text: JString<'local>,
) {
    if let Ok(text) = env.get_string(&text) {
        send_input(handle, Input::Text { text: text.into() });
    }
}
