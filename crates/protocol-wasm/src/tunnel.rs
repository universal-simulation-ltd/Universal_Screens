//! The encrypted browser leg: the PIN-keyed Noise tunnel and the protocol's own
//! framing, exposed to JavaScript.
//!
//! Until this existed, a browser tab was the one client that spoke the protocol
//! **in the clear** — TLS to the cloud rendezvous protected the wire, but the
//! relay itself could read every keystroke and every frame of the mirrored
//! screen. The obstacle was never that a browser can't do the cryptography; it
//! was that the tunnel lived inside a `TcpStream`. `transport::session` is that
//! same tunnel with the socket taken out, and this is its JS surface.
//!
//! ## What the JS has to do differently once encrypted
//!
//! The bridge stops re-framing (it becomes a byte pipe — see `Relay` in
//! `crates/web-bridge`), so the browser owns both jobs the bridge used to do:
//!
//! 1. **Add the 4-byte length prefix** to every outgoing protocol body
//!    ([`frame`]), because the host reads a *stream*, not messages.
//! 2. **Re-assemble** the incoming stream into whole bodies ([`FrameReader`]),
//!    because a Noise record boundary has nothing to do with a message boundary.
//!
//! ⚠️ Neither is optional and neither is visible in a small test: one WS message
//! usually happens to carry exactly one record carrying exactly one frame, so
//! code that ignores both looks fine right up until a 200 KB keyframe arrives.

use extender_transport::{Initiator, Session};
use wasm_bindgen::prelude::*;

/// A handshake in progress. Send [`Handshake::first_message`] as the first
/// thing on the connection, then hand the peer's reply to [`Handshake::finish`].
#[wasm_bindgen]
pub struct Handshake {
    initiator: Option<Initiator>,
    first: Vec<u8>,
}

#[wasm_bindgen]
impl Handshake {
    /// Start the `NNpsk0` handshake for `pin` (0 = no PIN, still encrypted).
    ///
    /// # Errors
    /// Returns the Noise error as a string; `wasm-bindgen` throws it.
    #[wasm_bindgen(constructor)]
    pub fn new(pin: u32) -> Result<Handshake, String> {
        let (initiator, first) = Initiator::start(pin).map_err(|e| e.to_string())?;
        Ok(Handshake { initiator: Some(initiator), first })
    }

    /// The bytes to send **before anything else** — the transport preamble plus
    /// the first handshake message. Sending anything ahead of these makes the
    /// host treat the connection as a legacy plaintext peer.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn first_message(&self) -> Vec<u8> {
        self.first.clone()
    }

    /// Complete the handshake from the peer's reply — the `u16`-LE-prefixed
    /// second handshake message, exactly as it arrived.
    ///
    /// ⚠️ **Wait until the whole message is present.** The reply arrives as a
    /// byte stream, so a caller must buffer until it holds `2 + len` bytes;
    /// handing over a partial reply is an error, not a retry.
    ///
    /// # Errors
    /// Returns an error if the reply is truncated or fails to authenticate — the
    /// latter is what a **wrong PIN** looks like, since the PIN keys the AEAD and
    /// there is no distinguishable "bad PIN" response to detect.
    pub fn finish(&mut self, reply: &[u8]) -> Result<Tunnel, String> {
        let initiator = self
            .initiator
            .take()
            .ok_or_else(|| "this handshake has already been finished".to_owned())?;
        let session = initiator.finish(reply).map_err(|e| e.to_string())?;
        Ok(Tunnel { session })
    }
}

/// A live tunnel: seal what you send, feed what arrives.
#[wasm_bindgen]
pub struct Tunnel {
    session: Session,
}

