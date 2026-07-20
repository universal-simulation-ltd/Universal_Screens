//! Transport encryption for the Universal Screens LAN protocol.
//!
//! The wire protocol (`crates/protocol`) is length-prefixed `postcard` frames over
//! plaintext TCP, gated by a 4-digit pairing PIN. Historically the PIN was *a gate,
//! not encryption*: anyone on the LAN could read the stream (mirror video, injected
//! keystrokes/text) or, on-path, tamper with it. This crate closes that gap.
//!
//! ## What it does
//!
//! A native client, right after `TcpStream::connect`, runs a **Noise** handshake
//! ([`connect`]) — `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s` — before it sends its
//! [`ClientHello`](../extender_protocol/struct.ClientHello.html). The PIN is folded
//! in as the Noise **pre-shared key** ([`derive_psk`]), so:
//!
//! - **Confidentiality + forward secrecy:** the ephemeral-ephemeral (`NN`) DH means
//!   a passive eavesdropper on the LAN learns nothing, even if the PIN later leaks.
//! - **PIN-bound MITM resistance:** an active on-path attacker can't complete the
//!   handshake (or silently relay it) without knowing the PIN, because the PSK keys
//!   the AEAD. This is what makes the PIN *encryption*, not just a gate.
//!
//! The existing plaintext-`ClientHello` PIN check is **kept unchanged** on top of
//! the tunnel (belt and suspenders) — this layer never weakens the existing auth.
//!
//! ## Compatibility
//!
//! The host [`accept`]s a connection by peeking the first bytes:
//!
//! - A [`PREAMBLE`] marker ⇒ an encrypted native client ⇒ run the Noise responder
//!   and hand back an encrypted [`Conn`].
//! - Anything else ⇒ a legacy plaintext peer or the loopback WebSocket bridge
//!   (`crates/web-bridge`, which forwards raw frames from a browser tab and can't
//!   speak Noise) ⇒ hand back a plaintext [`Conn`] (logged by the caller).
//!
//! So encryption is on by default for every native client, without breaking the
//! browser-bridge path or an older plaintext client. Requiring encryption from
//! non-loopback peers is a follow-up hardening once every client ships this build.
//!
//! ## Framing
//!
//! [`Conn`] implements [`Read`] + [`Write`], so `protocol::{read_framed,
//! write_framed}` run over it unchanged. The secure variant transparently splits
//! the byte stream into Noise transport messages (each ≤ 64 KiB, the Noise limit)
//! carried as `u16`-length-prefixed ciphertext. The read and write halves share one
//! [`snow::TransportState`] behind a mutex; each `Conn` clone drives a single
//! direction (one reader thread, one writer thread — the pattern the client session
//! and both hosts already use), so nonce order always matches wire order.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// The Noise handshake pattern + crypto suite. `NNpsk0`: ephemeral-ephemeral (no
/// static keys to distribute) with the PIN mixed in as a pre-shared key at the
/// very start of the handshake.
pub const NOISE_PARAMS: &str = "Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s";

/// Marker a native (encrypting) client writes before its first Noise message, so
/// the host can tell an encrypted peer from a legacy/loopback plaintext one. Chosen
/// so it can never collide with a plaintext `ClientHello`'s 4-byte little-endian
/// length prefix (`b'U'` = 0x55; a hello body is tens of bytes, so its length's
/// first byte is far smaller and the following bytes are zero).
pub const PREAMBLE: [u8; 5] = [b'U', b'S', b'C', b'R', 0x01];

/// Domain-separation tag for [`derive_psk`], so this PSK can't be confused with a
/// PIN hash used for any other purpose. Bump the version suffix if the derivation
/// ever changes (it would break the handshake between old and new peers).
const PSK_DOMAIN: &[u8] = b"universal-screens/noise-psk/v1";

/// Largest plaintext chunk per Noise transport message: the 65535-byte Noise
/// message limit minus the 16-byte ChaChaPoly authentication tag.
const MAX_PLAINTEXT: usize = 65535 - 16;

/// Upper bound on a single handshake message we'll read, so a garbage/oversized
/// length prefix can't make us allocate unboundedly. Noise `NN` handshake messages
/// are ~48 bytes; 4 KiB is comfortable headroom.
const MAX_HANDSHAKE_MSG: usize = 4096;

