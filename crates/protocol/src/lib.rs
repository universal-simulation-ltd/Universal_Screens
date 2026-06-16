//! ExtenderScreen wire protocol: message types and framing shared by host and client.
//!
//! Portable, platform-agnostic — no macOS/Windows dependencies live here so the
//! same protocol code compiles on every host and client.

use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Protocol version negotiated during the connection handshake. Bumped to 2 in
/// M5: [`ClientHello`] gained a `capture_mode` field and [`Input`] gained the
/// touch/gesture/text variants. Bumped in M6: v3 added [`Message::Snapshot`] (a
/// still-image preview an input-only host pushes to a clicker), v4 added
/// [`Message::HostInfo`] (the host's OS + name, for labelling saved connections).
/// v5 added [`Input::ScanDeck`] and a `slot` on [`Message::Snapshot`], for the
/// clicker's next-slide look-ahead. v6 added [`Message::WindowList`] plus
/// [`Input::ListWindows`] / [`Input::FocusWindow`], so a clicker can refocus the
/// host window that should receive its keys. The host warns (but proceeds) on a
/// version skew.
pub const PROTOCOL_VERSION: u32 = 6;

/// Video codec used for the encoded frame stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Codec {
    H264,
    Hevc,
}

/// How the client wants the host to source the streamed frames. Defaults to the
/// existing "extend" behaviour, so a client that doesn't care gets a virtual
/// second screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CaptureMode {
    /// Create + capture a virtual display sized to the client (the "extend" use
    /// case from M3). The host sizes the virtual display to the client's hello.
    #[default]
    VirtualDisplay,
    /// Capture the host's existing primary display and route input to it — i.e.
    /// remotely control the real desktop (the "control" use case from M5).
    MirrorPrimary,
    /// Accept input only — the host injects events but streams no video. For a
    /// presentation clicker, where the client just sends keystrokes and wants no
    /// battery/bandwidth cost (M6c). A host that doesn't support it can treat it
    /// like `MirrorPrimary` and simply stream anyway. Appended last so the
    /// existing `postcard` discriminants stay stable.
    ControlOnly,
}

/// A message on the host -> client stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    /// Sent once at stream start: geometry, codec, and the codec parameter
    /// sets (SPS/PPS for H.264) the client needs to build its decoder.
    StreamStart {
        width: u32,
        height: u32,
        codec: Codec,
        parameter_sets: Vec<Vec<u8>>,
    },
    /// One encoded frame (AVCC: length-prefixed NAL units).
    Frame {
        pts_value: i64,
        pts_timescale: i32,
        keyframe: bool,
        data: Vec<u8>,
    },
    /// A standalone still image of the host screen (`data` is JPEG-encoded). Used
    /// by an input-only host (the clicker's [`CaptureMode::ControlOnly`]) to push a
    /// lightweight slide preview without a continuous video stream — slides are
    /// static, so a still refreshed on each change is enough. Appended last so the
    /// existing `StreamStart`/`Frame` `postcard` discriminants stay stable.
    ///
    /// `slot` says which slide this preview is, relative to the current position:
    /// `0` = current (a live capture), `-1` = previous, `+1` = next (both from the
    /// host's pre-scan cache). An empty `data` for an adjacent slot means "no slide
    /// there" (e.g. at the start/end), so the client clears that tile.
    Snapshot {
        width: u32,
        height: u32,
        slot: i32,
        data: Vec<u8>,
    },
    /// The host's identity, sent once right after the handshake so a client can
    /// label and icon a saved connection for quick reconnect. `os` is a short
    /// lowercase tag (`"windows"`, `"macos"`, `"linux"`); `name` is the host's
    /// machine name. Appended last to keep existing discriminants stable.
    HostInfo {
        os: String,
        name: String,
    },
    /// The host's open top-level windows as `(id, title)` pairs, so a clicker can
    /// pick one to bring to the foreground (its keystrokes go to whatever's
    /// focused). `id` is an opaque host handle echoed back in [`Input::FocusWindow`].
    /// Sent on connect and on [`Input::ListWindows`].
    WindowList {
        windows: Vec<(i64, String)>,
    },
}

