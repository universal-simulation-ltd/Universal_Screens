//! M7b — `wasm-bindgen` shim over `extender-protocol` for the browser client.
//!
//! The browser speaks the *exact* `postcard` wire format the native client and
//! host use, by reusing the canonical Rust types here rather than reimplementing
//! `postcard` in TypeScript (which would drift — the wire has been revised 10
//! times). This crate exposes:
//!
//! - `decode_message(bytes) -> DecodedMessage` for the downstream `Message`
//!   stream, with typed getters (byte buffers come back as `Uint8Array`).
//! - `encode_hello` / `encode_*` for the upstream `ClientHello` + `Input` events.
//! - `avc_codec_string` / `avcc_description` to build the WebCodecs
//!   `VideoDecoder` config from the SPS/PPS in `StreamStart` (the M7c decoder).
//!
//! Errors are returned as `Result<_, String>`; `wasm-bindgen` throws the string
//! as a JS error, and a plain `String` is constructible in native tests (unlike
//! `JsError`, whose constructor traps off-wasm).
//!
//! WS transport, WebCodecs decode, canvas render, and input capture all live in
//! TypeScript on top of this — see `apps/web/`.

use extender_protocol::{
    self as protocol, Button, CaptureMode, ClientHello, ClientPlatform, Codec, Gesture, Input,
    Message, TouchPhase,
};
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Enum <-> u8 mappings shared with the TS side (and `hid.ts`). Kept explicit so
// an out-of-range value is a clear error rather than a silent wrong variant.
// ---------------------------------------------------------------------------

fn capture_mode(v: u8) -> Result<CaptureMode, String> {
    Ok(match v {
        0 => CaptureMode::VirtualDisplay,
        1 => CaptureMode::MirrorPrimary,
        2 => CaptureMode::ControlOnly,
        _ => return Err(format!("invalid capture mode: {v}")),
    })
}

fn platform(v: u8) -> Result<ClientPlatform, String> {
    Ok(match v {
        0 => ClientPlatform::Unknown,
        1 => ClientPlatform::Windows,
        2 => ClientPlatform::Macos,
        3 => ClientPlatform::Linux,
        4 => ClientPlatform::Android,
        5 => ClientPlatform::Ios,
        _ => return Err(format!("invalid platform: {v}")),
    })
}

fn button(v: u8) -> Result<Button, String> {
    Ok(match v {
        0 => Button::Left,
        1 => Button::Right,
        2 => Button::Middle,
        _ => return Err(format!("invalid mouse button: {v}")),
    })
}

fn touch_phase(v: u8) -> Result<TouchPhase, String> {
    Ok(match v {
        0 => TouchPhase::Began,
        1 => TouchPhase::Moved,
        2 => TouchPhase::Ended,
        3 => TouchPhase::Cancelled,
        _ => return Err(format!("invalid touch phase: {v}")),
    })
}