/// Derive the 32-byte Noise pre-shared key from the pairing PIN.
///
/// `pin == 0` means "no pairing"; it still yields a (fixed, well-known) key so the
/// channel is always encrypted against passive eavesdroppers — it just carries no
/// authentication, matching the existing "PIN 0 = accept anyone" semantics. When a
/// PIN is set, both ends must derive the same key or the handshake fails.
#[must_use]
pub fn derive_psk(pin: u32) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(PSK_DOMAIN);
    hasher.update(pin.to_le_bytes());
    hasher.finalize().into()
}

/// Map a `snow` error into an `io::Error` so handshakes and transport ops share the
/// `io::Result` signature the rest of the stack uses.
fn noise_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("noise: {e}"))
}

fn noise_params() -> io::Result<snow::params::NoiseParams> {
    NOISE_PARAMS.parse().map_err(noise_err)
}

/// Write one length-prefixed handshake message (`u16` LE length + body).
fn write_handshake_msg<W: Write>(w: &mut W, body: &[u8]) -> io::Result<()> {
    let len = u16::try_from(body.len()).map_err(|_| noise_err("handshake message too large"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(body)?;
    w.flush()
}

/// Read one length-prefixed handshake message, bounded by [`MAX_HANDSHAKE_MSG`].
fn read_handshake_msg<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 2];
    r.read_exact(&mut len_buf)?;
    let len = u16::from_le_bytes(len_buf) as usize;
    if len > MAX_HANDSHAKE_MSG {
        return Err(noise_err("handshake message exceeds the maximum size"));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(body)
}

/// Perform the Noise **initiator** handshake as the client: write the [`PREAMBLE`],
/// exchange the two `NNpsk0` handshake messages, and return an encrypted [`Conn`].
/// `pin` is the pairing PIN (0 = none) — it must match the host's or the handshake
/// fails. Call this immediately after `TcpStream::connect`, before any framing.
///
/// # Errors
/// Returns an error if the preamble/handshake write or read fails, or the peer
/// rejects the handshake (e.g. a PIN mismatch, which fails the AEAD).
pub fn connect(mut stream: TcpStream, pin: u32) -> io::Result<Conn> {
    let psk = derive_psk(pin);
    let mut hs = snow::Builder::new(noise_params()?)
        .psk(0, &psk)
        .build_initiator()
        .map_err(noise_err)?;

    stream.write_all(&PREAMBLE)?;

    // -> psk, e
    let mut buf = [0u8; MAX_HANDSHAKE_MSG];
    let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
    write_handshake_msg(&mut stream, &buf[..n])?;

    // <- e, ee
    let msg = read_handshake_msg(&mut stream)?;
    let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
    hs.read_message(&msg, &mut scratch).map_err(noise_err)?;

    let transport = hs.into_transport_mode().map_err(noise_err)?;
    Ok(Conn::Secure(SecureStream::new(stream, transport)))
}

/// Accept a connection as the host: peek the first bytes to decide whether the peer
/// is an encrypting native client (runs the Noise **responder** handshake, keyed by
/// `expected_pin`) or a legacy/loopback plaintext peer (returned as-is). Nothing is
/// consumed on the plaintext path, so the caller's existing `read_framed` sees an
/// intact stream.
///
/// # Errors
/// Returns an error if peeking, the handshake, or the underlying socket fails.
pub fn accept(stream: TcpStream, expected_pin: u32) -> io::Result<Conn> {
    if !peek_is_preamble(&stream)? {
        return Ok(Conn::Plain(stream));
    }
    accept_encrypted(stream, expected_pin)
}

/// Run the responder handshake on a stream already known to start with the
/// [`PREAMBLE`] (peeked by [`accept`]). Split out so it stays testable in isolation.
fn accept_encrypted(mut stream: TcpStream, expected_pin: u32) -> io::Result<Conn> {
    let mut pre = [0u8; PREAMBLE.len()];
    stream.read_exact(&mut pre)?;
    if pre != PREAMBLE {
        return Err(noise_err("client preamble mismatch"));
    }

    let psk = derive_psk(expected_pin);
    let mut hs = snow::Builder::new(noise_params()?)
        .psk(0, &psk)
        .build_responder()
        .map_err(noise_err)?;

    // -> psk, e
    let msg = read_handshake_msg(&mut stream)?;
    let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
    hs.read_message(&msg, &mut scratch).map_err(noise_err)?;

    // <- e, ee
    let mut buf = [0u8; MAX_HANDSHAKE_MSG];
    let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
    write_handshake_msg(&mut stream, &buf[..n])?;

    let transport = hs.into_transport_mode().map_err(noise_err)?;
    Ok(Conn::Secure(SecureStream::new(stream, transport)))
}

/// Peek (without consuming) enough bytes to tell whether the peer opened with the
/// encrypted [`PREAMBLE`]. Short-circuits as soon as a byte differs, so a plaintext
/// peer is classified from its very first byte with nothing consumed.
fn peek_is_preamble(stream: &TcpStream) -> io::Result<bool> {
    let mut buf = [0u8; PREAMBLE.len()];
    loop {
        let n = stream.peek(&mut buf)?;
        if n == 0 {
            // Peer closed before sending anything — not an encrypted client.
            return Ok(false);
        }
        // Any mismatch in the bytes we can see settles it immediately.
        if buf[..n] != PREAMBLE[..n] {
            return Ok(false);
        }
        if n >= PREAMBLE.len() {
            return Ok(true);
        }
        // Saw a matching-but-partial prefix; loop to peek more once it arrives.
    }
}

/// A possibly-encrypted connection. Implements [`Read`] + [`Write`] so the
/// `postcard` framing runs over it unchanged, and mirrors the `TcpStream` surface
/// the callers use (`try_clone`, `shutdown`, `set_nodelay`).
pub enum Conn {
    /// A legacy/loopback plaintext peer (e.g. the WebSocket bridge).
    Plain(TcpStream),
    /// A Noise-encrypted native client.
    Secure(SecureStream),
}

impl Conn {
    /// Clone this connection for a second thread (one reads, one writes) — like
    /// [`TcpStream::try_clone`]. A secure clone shares the one transport cipher
    /// state; a fresh read buffer is fine because each clone drives one direction.
    ///
    /// # Errors
    /// Returns an error if the underlying `TcpStream::try_clone` fails.
    pub fn try_clone(&self) -> io::Result<Conn> {
        match self {
            Conn::Plain(s) => Ok(Conn::Plain(s.try_clone()?)),
            Conn::Secure(s) => Ok(Conn::Secure(s.try_clone()?)),
        }
    }

    /// Shut the underlying socket down, as [`TcpStream::shutdown`] — used to unblock
    /// a parked reader when a session is dropped.
    ///
    /// # Errors
    /// Returns an error if the underlying shutdown fails.
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.tcp().shutdown(how)
    }

    /// Set `TCP_NODELAY` on the underlying socket (disable Nagle for low latency),
    /// as [`TcpStream::set_nodelay`].
    ///
    /// # Errors
    /// Returns an error if the underlying call fails.
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.tcp().set_nodelay(nodelay)
    }

    /// Set the read timeout on the underlying socket, as
    /// [`TcpStream::set_read_timeout`]. Used to bound a blocking read (e.g. the
    /// handshake) so a peer that goes silent can't wedge it forever; pass `None`
    /// to clear it and return to indefinite blocking.
    ///
    /// # Errors
    /// Returns an error if the underlying call fails.
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.tcp().set_read_timeout(dur)
    }

    /// Set the write timeout on the underlying socket, as
    /// [`TcpStream::set_write_timeout`]. `None` clears it.
    ///
    /// # Errors
    /// Returns an error if the underlying call fails.
    pub fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.tcp().set_write_timeout(dur)
    }

    /// Whether this connection is transport-encrypted.
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        matches!(self, Conn::Secure(_))
    }

    fn tcp(&self) -> &TcpStream {
        match self {
            Conn::Plain(s) => s,
            Conn::Secure(s) => &s.inner,
        }
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Secure(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Secure(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Secure(s) => s.flush(),
        }
    }
}

