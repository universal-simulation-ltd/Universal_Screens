//! The Noise tunnel with **no socket in it**: the initiator handshake and the
//! record layer, as pure byte-in / byte-out state machines.
//!
//! Everything in this file works on `&[u8]` and `Vec<u8>`, which is what makes it
//! usable from somewhere that has no `TcpStream` — specifically **a browser**,
//! where the carrier is a WebSocket and the only I/O primitive is "here is a
//! chunk of bytes" / "send this chunk of bytes". `crates/protocol-wasm` exposes
//! this to JavaScript; [`crate::SecureStream`] is the same code with a socket
//! bolted on.
//!
//! ⚠️ **One record layer, not two.** The framing here — a `u16` little-endian
//! length in front of each Noise message — is the wire format, so a second
//! implementation for the browser would be a second chance to get it subtly
//! wrong, with the failure showing up as an undecryptable stream rather than a
//! compile error. `SecureStream` was rewritten onto [`Session`] in the same
//! change that added this file, so there is exactly one.
//!
//! What is deliberately **not** here: the responder half. Hosts accept
//! connections and they all have sockets; only the *client* side needs to run
//! without one.

use std::collections::VecDeque;
use std::io;

use crate::{
    derive_psk, noise_err, noise_params, MAX_HANDSHAKE_MSG, MAX_PLAINTEXT, PREAMBLE,
};

/// An initiator handshake in progress: created by [`Initiator::start`], finished
/// by [`Initiator::finish`] once the peer's reply has arrived.
///
/// The two halves are separate calls because the caller owns the transport — a
/// socket can block on the reply, but a WebSocket has to return to the event loop
/// and come back when a message arrives.
pub struct Initiator {
    hs: snow::HandshakeState,
}

impl Initiator {
    /// Begin the `NNpsk0` handshake for `pin` (0 = no PIN, still encrypted).
    ///
    /// Returns the bytes to send **first, before anything else on the
    /// connection**: the [`PREAMBLE`] followed by the length-prefixed first
    /// handshake message. A host that speaks the legacy plaintext protocol
    /// detects that preamble and switches to the encrypted path; one that does
    /// not will read it as a malformed frame, which is why nothing may precede it.
    ///
    /// # Errors
    /// Returns an error if the Noise parameters or the initial message fail to
    /// build — neither depends on input, so in practice this is infallible.
    pub fn start(pin: u32) -> io::Result<(Self, Vec<u8>)> {
        let psk = derive_psk(pin);
        let mut hs = snow::Builder::new(noise_params()?)
            .psk(0, &psk)
            .build_initiator()
            .map_err(noise_err)?;

        let mut buf = [0u8; MAX_HANDSHAKE_MSG];
        let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;

        let mut out = Vec::with_capacity(PREAMBLE.len() + 2 + n);
        out.extend_from_slice(&PREAMBLE);
        out.extend_from_slice(&frame_handshake(&buf[..n])?);
        Ok((Self { hs }, out))
    }

    /// Consume the peer's reply — the length-prefixed second handshake message,
    /// exactly as it arrived — and produce the live [`Session`].
    ///
    /// # Errors
    /// Returns an error if `reply` is truncated, over-long, or fails to
    /// authenticate. ⚠️ **A wrong PIN fails here**, as an AEAD failure rather
    /// than a distinguishable "bad PIN" reply: the PSK is mixed into the
    /// handshake hash, so a mismatch simply cannot decrypt. Report it to a user
    /// as a wrong code; do not try to detect it any earlier.
    pub fn finish(mut self, reply: &[u8]) -> io::Result<Session> {
        let body = unframe_handshake(reply)?;
        let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
        self.hs.read_message(&body, &mut scratch).map_err(noise_err)?;
        Ok(Session::from_transport(self.hs.into_transport_mode().map_err(noise_err)?))
    }
}

/// A live Noise tunnel: [`Session::seal`] turns plaintext into wire bytes,
/// [`Session::feed`] turns wire bytes back into plaintext.
///
/// ⚠️ **Order is part of the cipher state.** Noise numbers its messages, so
/// records must be sealed in the order they are sent and fed in the order they
/// arrive. One `Session` therefore belongs to one direction-pair and must not be
/// driven from two threads without a lock (which is exactly what
/// [`crate::SecureStream`] does).
pub struct Session {
    transport: snow::TransportState,
    /// Decrypted bytes not yet taken by the caller — a Noise record can carry
    /// more than one protocol frame, and a caller may want fewer bytes than
    /// arrived.
    plain: VecDeque<u8>,
    /// Wire bytes of a record that has not fully arrived yet. A WebSocket
    /// delivers whole messages, but a TCP socket does not, and neither promises
    /// that a chunk boundary is a record boundary.
    partial: Vec<u8>,
}