fn pack<T: serde::Serialize>(v: &T) -> Vec<u8> {
    // The wire bodies are tiny and serialization here is infallible for our
    // owned values; surface the rare error as an empty buffer the caller rejects.
    postcard::to_stdvec(v).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Upstream: ClientHello + Input encoders. Each returns the `postcard` body the
// host expects; the WS layer ships it as one binary message (no length prefix).
// ---------------------------------------------------------------------------

/// Encode a [`ClientHello`]. `capture_mode_id` / `platform_id` are the u8 codes
/// above. A browser passes `platform_id = 0` (Unknown) — it's not a native
/// platform.
#[wasm_bindgen]
pub fn encode_hello(
    protocol_version: u32,
    width: u32,
    height: u32,
    capture_mode_id: u8,
    platform_id: u8,
    pin: u32,
) -> Result<Vec<u8>, String> {
    Ok(pack(&ClientHello {
        protocol_version,
        width,
        height,
        capture_mode: capture_mode(capture_mode_id)?,
        platform: platform(platform_id)?,
        pin,
    }))
}

/// The protocol version this shim was built against — so the TS side never
/// hard-codes a number that can drift from `crates/protocol`.
#[wasm_bindgen]
pub fn protocol_version() -> u32 {
    protocol::PROTOCOL_VERSION
}

#[wasm_bindgen]
pub fn encode_mouse_move(x: f32, y: f32) -> Vec<u8> {
    pack(&Input::MouseMove { x, y })
}

#[wasm_bindgen]
pub fn encode_mouse_move_relative(dx: f32, dy: f32) -> Vec<u8> {
    pack(&Input::MouseMoveRelative { dx, dy })
}

#[wasm_bindgen]
pub fn encode_mouse_button(button_id: u8, pressed: bool) -> Result<Vec<u8>, String> {
    Ok(pack(&Input::MouseButton { button: button(button_id)?, pressed }))
}

#[wasm_bindgen]
pub fn encode_scroll(dx: f32, dy: f32) -> Vec<u8> {
    pack(&Input::Scroll { dx, dy })
}

#[wasm_bindgen]
pub fn encode_key(code: u32, pressed: bool) -> Vec<u8> {
    pack(&Input::Key { code, pressed })
}

#[wasm_bindgen]
pub fn encode_touch(id: u32, phase_id: u8, x: f32, y: f32) -> Result<Vec<u8>, String> {
    Ok(pack(&Input::Touch { id, phase: touch_phase(phase_id)?, x, y }))
}

#[wasm_bindgen]
pub fn encode_pinch(scale: f32) -> Vec<u8> {
    pack(&Input::Gesture(Gesture::Pinch { scale }))
}

#[wasm_bindgen]
pub fn encode_secondary_click(x: f32, y: f32) -> Vec<u8> {
    pack(&Input::Gesture(Gesture::SecondaryClick { x, y }))
}

#[wasm_bindgen]
pub fn encode_text(text: &str) -> Vec<u8> {
    pack(&Input::Text { text: text.to_string() })
}

// ---------------------------------------------------------------------------
// Downstream: decode a Message, exposed via typed getters. Byte buffers return
// as `Uint8Array` (wasm-bindgen maps `Vec<u8>`), so frame data never round-trips
// through a JS number array.
// ---------------------------------------------------------------------------

/// A decoded downstream [`Message`]. Read [`kind`](DecodedMessage::kind) first,
/// then the getters relevant to that variant (others return `undefined`/0).
#[wasm_bindgen]
pub struct DecodedMessage {
    inner: Message,
}

#[wasm_bindgen]
impl DecodedMessage {
    /// `"StreamStart" | "Frame" | "Snapshot" | "HostInfo" | "WindowList"`.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        match self.inner {
            Message::StreamStart { .. } => "StreamStart",
            Message::Frame { .. } => "Frame",
            Message::Snapshot { .. } => "Snapshot",
            Message::HostInfo { .. } => "HostInfo",
            Message::WindowList { .. } => "WindowList",
        }
        .to_string()
    }

    /// StreamStart / Snapshot width.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        match self.inner {
            Message::StreamStart { width, .. } | Message::Snapshot { width, .. } => width,
            _ => 0,
        }
    }

    /// StreamStart / Snapshot height.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        match self.inner {
            Message::StreamStart { height, .. } | Message::Snapshot { height, .. } => height,
            _ => 0,
        }
    }

    /// StreamStart codec: `"H264"` or `"Hevc"` (else `undefined`).
    #[wasm_bindgen(getter)]
    pub fn codec(&self) -> Option<String> {
        match self.inner {
            Message::StreamStart { codec, .. } => Some(
                match codec {
                    Codec::H264 => "H264",
                    Codec::Hevc => "Hevc",
                }
                .to_string(),
            ),
            _ => None,
        }
    }

    /// Number of parameter sets (SPS/PPS) in a StreamStart.
    #[wasm_bindgen(getter)]
    pub fn parameter_set_count(&self) -> usize {
        match &self.inner {
            Message::StreamStart { parameter_sets, .. } => parameter_sets.len(),
            _ => 0,
        }
    }

    /// The i-th parameter set (raw NAL bytes), or `undefined` if out of range.
    pub fn parameter_set(&self, i: usize) -> Option<Vec<u8>> {
        match &self.inner {
            Message::StreamStart { parameter_sets, .. } => parameter_sets.get(i).cloned(),
            _ => None,
        }
    }

    /// Frame keyframe flag.
    #[wasm_bindgen(getter)]
    pub fn keyframe(&self) -> bool {
        matches!(self.inner, Message::Frame { keyframe: true, .. })
    }

    /// Frame presentation timestamp in microseconds (for `EncodedVideoChunk`),
    /// computed from `pts_value` / `pts_timescale`. 0 for non-frames.
    #[wasm_bindgen(getter)]
    pub fn timestamp_micros(&self) -> f64 {
        match self.inner {
            Message::Frame { pts_value, pts_timescale, .. } if pts_timescale != 0 => {
                pts_value as f64 * 1_000_000.0 / f64::from(pts_timescale)
            }
            _ => 0.0,
        }
    }

    /// Frame / Snapshot payload bytes (AVCC for a Frame, JPEG for a Snapshot).
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Option<Vec<u8>> {
        match &self.inner {
            Message::Frame { data, .. } | Message::Snapshot { data, .. } => Some(data.clone()),
            _ => None,
        }
    }

    /// Snapshot slot (-1 prev / 0 current / +1 next). 0 for non-snapshots.
    #[wasm_bindgen(getter)]
    pub fn slot(&self) -> i32 {
        match self.inner {
            Message::Snapshot { slot, .. } => slot,
            _ => 0,
        }
    }

    /// HostInfo OS tag (`"windows"`/`"macos"`/`"linux"`), else `undefined`.
    #[wasm_bindgen(getter)]
    pub fn os(&self) -> Option<String> {
        match &self.inner {
            Message::HostInfo { os, .. } => Some(os.clone()),
            _ => None,
        }
    }

    /// HostInfo machine name, else `undefined`.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> Option<String> {
        match &self.inner {
            Message::HostInfo { name, .. } => Some(name.clone()),
            _ => None,
        }
    }

    /// Number of windows in a WindowList.
    #[wasm_bindgen(getter)]
    pub fn window_count(&self) -> usize {
        match &self.inner {
            Message::WindowList { windows } => windows.len(),
            _ => 0,
        }
    }

    /// The i-th window's opaque host id (as f64; ids fit comfortably).
    pub fn window_id(&self, i: usize) -> Option<f64> {
        match &self.inner {
            Message::WindowList { windows } => {
                windows.get(i).map(|(id, _)| *id as f64)
            }
            _ => None,
        }
    }

    /// The i-th window's title.
    pub fn window_title(&self, i: usize) -> Option<String> {
        match &self.inner {
            Message::WindowList { windows } => windows.get(i).map(|(_, t)| t.clone()),
            _ => None,
        }
    }
}

