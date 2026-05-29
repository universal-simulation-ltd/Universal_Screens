//! ExtenderScreen wire protocol: message types and framing shared by host and client.
//!
//! Portable, platform-agnostic — no macOS/Windows dependencies live here so the
//! same protocol code compiles on every host and client.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// Protocol version negotiated during the connection handshake.
pub const PROTOCOL_VERSION: u32 = 1;

/// Video codec used for the encoded frame stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Codec {
    H264,
    Hevc,
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
}

/// Write a length-prefixed, postcard-encoded message to a stream.
///
/// # Errors
/// Returns an error if encoding fails or the underlying writer errors.
pub fn write_message<W: Write>(w: &mut W, msg: &Message) -> io::Result<()> {
    let body = postcard::to_stdvec(msg).map_err(io::Error::other)?;
    let len = u32::try_from(body.len()).map_err(|_| io::Error::other("message too large"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(&body)?;
    Ok(())
}

/// Read a length-prefixed, postcard-encoded message from a stream.
///
/// # Errors
/// Returns an error if the stream ends, the length is invalid, or decoding fails.
pub fn read_message<R: Read>(r: &mut R) -> io::Result<Message> {
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
        ];

        let mut buf = Vec::new();
        for m in &msgs {
            write_message(&mut buf, m).unwrap();
        }

        let mut cursor = io::Cursor::new(buf);
        for expected in &msgs {
            let got = read_message(&mut cursor).unwrap();
            assert_eq!(&got, expected);
        }
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
}