#[wasm_bindgen]
impl Tunnel {
    /// Encrypt one buffer into wire bytes. Send them as they are — splitting or
    /// merging is fine, reordering is not.
    ///
    /// # Errors
    /// Returns an error if the cipher state rejects the write.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        self.session.seal(plaintext).map_err(|e| e.to_string())
    }

    /// Feed received wire bytes in. Whole records are decrypted and buffered; a
    /// trailing partial record waits for the rest.
    ///
    /// # Errors
    /// Returns an error if a record fails to authenticate, which is fatal — the
    /// cipher state cannot resynchronise, so the connection has to be remade.
    pub fn feed(&mut self, wire: &[u8]) -> Result<(), String> {
        self.session.feed(wire).map_err(|e| e.to_string())
    }

    /// Take everything decrypted so far.
    #[must_use]
    pub fn take(&mut self) -> Vec<u8> {
        self.session.take_all()
    }

    /// How many decrypted bytes are waiting.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn available(&self) -> usize {
        self.session.available()
    }
}

/// Prefix a protocol body with its 4-byte little-endian length — what
/// `protocol::write_framed` puts on the wire, and what the bridge used to add on
/// the browser's behalf.
#[wasm_bindgen]
#[must_use]
pub fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// Re-assembles a byte stream into protocol message bodies.
///
/// ⚠️ Stateful on purpose. Downstream is a *stream*: one decrypted chunk may hold
/// three messages and half of a fourth, and a single 200 KB keyframe spans many
/// chunks. Treating a chunk as a message works for a hello and fails for video.
#[wasm_bindgen]
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

#[wasm_bindgen]
impl FrameReader {
    /// A reader with an empty buffer.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> FrameReader {
        FrameReader::default()
    }

    /// Add received bytes.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Take the next complete message body, or `undefined` when one has not
    /// fully arrived. Call it in a loop until it yields nothing.
    ///
    /// `next` on the JS side, `next_frame` in Rust: a bare `next` there reads as
    /// `Iterator::next`, which this is not (it can return `None` and then yield
    /// again once more bytes arrive).
    #[wasm_bindgen(js_name = next)]
    #[must_use]
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        let len_bytes = self.buf.get(..4)?;
        let len = u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]])
            as usize;
        if self.buf.len() < 4 + len {
            return None;
        }
        let body = self.buf[4..4 + len].to_vec();
        self.buf.drain(..4 + len);
        Some(body)
    }

    /// Bytes held that don't yet form a whole message — diagnostics, not flow
    /// control.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn pending(&self) -> usize {
        self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_prefixes_the_little_endian_length() {
        assert_eq!(frame(b"hi"), vec![2, 0, 0, 0, b'h', b'i']);
        assert_eq!(frame(&[]), vec![0, 0, 0, 0]);
    }

    #[test]
    fn a_reader_reassembles_messages_split_across_chunks() {
        let mut reader = FrameReader::new();
        let wire = [frame(b"first"), frame(b"second")].concat();
        // One byte at a time: the worst case a real stream can produce.
        let mut out = Vec::new();
        for byte in &wire {
            reader.push(&[*byte]);
            while let Some(body) = reader.next_frame() {
                out.push(body);
            }
        }
        assert_eq!(out, vec![b"first".to_vec(), b"second".to_vec()]);
        assert_eq!(reader.pending(), 0);
    }

    #[test]
    fn a_reader_yields_every_message_in_one_chunk() {
        let mut reader = FrameReader::new();
        reader.push(&[frame(b"a"), frame(b"bb"), frame(b"ccc")].concat());
        let mut out = Vec::new();
        while let Some(body) = reader.next_frame() {
            out.push(body);
        }
        assert_eq!(out, vec![b"a".to_vec(), b"bb".to_vec(), b"ccc".to_vec()]);
    }

    #[test]
    fn a_reader_holds_a_partial_message_rather_than_yielding_it() {
        let mut reader = FrameReader::new();
        reader.push(&frame(b"incomplete")[..7]);
        assert!(reader.next_frame().is_none());
        assert_eq!(reader.pending(), 7);
    }

    #[test]
    fn a_handshake_cannot_be_finished_twice() {
        let mut hs = Handshake::new(1234).unwrap();
        assert!(!hs.first_message().is_empty());
        // A garbage reply fails, but the initiator is consumed either way — the
        // second call must say so rather than panic on a `None`.
        assert!(hs.finish(&[0, 0]).is_err());
        let second = hs.finish(&[0, 0]).err().expect("a finished handshake cannot restart");
        assert!(second.contains("already been finished"), "got: {second}");
    }
}