/// Decode one downstream `Message` body (the bytes of a single WS binary
/// message). Errors if the bytes aren't a valid `Message`.
#[wasm_bindgen]
pub fn decode_message(bytes: &[u8]) -> Result<DecodedMessage, String> {
    let inner: Message =
        postcard::from_bytes(bytes).map_err(|e| format!("decode failed: {e}"))?;
    Ok(DecodedMessage { inner })
}

// ---------------------------------------------------------------------------
// WebCodecs config helpers — build the H.264 decoder config from the raw SPS/PPS
// NALs in StreamStart. The host emits AVCC frames, so `avcC` mode feeds frames
// straight through with no Annex-B conversion (see docs/M7-browser-client.md).
// ---------------------------------------------------------------------------

/// The WebCodecs `codec` string (`"avc1.PPCCLL"`) from an SPS NAL: profile_idc /
/// constraint_flags / level_idc are the 3 bytes after the 1-byte NAL header.
#[wasm_bindgen]
pub fn avc_codec_string(sps: &[u8]) -> Result<String, String> {
    if sps.len() < 4 {
        return Err("SPS too short for a codec string".to_string());
    }
    Ok(format!("avc1.{:02x}{:02x}{:02x}", sps[1], sps[2], sps[3]))
}

/// Build the `avcDecoderConfigurationRecord` (the `avcC` box) for
/// `VideoDecoder.configure({ description })` from one SPS + one PPS NAL.
#[wasm_bindgen]
pub fn avcc_description(sps: &[u8], pps: &[u8]) -> Result<Vec<u8>, String> {
    if sps.len() < 4 {
        return Err("SPS too short".to_string());
    }
    let sps_len = u16::try_from(sps.len()).map_err(|_| "SPS too long".to_string())?;
    let pps_len = u16::try_from(pps.len()).map_err(|_| "PPS too long".to_string())?;
    let mut out = Vec::with_capacity(11 + sps.len() + pps.len());
    out.extend_from_slice(&[
        1,      // configurationVersion
        sps[1], // AVCProfileIndication
        sps[2], // profile_compatibility
        sps[3], // AVCLevelIndication
        0xff,   // 6 reserved bits + lengthSizeMinusOne = 3 (4-byte NAL lengths)
        0xe1,   // 3 reserved bits + numOfSequenceParameterSets = 1
    ]);
    out.extend_from_slice(&sps_len.to_be_bytes());
    out.extend_from_slice(sps);
    out.push(1); // numOfPictureParameterSets
    out.extend_from_slice(&pps_len.to_be_bytes());
    out.extend_from_slice(pps);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Round-trip every upstream encoder through the canonical `postcard` decoder
    // — guards the u8→enum mappings and that each fn builds the right variant.
    #[test]
    fn upstream_encoders_decode_back_to_the_right_input() {
        let cases: Vec<(Vec<u8>, Input)> = vec![
            (encode_mouse_move(0.25, 0.75), Input::MouseMove { x: 0.25, y: 0.75 }),
            (encode_mouse_move_relative(-3.0, 8.0), Input::MouseMoveRelative { dx: -3.0, dy: 8.0 }),
            (encode_mouse_button(1, true).unwrap(), Input::MouseButton { button: Button::Right, pressed: true }),
            (encode_scroll(-1.0, 2.5), Input::Scroll { dx: -1.0, dy: 2.5 }),
            (encode_key(0x04, true), Input::Key { code: 0x04, pressed: true }),
            (encode_touch(2, 1, 0.1, 0.2).unwrap(), Input::Touch { id: 2, phase: TouchPhase::Moved, x: 0.1, y: 0.2 }),
            (encode_pinch(1.5), Input::Gesture(Gesture::Pinch { scale: 1.5 })),
            (encode_secondary_click(0.4, 0.9), Input::Gesture(Gesture::SecondaryClick { x: 0.4, y: 0.9 })),
            (encode_text("héllo 世界"), Input::Text { text: "héllo 世界".to_string() }),
        ];
        for (bytes, expected) in cases {
            let got: Input = postcard::from_bytes(&bytes).unwrap();
            assert_eq!(got, expected);
        }
    }

    #[test]
    fn hello_round_trips_and_rejects_bad_codes() {
        let bytes = encode_hello(protocol::PROTOCOL_VERSION, 2560, 1440, 1, 0, 4321).unwrap();
        let got: ClientHello = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(got.capture_mode, CaptureMode::MirrorPrimary);
        assert_eq!(got.platform, ClientPlatform::Unknown);
        assert_eq!((got.width, got.height, got.pin), (2560, 1440, 4321));
        assert!(encode_hello(10, 1, 1, 99, 0, 0).is_err()); // bad capture mode
        assert!(encode_hello(10, 1, 1, 0, 99, 0).is_err()); // bad platform
    }

    #[test]
    fn decode_message_exposes_streamstart_and_frame_fields() {
        let start = postcard::to_stdvec(&Message::StreamStart {
            width: 1920,
            height: 1080,
            codec: Codec::H264,
            parameter_sets: vec![vec![0x67, 0x42, 0xc0, 0x1f], vec![0x68, 0xce]],
        })
        .unwrap();
        let m = decode_message(&start).unwrap();
        assert_eq!(m.kind(), "StreamStart");
        assert_eq!((m.width(), m.height()), (1920, 1080));
        assert_eq!(m.codec().as_deref(), Some("H264"));
        assert_eq!(m.parameter_set_count(), 2);
        assert_eq!(m.parameter_set(0).unwrap(), vec![0x67, 0x42, 0xc0, 0x1f]);

        let frame = postcard::to_stdvec(&Message::Frame {
            pts_value: 60,
            pts_timescale: 60,
            keyframe: true,
            data: vec![0xde, 0xad],
        })
        .unwrap();
        let m = decode_message(&frame).unwrap();
        assert_eq!(m.kind(), "Frame");
        assert!(m.keyframe());
        assert_eq!(m.timestamp_micros(), 1_000_000.0); // 60/60 s = 1e6 µs
        assert_eq!(m.data().unwrap(), vec![0xde, 0xad]);
    }

    #[test]
    fn avc_helpers_match_the_known_layout() {
        let sps = [0x67, 0x42, 0xc0, 0x1f];
        let pps = [0x68, 0xce, 0x3c, 0x80];
        assert_eq!(avc_codec_string(&sps).unwrap(), "avc1.42c01f");
        let avcc = avcc_description(&sps, &pps).unwrap();
        assert_eq!(
            avcc,
            vec![
                1, 0x42, 0xc0, 0x1f, 0xff, // header
                0xe1, 0x00, 0x04, 0x67, 0x42, 0xc0, 0x1f, // 1 SPS, len 4
                0x01, 0x00, 0x04, 0x68, 0xce, 0x3c, 0x80, // 1 PPS, len 4
            ]
        );
        assert!(avc_codec_string(&[0x67]).is_err());
    }
}