/// The first message a client sends upstream, immediately after connecting and
/// before any [`Input`]. Carries the protocol version (so the host can detect a
/// mismatch), the client's full panel resolution in physical pixels (so the host
/// can size a virtual display to match), and the requested [`CaptureMode`].
///
/// Note: this struct is `postcard`-encoded as its fields in order, so the
/// `capture_mode` field added in protocol v2 is *not* wire-compatible with a v1
/// peer — both ends must be built from the same protocol version. The version
/// field lets the host detect and warn about a skew.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientHello {
    pub protocol_version: u32,
    pub width: u32,
    pub height: u32,
    pub capture_mode: CaptureMode,
}

/// A client -> host input event. Pointer coordinates are normalized to the
/// streamed frame (`[0, 1]` from the top-left), so they're independent of the
/// client window size and the host display resolution.
///
/// The touch/gesture/text variants were added in protocol v2 for mobile clients.
/// They're appended after `Key` so the existing variants keep their `postcard`
/// discriminants; a v1 host simply never receives the new ones. `Input` is no
/// longer `Copy` because [`Input::Text`] owns a `String`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Input {
    /// Absolute pointer position within the streamed frame.
    MouseMove { x: f32, y: f32 },
    /// Relative pointer motion in points (used in pointer-locked control mode).
    MouseMoveRelative { dx: f32, dy: f32 },
    /// A mouse button changed state.
    MouseButton { button: Button, pressed: bool },
    /// Wheel scroll in lines (positive `dy` scrolls up, positive `dx` right).
    Scroll { dx: f32, dy: f32 },
    /// A key changed state. `code` is a platform-neutral key identifier the host
    /// maps to its OS keycode.
    Key { code: u32, pressed: bool },
    /// A touch/pen contact changed. `id` distinguishes simultaneous fingers;
    /// `x`/`y` are normalized to the frame like [`Input::MouseMove`]. The host
    /// drives the pointer with these, treating a single contact's begin/move/end
    /// as a left press/drag/release at the contact point.
    Touch { id: u32, phase: TouchPhase, x: f32, y: f32 },
    /// A high-level gesture pre-classified on the client (where the touch history
    /// lives). See [`Gesture`].
    Gesture(Gesture),
    /// Committed Unicode text from a soft keyboard / IME, which can't be expressed
    /// as physical [`Input::Key`] scancodes. The host synthesizes a keystroke
    /// carrying the string.
    Text { text: String },
    /// Ask a clicker host to pre-scan the open document into its slide cache, so it
    /// can preview the *upcoming* slide: the host pages through to the end and
    /// returns to the start, capturing each page. A host that doesn't support
    /// look-ahead ignores it. Appended last to keep existing discriminants stable.
    ScanDeck,
    /// Ask the host to (re)send its [`Message::WindowList`].
    ListWindows,
    /// Bring the host window with this id (from [`Message::WindowList`]) to the
    /// foreground, so subsequent keystrokes land in it.
    FocusWindow { id: i64 },
}

/// The lifecycle phase of a touch contact (mirrors the common touch APIs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchPhase {
    Began,
    Moved,
    Ended,
    Cancelled,
}

/// A high-level gesture, classified on the client and sent ready to act on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Gesture {
    /// Pinch scale factor relative to the gesture's start (`1.0` = unchanged).
    Pinch { scale: f32 },
    /// A secondary-click request (e.g. long-press) at a normalized point.
    SecondaryClick { x: f32, y: f32 },
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// Write a length-prefixed, postcard-encoded value to a stream. Used for every
/// framed message in both directions ([`Message`] downstream, [`Input`] upstream).
///
/// # Errors
/// Returns an error if encoding fails or the underlying writer errors.
pub fn write_framed<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    let body = postcard::to_stdvec(msg).map_err(io::Error::other)?;
    let len = u32::try_from(body.len()).map_err(|_| io::Error::other("message too large"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(&body)?;
    Ok(())
}

/// Read a length-prefixed, postcard-encoded value from a stream.
///
/// # Errors
/// Returns an error if the stream ends, the length is invalid, or decoding fails.
pub fn read_framed<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    postcard::from_bytes(&body).map_err(io::Error::other)
}