impl Session {
    /// Wrap a completed Noise transport state.
    ///
    /// `pub(crate)` on purpose: the **responder** half of the handshake lives in
    /// `lib.rs` because only a host runs it, and only a host has a socket — but
    /// it must share this record layer, not grow a second one.
    pub(crate) fn from_transport(transport: snow::TransportState) -> Self {
        Self { transport, plain: VecDeque::new(), partial: Vec::new() }
    }

    /// Encrypt `plaintext` into wire bytes: one or more `u16`-LE-length-prefixed
    /// Noise records, split at [`MAX_PLAINTEXT`].
    ///
    /// # Errors
    /// Returns an error if the cipher state rejects the write (a nonce
    /// exhaustion, in practice unreachable).
    pub fn seal(&mut self, plaintext: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(plaintext.len() + 32);
        for chunk in plaintext.chunks(MAX_PLAINTEXT) {
            let mut ct = vec![0u8; chunk.len() + 16];
            let n = self.transport.write_message(chunk, &mut ct).map_err(noise_err)?;
            let clen = u16::try_from(n).map_err(|_| noise_err("ciphertext too large"))?;
            out.extend_from_slice(&clen.to_le_bytes());
            out.extend_from_slice(&ct[..n]);
        }
        Ok(out)
    }

    /// Feed wire bytes in. Any records completed by this chunk are decrypted and
    /// buffered; a trailing partial record is kept until the rest arrives.
    ///
    /// # Errors
    /// Returns an error if a record fails to authenticate — which is fatal for
    /// the tunnel, since the cipher state cannot be resynchronised.
    pub fn feed(&mut self, wire: &[u8]) -> io::Result<()> {
        self.partial.extend_from_slice(wire);
        let mut at = 0usize;
        while let Some(len_bytes) = self.partial.get(at..at + 2) {
            let clen = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
            let Some(ct) = self.partial.get(at + 2..at + 2 + clen) else { break };
            let mut pt = vec![0u8; clen];
            let n = self.transport.read_message(ct, &mut pt).map_err(noise_err)?;
            self.plain.extend(pt[..n].iter().copied());
            at += 2 + clen;
        }
        self.partial.drain(..at);
        Ok(())
    }

    /// How much decrypted plaintext is waiting.
    #[must_use]
    pub fn available(&self) -> usize {
        self.plain.len()
    }

    /// True when bytes of an incomplete record are held. ⚠️ At end-of-stream this
    /// means the peer was **cut off mid-record** — a truncated stream, not a
    /// clean close.
    #[must_use]
    pub fn has_partial_record(&self) -> bool {
        !self.partial.is_empty()
    }

    /// Move up to `buf.len()` decrypted bytes out, returning how many.
    pub fn take(&mut self, buf: &mut [u8]) -> usize {
        let n = buf.len().min(self.plain.len());
        for slot in buf.iter_mut().take(n) {
            *slot = self.plain.pop_front().expect("plain is non-empty");
        }
        n
    }

    /// Move **all** decrypted bytes out. The shape a message-oriented caller (a
    /// browser reading WebSocket messages) wants.
    pub fn take_all(&mut self) -> Vec<u8> {
        self.plain.drain(..).collect()
    }
}

/// `u16`-LE length + body, for one handshake message.
fn frame_handshake(body: &[u8]) -> io::Result<Vec<u8>> {
    let len = u16::try_from(body.len()).map_err(|_| noise_err("handshake message too large"))?;
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(body);
    Ok(out)
}