/// The encrypted half of a [`Conn`]: a `TcpStream` plus the shared Noise transport
/// cipher state. Reads decrypt one Noise message at a time into `rbuf`; writes
/// encrypt the caller's bytes into `≤ 64 KiB` Noise messages.
pub struct SecureStream {
    inner: TcpStream,
    transport: Arc<Mutex<snow::TransportState>>,
    /// Decrypted plaintext already read off the socket but not yet handed to the
    /// caller (a Noise message can hold more bytes than one `read` asked for).
    rbuf: VecDeque<u8>,
}

impl SecureStream {
    fn new(inner: TcpStream, transport: snow::TransportState) -> Self {
        SecureStream {
            inner,
            transport: Arc::new(Mutex::new(transport)),
            rbuf: VecDeque::new(),
        }
    }

    fn try_clone(&self) -> io::Result<SecureStream> {
        Ok(SecureStream {
            inner: self.inner.try_clone()?,
            transport: Arc::clone(&self.transport),
            rbuf: VecDeque::new(),
        })
    }

    /// Read and decrypt exactly one Noise transport message into `rbuf`. Returns
    /// `Ok(false)` on a clean EOF at a message boundary, `Ok(true)` otherwise.
    fn fill_rbuf(&mut self) -> io::Result<bool> {
        let mut len_buf = [0u8; 2];
        match self.inner.read_exact(&mut len_buf) {
            Ok(()) => {}
            // A clean EOF exactly at a message boundary is a normal stream end.
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(false),
            Err(e) => return Err(e),
        }
        let clen = u16::from_le_bytes(len_buf) as usize;
        let mut ct = vec![0u8; clen];
        self.inner.read_exact(&mut ct)?;

        let mut pt = vec![0u8; clen];
        let n = {
            let mut t = self.transport.lock().unwrap();
            t.read_message(&ct, &mut pt).map_err(noise_err)?
        };
        self.rbuf.extend(pt[..n].iter().copied());
        Ok(true)
    }
}