/// Report whether an AVCC bitstream contains a keyframe (an IRAP access unit):
/// an H.264 IDR slice (NAL type 5) or an HEVC IRAP NAL (types 16..=23).
///
/// `data` is the length-prefixed NAL stream VideoToolbox emits (4-byte
/// big-endian prefixes). The host uses this to set [`Message::Frame`]'s
/// `keyframe` flag; the client uses it to know when it can start decoding.
/// Malformed or truncated input reports `false` (unknown ⇒ not a keyframe).
#[must_use]
pub fn is_keyframe(codec: Codec, data: &[u8]) -> bool {
    nal_units(data).any(|nal| match codec {
        // H.264: nal_unit_type is the low 5 bits; 5 = IDR slice.
        Codec::H264 => nal.first().is_some_and(|&b| b & 0x1F == 5),
        // HEVC: nal_unit_type is bits 1..=6 of the first header byte; the
        // IRAP range 16..=23 (BLA/IDR/CRA) are the random-access pictures.
        Codec::Hevc => nal.first().is_some_and(|&b| (16..=23).contains(&((b >> 1) & 0x3F))),
    })
}

/// Append the NAL units of an AVCC buffer (4-byte big-endian length prefixes, as
/// VideoToolbox emits) to `out` in Annex-B form — each NAL prefixed with the
/// start code `00 00 00 01`. This is the form software decoders like openh264
/// expect, so a cross-platform client can feed frames straight through.
pub fn append_annex_b(out: &mut Vec<u8>, avcc: &[u8]) {
    for nal in nal_units(avcc) {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
    }
}

/// Build an Annex-B stream from the raw parameter-set NALs (SPS/PPS) carried in
/// [`Message::StreamStart`], each prefixed with a start code — used to prime a
/// decoder before the first frame.
#[must_use]
pub fn annex_b_parameter_sets(parameter_sets: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for nal in parameter_sets {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
    }
    out
}