/// The inverse of [`frame_handshake`], rejecting a truncated or over-long message.
fn unframe_handshake(msg: &[u8]) -> io::Result<Vec<u8>> {
    let len_bytes = msg.get(..2).ok_or_else(|| noise_err("handshake reply is truncated"))?;
    let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
    if len > MAX_HANDSHAKE_MSG {
        return Err(noise_err("handshake message exceeds the maximum size"));
    }
    let body = msg.get(2..2 + len).ok_or_else(|| noise_err("handshake reply is truncated"))?;
    Ok(body.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive both halves in memory, with no socket anywhere, by running the
    /// responder side by hand — the shape the browser will use.
    fn paired(pin_client: u32, pin_host: u32) -> io::Result<(Session, snow::TransportState)> {
        let (init, first) = Initiator::start(pin_client)?;
        assert_eq!(&first[..PREAMBLE.len()], &PREAMBLE[..], "the preamble leads");

        let psk = derive_psk(pin_host);
        let mut responder = snow::Builder::new(noise_params()?)
            .psk(0, &psk)
            .build_responder()
            .map_err(noise_err)?;
        let msg1 = unframe_handshake(&first[PREAMBLE.len()..])?;
        let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
        responder.read_message(&msg1, &mut scratch).map_err(noise_err)?;
        let mut buf = [0u8; MAX_HANDSHAKE_MSG];
        let n = responder.write_message(&[], &mut buf).map_err(noise_err)?;
        let reply = frame_handshake(&buf[..n])?;

        let session = init.finish(&reply)?;
        Ok((session, responder.into_transport_mode().map_err(noise_err)?))
    }

    /// Decrypt one sealed buffer from the responder's side.
    fn responder_open(t: &mut snow::TransportState, wire: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut at = 0;
        while at + 2 <= wire.len() {
            let clen = u16::from_le_bytes([wire[at], wire[at + 1]]) as usize;
            let mut pt = vec![0u8; clen];
            let n = t.read_message(&wire[at + 2..at + 2 + clen], &mut pt).unwrap();
            out.extend_from_slice(&pt[..n]);
            at += 2 + clen;
        }
        out
    }

    #[test]
    fn a_sealed_message_opens_on_the_other_side() {
        let (mut session, mut responder) = paired(4321, 4321).unwrap();
        let wire = session.seal(b"page down").unwrap();
        assert_ne!(&wire[2..], b"page down", "the wire must not carry the plaintext");
        assert_eq!(responder_open(&mut responder, &wire), b"page down");
    }

    #[test]
    fn a_wrong_pin_cannot_complete_the_handshake() {
        assert!(paired(4321, 1234).is_err());
    }

    #[test]
    fn pin_zero_still_encrypts() {
        let (mut session, mut responder) = paired(0, 0).unwrap();
        let wire = session.seal(b"no pin, still sealed").unwrap();
        assert_eq!(responder_open(&mut responder, &wire), b"no pin, still sealed");
    }

    #[test]
    fn feeding_one_byte_at_a_time_yields_the_whole_message() {
        let (mut session, mut responder) = paired(7, 7).unwrap();
        // Seal on the responder's side so `session` can open it.
        let mut ct = vec![0u8; 32 + 16];
        let n = responder.write_message(b"drip fed", &mut ct).unwrap();
        let mut wire = (n as u16).to_le_bytes().to_vec();
        wire.extend_from_slice(&ct[..n]);

        for (i, byte) in wire.iter().enumerate() {
            session.feed(&[*byte]).unwrap();
            if i + 1 < wire.len() {
                assert_eq!(session.available(), 0, "nothing decrypts before the record is whole");
            }
        }
        assert_eq!(session.take_all(), b"drip fed");
        assert!(!session.has_partial_record(), "a complete record leaves nothing behind");
    }

    #[test]
    fn two_records_in_one_chunk_both_decrypt() {
        let (mut session, mut responder) = paired(8, 8).unwrap();
        let mut wire = Vec::new();
        for msg in [b"first".as_slice(), b"second".as_slice()] {
            let mut ct = vec![0u8; msg.len() + 16];
            let n = responder.write_message(msg, &mut ct).unwrap();
            wire.extend_from_slice(&(n as u16).to_le_bytes());
            wire.extend_from_slice(&ct[..n]);
        }
        session.feed(&wire).unwrap();
        assert_eq!(session.take_all(), b"firstsecond");
    }

    #[test]
    fn a_payload_larger_than_one_noise_message_is_split_and_rejoined() {
        let (mut session, mut responder) = paired(9, 9).unwrap();
        let big = vec![0xABu8; MAX_PLAINTEXT * 2 + 100];
        let wire = session.seal(&big).unwrap();
        assert_eq!(responder_open(&mut responder, &wire), big);
    }

    #[test]
    fn take_hands_back_only_what_was_asked_for() {
        let (mut session, mut responder) = paired(10, 10).unwrap();
        let mut ct = vec![0u8; 16 + 16];
        let n = responder.write_message(b"0123456789", &mut ct).unwrap();
        let mut wire = (n as u16).to_le_bytes().to_vec();
        wire.extend_from_slice(&ct[..n]);
        session.feed(&wire).unwrap();

        let mut buf = [0u8; 4];
        assert_eq!(session.take(&mut buf), 4);
        assert_eq!(&buf, b"0123");
        assert_eq!(session.available(), 6);
        assert_eq!(session.take_all(), b"456789");
    }

    #[test]
    fn a_truncated_handshake_reply_is_an_error_not_a_panic() {
        let (init, _) = Initiator::start(1).unwrap();
        assert!(init.finish(&[0x05]).is_err());
        let (init, _) = Initiator::start(1).unwrap();
        assert!(init.finish(&[0xff, 0xff, 0x00]).is_err()); // claims 65535 bytes
    }
}
