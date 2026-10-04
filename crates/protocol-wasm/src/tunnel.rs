//! The encrypted browser leg: pairing (v2 — SPAKE2, then Noise; or by
//! remembered key), the older PIN-keyed Noise tunnel (v1, for v0.3 hosts only),
//! and the protocol's own framing, exposed to JavaScript.
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

use extender_transport::pairing::{from_hex, to_hex};
use extender_transport::{failure, ClientHandshake, Failure, Identity, Initiator, Session, Step};
use wasm_bindgen::prelude::*;

// ---- v2: pairing with SPAKE2, reconnecting by key -------------------------------

/// This tab's long-term key pair — what a host remembers it by after pairing.
///
/// The page keeps the two hex halves in `localStorage` (the browser's only
/// durable store) and rebuilds the pair with [`PairingKeys::from_hex`]. ⚠️ So
/// anyone who can read this origin's storage can act as this browser to the
/// hosts it paired with; "Forget paired devices" on the host is the remedy.
#[wasm_bindgen]
pub struct PairingKeys {
    identity: Identity,
}

#[wasm_bindgen]
impl PairingKeys {
    /// A fresh key pair from `crypto.getRandomValues`.
    ///
    /// # Errors
    /// If no randomness is available.
    pub fn generate() -> Result<PairingKeys, String> {
        Identity::generate().map(|identity| PairingKeys { identity }).map_err(|e| e.to_string())
    }

    /// Rebuild a stored pair.
    ///
    /// # Errors
    /// If either half is not 64 hex digits.
    #[wasm_bindgen(js_name = fromHex)]
    pub fn from_hex(secret: &str, public: &str) -> Result<PairingKeys, String> {
        let secret = from_hex(secret).ok_or("the stored secret key is not 64 hex digits")?;
        let public = from_hex(public).ok_or("the stored public key is not 64 hex digits")?;
        Ok(PairingKeys { identity: Identity::from_parts(secret, public) })
    }

    /// The secret half, as hex, for storage.
    #[wasm_bindgen(getter, js_name = secretHex)]
    #[must_use]
    pub fn secret_hex(&self) -> String {
        to_hex(&self.identity.secret())
    }

    /// The public half, as hex, for storage.
    #[wasm_bindgen(getter, js_name = publicHex)]
    #[must_use]
    pub fn public_hex(&self) -> String {
        to_hex(&self.identity.public())
    }
}

/// The **v2** handshake for a tab — the same `ClientHandshake` state machine
/// every native client runs, bytes in and bytes out.
///
/// ```js
/// const hs = PairingHandshake.pair(pin, keys);        // or .reconnect(keys, hostKeys)
/// ws.send(hs.firstMessage);
/// ws.onmessage = (m) => {
///   try { const out = hs.feed(bytes); if (out.length) ws.send(out); }
///   catch (e) { report(hs.failure); } // "wrong-code", "not-paired", …
///
/// ⚠️ No block comments in these examples: wasm-bindgen copies them into a JS
/// doc comment, where a closing star-slash ends it early and breaks the module.
///   if (hs.done) { const tunnel = hs.takeTunnel(); remember(hs.hostKeyHex); }
/// };
/// ws.onclose = () => { if (!hs.done) report(hs.closedFailure()); };
/// ```
#[wasm_bindgen]
pub struct PairingHandshake {
    hs: ClientHandshake,
    first: Vec<u8>,
    established: Option<extender_transport::Established>,
    host_key: Option<[u8; 32]>,
    paired: bool,
    failure: Option<Failure>,
}

fn failure_code(f: Failure) -> &'static str {
    match f {
        Failure::WrongCode => "wrong-code",
        Failure::HostLocked { .. } => "host-locked",
        Failure::NotPaired => "not-paired",
        Failure::UnknownHost => "unknown-host",
        Failure::OlderHost => "older-host",
        Failure::Unsupported => "unsupported",
    }
}

#[wasm_bindgen]
impl PairingHandshake {
    /// Pair over `pin` (0 = a host with pairing switched off).
    ///
    /// # Errors
    /// Only if the opening cannot be built.
    pub fn pair(pin: u32, keys: &PairingKeys) -> Result<PairingHandshake, String> {
        let (hs, first) = ClientHandshake::pair(pin, &keys.identity).map_err(|e| e.to_string())?;
        Ok(Self::new(hs, first))
    }

    /// Reconnect by key, with no code, to any host whose public key (hex) is in
    /// `host_keys`.
    ///
    /// # Errors
    /// A key that is not 64 hex digits, or an opening that cannot be built.
    pub fn reconnect(keys: &PairingKeys, host_keys: Vec<String>) -> Result<PairingHandshake, String> {
        let trusted = host_keys
            .iter()
            .map(|k| from_hex(k).ok_or_else(|| format!("not a host key: {k:?}")))
            .collect::<Result<Vec<_>, _>>()?;
        let (hs, first) = ClientHandshake::reconnect(&keys.identity, trusted).map_err(|e| e.to_string())?;
        Ok(Self::new(hs, first))
    }

    fn new(hs: ClientHandshake, first: Vec<u8>) -> Self {
        PairingHandshake { hs, first, established: None, host_key: None, paired: false, failure: None }
    }

    /// The bytes to send before anything else.
    #[wasm_bindgen(getter, js_name = firstMessage)]
    #[must_use]
    pub fn first_message(&self) -> Vec<u8> {
        self.first.clone()
    }