/// Iterate the NAL unit payloads in an AVCC buffer (each prefixed by a 4-byte
/// big-endian length). Stops at the first truncated or zero-length prefix.
fn nal_units(mut data: &[u8]) -> impl Iterator<Item = &[u8]> {
    core::iter::from_fn(move || {
        let (len_bytes, rest) = data.split_at_checked(4)?;
        let len = u32::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
        let (nal, tail) = rest.split_at_checked(len)?;
        if nal.is_empty() {
            return None;
        }
        data = tail;
        Some(nal)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an AVCC buffer (4-byte big-endian length prefix per NAL) from a
    /// list of raw NAL unit payloads.
    fn avcc(nals: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for nal in nals {
            out.extend_from_slice(&u32::try_from(nal.len()).unwrap().to_be_bytes());
            out.extend_from_slice(nal);
        }
        out
    }

    #[test]
    fn message_roundtrip_over_a_buffer() {
        let msgs = vec![
            Message::StreamStart {
                width: 1920,
                height: 1080,
                codec: Codec::H264,
                parameter_sets: vec![vec![1, 2, 3, 4], vec![5, 6]],
            },
            Message::Frame {
                pts_value: 7,
                pts_timescale: 60,
                keyframe: true,
                data: vec![0, 1, 2, 3, 4, 5],
            },
            Message::Frame {
                pts_value: 8,
                pts_timescale: 60,
                keyframe: false,
                data: vec![9, 9, 9],
            },
            Message::Snapshot {
                width: 960,
                height: 540,
                slot: 0,
                data: vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3],
            },
            Message::Snapshot { width: 0, height: 0, slot: -1, data: vec![] },
            Message::Snapshot { width: 480, height: 270, slot: 1, data: vec![9, 9] },
            Message::HostInfo {
                os: "windows".to_string(),
                name: "DESKTOP-ABC123".to_string(),
            },
            Message::WindowList {
                windows: vec![(12345, "slides.pdf — Edge".to_string()), (67890, "Notes".to_string())],
            },
        ];

        let mut buf = Vec::new();
        for m in &msgs {
            write_framed(&mut buf, m).unwrap();
        }

        let mut cursor = io::Cursor::new(buf);
        for expected in &msgs {
            let got: Message = read_framed(&mut cursor).unwrap();
            assert_eq!(&got, expected);
        }
    }

    #[test]
    fn input_messages_round_trip() {
        let inputs = vec![
            Input::MouseMove { x: 0.25, y: 0.75 },
            Input::MouseMoveRelative { dx: -3.5, dy: 8.0 },
            Input::MouseButton { button: Button::Left, pressed: true },
            Input::MouseButton { button: Button::Right, pressed: false },
            Input::Scroll { dx: -1.0, dy: 2.5 },
            Input::Key { code: 42, pressed: true },
            Input::Touch { id: 1, phase: TouchPhase::Began, x: 0.1, y: 0.2 },
            Input::Touch { id: 1, phase: TouchPhase::Moved, x: 0.3, y: 0.4 },
            Input::Touch { id: 1, phase: TouchPhase::Ended, x: 0.5, y: 0.6 },
            Input::Gesture(Gesture::Pinch { scale: 1.5 }),
            Input::Gesture(Gesture::SecondaryClick { x: 0.4, y: 0.9 }),
            Input::Text { text: "héllo, 世界 🌍".to_string() },
            Input::ScanDeck,
            Input::ListWindows,
            Input::FocusWindow { id: 1234567890 },
        ];

        let mut buf = Vec::new();
        for input in &inputs {
            write_framed(&mut buf, input).unwrap();
        }

        let mut cursor = io::Cursor::new(buf);
        for expected in &inputs {
            let got: Input = read_framed(&mut cursor).unwrap();
            assert_eq!(&got, expected);
        }
    }

    #[test]
    fn client_hello_round_trips_with_capture_mode() {
        for mode in [
            CaptureMode::VirtualDisplay,
            CaptureMode::MirrorPrimary,
            CaptureMode::ControlOnly,
        ] {
            let hello = ClientHello {
                protocol_version: PROTOCOL_VERSION,
                width: 2560,
                height: 1440,
                capture_mode: mode,
            };
            let mut buf = Vec::new();
            write_framed(&mut buf, &hello).unwrap();
            let got: ClientHello = read_framed(&mut io::Cursor::new(buf)).unwrap();
            assert_eq!(got, hello);
        }
    }

    #[test]
    fn capture_mode_defaults_to_virtual_display() {
        assert_eq!(CaptureMode::default(), CaptureMode::VirtualDisplay);
    }

    #[test]
    fn h264_idr_access_unit_is_a_keyframe() {
        // SPS (type 7), PPS (type 8), IDR slice (type 5).
        let au = avcc(&[&[0x67, 0x42, 0x00], &[0x68, 0xce], &[0x65, 0x88, 0x84]]);
        assert!(is_keyframe(Codec::H264, &au));
    }

    #[test]
    fn h264_non_idr_slice_is_not_a_keyframe() {
        // A single non-IDR coded slice (type 1).
        let au = avcc(&[&[0x41, 0x9a, 0x00]]);
        assert!(!is_keyframe(Codec::H264, &au));
    }

    #[test]
    fn hevc_idr_is_a_keyframe_but_trailing_picture_is_not() {
        // IDR_W_RADL is NAL type 19 -> first header byte (19 << 1) = 0x26.
        assert!(is_keyframe(Codec::Hevc, &avcc(&[&[0x26, 0x01, 0x00]])));
        // TRAIL_R is type 1 -> 0x02; not in the IRAP range.
        assert!(!is_keyframe(Codec::Hevc, &avcc(&[&[0x02, 0x01, 0x00]])));
    }

    #[test]
    fn keyframe_in_a_later_nal_is_still_found() {
        let au = avcc(&[&[0x06, 0x00], &[0x65, 0x88]]); // SEI (type 6) then IDR (type 5)
        assert!(is_keyframe(Codec::H264, &au));
    }

    #[test]
    fn malformed_or_truncated_streams_report_not_a_keyframe() {
        assert!(!is_keyframe(Codec::H264, &[]));
        assert!(!is_keyframe(Codec::H264, &[0, 0, 0])); // shorter than one length prefix
        // Length prefix claims 9 bytes but only one follows — stop, don't peek.
        assert!(!is_keyframe(Codec::H264, &[0, 0, 0, 9, 0x65]));
    }

    #[test]
    fn avcc_converts_to_annex_b() {
        let stream = avcc(&[&[0x67, 0x42], &[0x65, 0x88, 0x84]]);
        let mut out = Vec::new();
        append_annex_b(&mut out, &stream);
        assert_eq!(
            out,
            vec![0, 0, 0, 1, 0x67, 0x42, 0, 0, 0, 1, 0x65, 0x88, 0x84]
        );
    }

    #[test]
    fn parameter_sets_become_annex_b() {
        let out = annex_b_parameter_sets(&[vec![0x67, 0x42], vec![0x68, 0xce]]);
        assert_eq!(out, vec![0, 0, 0, 1, 0x67, 0x42, 0, 0, 0, 1, 0x68, 0xce]);
    }
}