impl Read for SecureStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // Refill until we have plaintext or hit a real EOF. The loop skips any
        // (unexpected) empty Noise message rather than misreporting it as EOF.
        while self.rbuf.is_empty() {
            if !self.fill_rbuf()? {
                return Ok(0);
            }
        }
        let n = buf.len().min(self.rbuf.len());
        for slot in buf.iter_mut().take(n) {
            *slot = self.rbuf.pop_front().expect("rbuf non-empty");
        }
        Ok(n)
    }
}

impl Write for SecureStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // Encrypt every chunk under the lock (so nonces increment in order), then
        // release the lock before the socket write so a concurrent reader can
        // decrypt meanwhile. Correct because exactly one thread ever writes a given
        // direction, so encryption order == wire order.
        let mut out = Vec::with_capacity(buf.len() + 32);
        {
            let mut t = self.transport.lock().unwrap();
            for chunk in buf.chunks(MAX_PLAINTEXT) {
                let mut ct = vec![0u8; chunk.len() + 16];
                let n = t.write_message(chunk, &mut ct).map_err(noise_err)?;
                let clen = u16::try_from(n).map_err(|_| noise_err("ciphertext too large"))?;
                out.extend_from_slice(&clen.to_le_bytes());
                out.extend_from_slice(&ct[..n]);
            }
        }
        self.inner.write_all(&out)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `Read` + `Write` are already in scope via `super::*` (the parent's io imports).
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn psk_is_deterministic_and_pin_sensitive() {
        assert_eq!(derive_psk(1234), derive_psk(1234));
        assert_ne!(derive_psk(1234), derive_psk(1235));
        assert_ne!(derive_psk(0), derive_psk(1234));
        assert_eq!(derive_psk(0).len(), 32);
    }

    #[test]
    fn preamble_cannot_collide_with_a_plaintext_hello_length_prefix() {
        // A plaintext client opens with a 4-byte LE length of a small postcard body.
        for body_len in 0u32..4096 {
            let prefix = body_len.to_le_bytes();
            assert_ne!(prefix[..], PREAMBLE[..4], "len {body_len} collides");
        }
    }

    /// Spin up a loopback host that runs the responder handshake, then echoes every
    /// framed-style write back, so a client can prove the tunnel round-trips.
    fn secure_pair(pin_client: u32, pin_host: u32) -> io::Result<(Conn, Conn)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            accept(sock, pin_host)
        });
        let client = connect(TcpStream::connect(addr)?, pin_client)?;
        let server = host.join().unwrap()?;
        Ok((client, server))
    }

    #[test]
    fn matching_pin_round_trips_bytes_both_ways() {
        let (mut client, mut server) = secure_pair(4321, 4321).unwrap();
        assert!(client.is_encrypted() && server.is_encrypted());

        // Client -> server.
        client.write_all(b"hello over the wire").unwrap();
        client.flush().unwrap();
        let mut got = [0u8; 19];
        server.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"hello over the wire");

        // Server -> client.
        server.write_all(b"ack").unwrap();
        server.flush().unwrap();
        let mut back = [0u8; 3];
        client.read_exact(&mut back).unwrap();
        assert_eq!(&back, b"ack");
    }

    #[test]
    fn wrong_pin_fails_the_handshake() {
        // A PIN mismatch must not yield a usable channel (fails the AEAD).
        let result = secure_pair(1111, 2222);
        assert!(result.is_err(), "mismatched PINs must not establish a tunnel");
    }

    #[test]
    fn pin_zero_still_encrypts() {
        // No pairing (PIN 0) on both ends: still a working, encrypted channel.
        let (mut client, mut server) = secure_pair(0, 0).unwrap();
        client.write_all(b"unpaired but encrypted").unwrap();
        client.flush().unwrap();
        let mut got = [0u8; 22];
        server.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"unpaired but encrypted");
    }

    #[test]
    fn large_payload_spans_multiple_noise_messages() {
        // Bigger than one Noise message (64 KiB), to exercise chunking + reassembly
        // — this is the keyframe-sized case for the video stream.
        let (mut client, mut server) = secure_pair(9, 9).unwrap();
        let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let expected = payload.clone();
        let writer = thread::spawn(move || {
            client.write_all(&payload).unwrap();
            client.flush().unwrap();
            client // keep it alive until the reader is done
        });
        let mut got = vec![0u8; expected.len()];
        server.read_exact(&mut got).unwrap();
        assert_eq!(got, expected);
        let _client = writer.join().unwrap();
    }

    #[test]
    fn ciphertext_on_the_wire_is_not_plaintext() {
        // Prove the bytes actually leaving the socket aren't the cleartext. The peer
        // completes the responder handshake by hand (so the client's `connect`
        // proceeds), then reads the *raw* transport frames — which must not contain
        // the secret, and must not be empty.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let secret = b"TOP-SECRET-KEYSTROKES";
        let raw = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            // Drive the responder side manually with the crate's own helpers.
            let mut pre = [0u8; PREAMBLE.len()];
            sock.read_exact(&mut pre).unwrap();
            assert_eq!(pre, PREAMBLE);
            let psk = derive_psk(7777);
            let mut hs = snow::Builder::new(noise_params().unwrap())
                .psk(0, &psk)
                .build_responder()
                .unwrap();
            let m1 = read_handshake_msg(&mut sock).unwrap();
            let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
            hs.read_message(&m1, &mut scratch).unwrap();
            let mut buf = [0u8; MAX_HANDSHAKE_MSG];
            let n = hs.write_message(&[], &mut buf).unwrap();
            write_handshake_msg(&mut sock, &buf[..n]).unwrap();
            let _transport = hs.into_transport_mode().unwrap();

            // Capture the raw transport ciphertext the client sends (bounded).
            sock.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0u8; 512];
            loop {
                match sock.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(k) => {
                        seen.extend_from_slice(&chunk[..k]);
                        if seen.len() > 8192 {
                            break;
                        }
                    }
                    Err(_) => break, // read timeout — no more data
                }
            }
            seen
        });
        let mut client = connect(TcpStream::connect(addr).unwrap(), 7777).unwrap();
        // Write the secret a few times so there's plenty on the wire to scan.
        for _ in 0..4 {
            client.write_all(secret).unwrap();
        }
        client.flush().unwrap();
        let on_wire = raw.join().unwrap();
        assert!(!on_wire.is_empty(), "expected ciphertext on the wire");
        assert!(
            !on_wire.windows(secret.len()).any(|w| w == secret),
            "cleartext secret found on the wire"
        );
    }

    #[test]
    fn plaintext_peer_is_passed_through_untouched() {
        // A peer that doesn't send the preamble (e.g. the loopback bridge) must be
        // returned as a plaintext Conn with its first bytes intact.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            accept(sock, 0)
        });
        let mut raw = TcpStream::connect(addr).unwrap();
        // Looks like a legacy plaintext frame: a 4-byte LE length then the body.
        raw.write_all(&5u32.to_le_bytes()).unwrap();
        raw.write_all(b"world").unwrap();
        raw.flush().unwrap();

        let mut conn = host.join().unwrap().unwrap();
        assert!(!conn.is_encrypted());
        let mut len = [0u8; 4];
        conn.read_exact(&mut len).unwrap();
        assert_eq!(u32::from_le_bytes(len), 5);
        let mut body = [0u8; 5];
        conn.read_exact(&mut body).unwrap();
        assert_eq!(&body, b"world");
    }
}