    /// Feed what arrived; returns what to send now (possibly nothing).
    ///
    /// # Errors
    /// The handshake failed; [`failure`](Self::failure) names why when it is
    /// something a person can act on.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        match self.hs.feed(bytes) {
            Ok(Step::Continue(out)) => Ok(out),
            Ok(Step::Done { send, established }) => {
                self.host_key = Some(established.host_key);
                self.paired = established.paired;
                self.established = Some(*established);
                Ok(send)
            }
            Err(e) => {
                self.failure = failure(&e);
                Err(e.to_string())
            }
        }
    }

    /// True once the tunnel is open (take it with [`take_tunnel`](Self::take_tunnel)).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn done(&self) -> bool {
        self.host_key.is_some()
    }

    /// The open tunnel. It may already hold decrypted bytes.
    ///
    /// # Errors
    /// Before the handshake is done, or a second time.
    #[wasm_bindgen(js_name = takeTunnel)]
    pub fn take_tunnel(&mut self) -> Result<Tunnel, String> {
        self.established
            .take()
            .map(|e| Tunnel { session: e.session })
            .ok_or_else(|| "no tunnel to take".to_owned())
    }

    /// The host's long-term key (hex), once done — what to remember.
    #[wasm_bindgen(getter, js_name = hostKeyHex)]
    #[must_use]
    pub fn host_key_hex(&self) -> Option<String> {
        self.host_key.map(|k| to_hex(&k))
    }

    /// True when this connection paired with the code (so the host key is new
    /// trust worth saving), false when it reconnected by key.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn paired(&self) -> bool {
        self.paired
    }

    /// Why the handshake failed, as a stable code: `wrong-code`, `host-locked`,
    /// `not-paired`, `unknown-host`, `older-host`, `unsupported` — or
    /// `undefined` for anything else.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn failure(&self) -> Option<String> {
        self.failure.map(|f| failure_code(f).to_owned())
    }

    /// Seconds left when [`failure`](Self::failure) is `host-locked`, else 0.
    #[wasm_bindgen(getter, js_name = lockedSeconds)]
    #[must_use]
    pub fn locked_seconds(&self) -> u32 {
        match self.failure {
            Some(Failure::HostLocked { seconds }) => seconds,
            _ => 0,
        }
    }

    /// The failure code to report when the socket closed before
    /// [`done`](Self::done) — what a close means depends on how far it got
    /// (`older-host` before any reply, `wrong-code` after the proof).
    #[wasm_bindgen(js_name = closedFailure)]
    #[must_use]
    pub fn closed_failure(&mut self) -> Option<String> {
        let f = failure(&self.hs.closed());
        self.failure = f;
        f.map(|f| failure_code(f).to_owned())
    }
}

// ---- v1: for a v0.3 host only ----------------------------------------------------

/// The **v1** handshake (`NNpsk0`, keyed by a hash of the PIN). ⚠️ A recording
/// of it lets the PIN be guessed offline — use it only when the host can do
/// nothing better (its bridge answered only `usscreens-e2ee.v1`). Send
/// [`Handshake::first_message`] as the first thing on the connection, then hand
/// the peer's reply to [`Handshake::finish`].
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
        // Checked: on wasm32 `usize` is 32 bits, so a length near `u32::MAX`
        // wrapped `4 + len` round to a tiny number and the slice below panicked,
        // taking the tab's whole WASM module with it.
        let end = len.checked_add(4)?;
        if self.buf.len() < end {
            return None;
        }
        let body = self.buf[4..end].to_vec();
        self.buf.drain(..end);
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
    fn stored_keys_round_trip_through_hex() {
        let keys = PairingKeys::generate().unwrap();
        let again = PairingKeys::from_hex(&keys.secret_hex(), &keys.public_hex()).unwrap();
        assert_eq!(again.public_hex(), keys.public_hex());
        assert!(PairingKeys::from_hex("nope", &keys.public_hex()).is_err());
    }

    #[test]
    fn a_v2_opening_starts_with_the_preamble_the_bridge_relays_on() {
        let keys = PairingKeys::generate().unwrap();
        let hs = PairingHandshake::pair(1234, &keys).unwrap();
        assert!(hs.first_message().starts_with(&extender_transport::PREAMBLE));
        let hs = PairingHandshake::reconnect(&keys, vec!["ab".repeat(32)]).unwrap();
        assert!(hs.first_message().starts_with(&extender_transport::handshake::OPENING));
        assert!(PairingHandshake::reconnect(&keys, vec!["xyz".into()]).is_err());
    }

    #[test]
    fn a_close_before_any_reply_reads_as_an_older_host() {
        let keys = PairingKeys::generate().unwrap();
        let mut hs = PairingHandshake::pair(1234, &keys).unwrap();
        assert_eq!(hs.closed_failure().as_deref(), Some("older-host"));
        assert!(!hs.done());
    }

    /// The host's "locked" reply surfaces as a code and a wait.
    #[test]
    fn a_locked_reply_is_reported_with_its_wait() {
        let keys = PairingKeys::generate().unwrap();
        let mut hs = PairingHandshake::pair(1234, &keys).unwrap();
        // u16 len 5, STATUS_LOCKED, 90 s.
        let reply = [5, 0, 1, 90, 0, 0, 0];
        assert!(hs.feed(&reply).is_err());
        assert_eq!(hs.failure().as_deref(), Some("host-locked"));
        assert_eq!(hs.locked_seconds(), 90);
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
