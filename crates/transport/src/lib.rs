//! Transport encryption and pairing for the Universal Screens LAN protocol.
//!
//! The wire protocol (`crates/protocol`) is length-prefixed `postcard` frames.
//! This crate wraps the TCP stream they travel on in a **Noise** tunnel, and
//! decides who is allowed to open one.
//!
//! ## Two handshakes, one of them retired
//!
//! **v2 — what every current client runs** ([`handshake`] has the details):
//!
//! - **Pairing** ([`connect_pair`]): the 4-digit code goes through **SPAKE2**
//!   (`unisim-pake`), and the Noise handshake (`XXpsk0`) is keyed by the
//!   result. A recording of it gives an eavesdropper nothing to test codes
//!   against; someone actively guessing gets one try per connection, and the
//!   host counts it ([`is_pin_rejection`] → [`PinGuard`]). Inside it the two
//!   ends swap long-term keys and remember each other ([`PairingStore`]).
//! - **Reconnecting** ([`connect_known`]): `XX` with the remembered keys and no
//!   code at all, so a paired phone reconnects after the host restarts with a
//!   new PIN.
//!
//! **v1 — `NNpsk0` keyed by `SHA-256(domain ‖ PIN)`** — is what v0.3 and earlier
//! spoke. Its first message is authenticated by a fixed function of the PIN,
//! so one recorded connection let anyone try all 10,000 PINs offline. Hosts
//! still **accept** it, so old clients are not stranded (the same call as
//! plaintext, below); no current client sends it to a host that can do better.
//!
//! Either way the tunnel gives confidentiality and forward secrecy (ephemeral
//! Diffie-Hellman), and the host's in-tunnel `ClientHello` PIN check stays on
//! top for everything except a v2 peer, whose handshake already settled it
//! ([`PeerAuth::settles_pin`]).
//!
//! ## Compatibility
//!
//! The host [`accept_with`]s a connection by peeking the first bytes:
//!
//! - the v2 [`OPENING`](handshake::OPENING) ⇒ pair, or let a remembered device in;
//! - the [`PREAMBLE`] with an ordinary length ⇒ a v1 client ⇒ the v1 responder;
//! - anything else ⇒ a legacy plaintext peer or the loopback WebSocket bridge
//!   relaying a tab that could not encrypt ⇒ a plaintext [`Conn`].
//!
//! Refusing v1 and plaintext from non-loopback peers is the follow-up once no
//! v0.3 client is left; until then, hosts log which kind each peer was.
//!
//! ## Framing
//!
//! [`Conn`] implements [`Read`] + [`Write`], so `protocol::{read_framed,
//! write_framed}` run over it unchanged. The secure variant transparently splits
//! the byte stream into Noise transport messages (each ≤ 64 KiB, the Noise limit)
//! carried as `u16`-length-prefixed ciphertext. The read and write halves share one
//! [`Session`] behind a mutex; each `Conn` clone drives a single direction (one
//! reader thread, one writer thread — the pattern the client session and both
//! hosts already use), so nonce order always matches wire order.
//!
//! ## Where the socket isn't
//!
//! The client handshakes and the record layer live in [`session`], which touches
//! no socket at all — it is bytes in, bytes out. That is what allows the
//! **browser** client, whose carrier is a WebSocket, to run the same tunnel with
//! the same code rather than a JavaScript re-implementation of the same wire
//! format. Everything in this file is the `TcpStream` adapter on top of it.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// The socket-free half of this crate: the initiator handshake and the record
/// layer as byte-in/byte-out state machines, so a carrier that is not a
/// `TcpStream` (a browser's WebSocket) can run the identical tunnel.
pub mod session;

/// Wrong-PIN rate limiting for the hosts' accept loops — see [`PinGuard`].
pub mod guard;

/// The v2 handshake's wire constants and failure kinds.
pub mod handshake;

/// This device's long-term key and the devices it has paired with.
pub mod pairing;

pub use guard::PinGuard;
pub use handshake::{failure, Failure};
pub use pairing::{Identity, PairedPeer, PairingStore};
pub use session::{ClientHandshake, Established, Initiator, Session, Step};

use handshake::{
    code_bytes, MODE_KNOWN, MODE_PAIR, OPENING, PAIR_CONTEXT, PSK_LABEL, STATUS_GO, STATUS_LOCKED,
    STATUS_UNSUPPORTED, VERDICT_NOT_PAIRED, VERDICT_OK, VERSION_ESCAPE,
};
use unisim_pake::{Pake, Role};

/// The inner error of an [`accept`] that failed because the peer's first Noise
/// message did not authenticate — which, since the PIN keys that message, is
/// what a wrong PIN looks like. Test for it with [`is_pin_rejection`].
#[derive(Debug)]
struct PinRejected(snow::Error);

impl std::fmt::Display for PinRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "noise: handshake rejected (wrong PIN?): {}", self.0)
    }
}

impl std::error::Error for PinRejected {}

/// Whether an [`accept`] error means the peer *tested a PIN and got it wrong* —
/// its handshake message arrived and failed to authenticate — as opposed to a
/// peer that hung up, timed out, or never spoke Noise. Only the former is a
/// guess, so only the former should count towards a [`PinGuard`] lockout.
#[must_use]
pub fn is_pin_rejection(e: &io::Error) -> bool {
    e.get_ref().is_some_and(|inner| inner.is::<PinRejected>())
}

/// How the far end of a [`Conn`] was authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerAuth {
    /// Not encrypted at all: a legacy client, or the loopback web bridge
    /// relaying a tab that could not encrypt.
    Plaintext,
    /// **v1**: encrypted, keyed by a hash of the PIN. It proves the PIN, but a
    /// recording of it lets the PIN be guessed offline. Only clients from v0.3
    /// and before (and a browser tab facing a v0.3 host) still speak it.
    LegacyPin,
    /// **v2**, paired on this connection: the code was proven by SPAKE2.
    Paired {
        /// The peer's long-term public key.
        key: [u8; pairing::KEY_LEN],
    },
    /// **v2**, a remembered device: its long-term key is on the paired list
    /// (or the host has pairing switched off and lets anyone in).
    Known {
        /// The peer's long-term public key.
        key: [u8; pairing::KEY_LEN],
    },
}

impl PeerAuth {
    /// Whether the transport itself already settled the PIN question, so the
    /// host's older in-tunnel `ClientHello` PIN check must be skipped. A
    /// remembered device does not send the current PIN — that is the point of
    /// remembering it.
    #[must_use]
    pub fn settles_pin(self) -> bool {
        matches!(self, PeerAuth::Paired { .. } | PeerAuth::Known { .. })
    }
}

/// The **v1** Noise handshake pattern + crypto suite. `NNpsk0`:
/// ephemeral-ephemeral with a hash of the PIN as the pre-shared key. Kept for
/// compatibility only — see [`handshake`] for v2 and why it replaced this.
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

/// How long a peer that has not yet authenticated may hold a host's accept loop:
/// the Noise handshake plus the `ClientHello`. Matches the client side
/// (`extender_core::HANDSHAKE_TIMEOUT`).
///
/// ⚠️ **Every host serves one connection at a time, on the accept loop's own
/// thread.** Without a bound, one TCP connection that opened and then said
/// nothing — or sent the first byte of the preamble and stopped — parked
/// [`accept`]'s peek forever, and nobody else could connect until the host was
/// restarted. It needed no PIN and no skill: `nc host 9000` was enough.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Bound the unauthenticated part of a connection by [`HANDSHAKE_TIMEOUT`]. Call
/// it on a freshly accepted socket, before [`accept`]; once the hello has been
/// read and checked, call [`Conn::end_handshake`] so an idle session (a clicker
/// between slides) is not cut off.
pub fn begin_handshake(stream: &TcpStream) {
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT));
}

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
///
/// ⚠️ One write, not two. Writing the length and then the body as separate
/// small writes meets Nagle's algorithm on the second one, which then waits for
/// the peer's delayed ACK — up to 40 ms on Linux and 200 ms on Windows, per
/// message, and v2 has three host messages.
fn write_handshake_msg<W: Write>(w: &mut W, body: &[u8]) -> io::Result<()> {
    w.write_all(&handshake::frame(body)?)?;
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

/// **v1** client handshake: write the [`PREAMBLE`], exchange the two `NNpsk0`
/// messages keyed by a hash of `pin`, and return an encrypted [`Conn`].
///
/// ⚠️ **Compatibility and tests only.** Its first message lets anyone who
/// records it guess the PIN offline. Current clients use [`connect_pair`] /
/// [`connect_known`]; this stays so the hosts' acceptance of v0.3 clients can be
/// tested.
///
/// # Errors
/// Returns an error if the preamble/handshake write or read fails, or the peer
/// rejects the handshake (e.g. a PIN mismatch, which fails the AEAD).
pub fn connect_v1(mut stream: TcpStream, pin: u32) -> io::Result<Conn> {
    let (init, first) = session::Initiator::start(pin)?;
    stream.write_all(&first)?; // PREAMBLE + the length-prefixed `-> psk, e`
    stream.flush()?;

    // <- e, ee
    let msg = read_handshake_msg(&mut stream)?;
    let mut reply = u16::try_from(msg.len())
        .map_err(|_| noise_err("handshake message too large"))?
        .to_le_bytes()
        .to_vec();
    reply.extend_from_slice(&msg);

    Ok(Conn::Secure(SecureStream::new(stream, init.finish(&reply)?, PeerAuth::LegacyPin)))
}

/// **Pair** with a host over its code (v2): SPAKE2, then `Noise_XXpsk0`. On
/// success the returned [`Established`]-style tuple carries the host's
/// long-term key — remember it (when `pin != 0`) so the next connection can use
/// [`connect_known`] and skip the code.
///
/// Call it immediately after `TcpStream::connect`, before any framing.
///
/// # Errors
/// I/O errors, or a [`Failure`] (read it with [`failure`]): `WrongCode`,
/// `HostLocked`, `OlderHost` (a v0.3 host, which cannot pair this way).
pub fn connect_pair(stream: TcpStream, pin: u32, identity: &Identity) -> io::Result<(Conn, [u8; 32])> {
    let (hs, first) = ClientHandshake::pair(pin, identity)?;
    drive_client(stream, hs, &first)
}

/// **Reconnect** to a host this device has paired with (v2): `Noise_XX` with
/// long-term keys, no code. `trusted` is every host key this device accepts.
///
/// # Errors
/// I/O errors, or a [`Failure`]: `NotPaired` (the host does not know, or has
/// forgotten, this device — pair again with the code), `UnknownHost` (the far
/// end is not a host this device paired with), `OlderHost`.
pub fn connect_known(
    stream: TcpStream,
    identity: &Identity,
    trusted: Vec<[u8; 32]>,
) -> io::Result<(Conn, [u8; 32])> {
    let (hs, first) = ClientHandshake::reconnect(identity, trusted)?;
    drive_client(stream, hs, &first)
}

/// Run a [`ClientHandshake`] over a socket: the same state machine a browser
/// drives over a WebSocket, with the socket as nothing but a byte carrier.
fn drive_client(
    mut stream: TcpStream,
    mut hs: ClientHandshake,
    first: &[u8],
) -> io::Result<(Conn, [u8; 32])> {
    stream.write_all(first)?;
    stream.flush()?;
    let mut chunk = [0u8; 4096];
    loop {
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            // ⚠️ A host that closes with our bytes still unread in its buffer
            // makes the OS send a reset, not a clean close — which is exactly
            // how a v0.3 host turns a v2 opening away. Same meaning as EOF.
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted | io::ErrorKind::BrokenPipe
                ) =>
            {
                0
            }
            Err(e) => return Err(e),
        };
        if n == 0 {
            return Err(hs.closed());
        }
        match hs.feed(&chunk[..n])? {
            Step::Continue(out) => {
                if !out.is_empty() {
                    stream.write_all(&out)?;
                    stream.flush()?;
                }
            }
            Step::Done { send, established } => {
                if !send.is_empty() {
                    stream.write_all(&send)?;
                    stream.flush()?;
                }
                let key = established.host_key;
                let auth = if established.paired { PeerAuth::Paired { key } } else { PeerAuth::Known { key } };
                return Ok((Conn::Secure(SecureStream::new(stream, established.session, auth)), key));
            }
        }
    }
}

/// Accept a connection as the host, with a throwaway identity and no memory
/// of paired devices: every v2 client must pair with the code, every time.
/// For tests and the development host; the real hosts use [`accept_with`].
///
/// # Errors
/// As [`accept_with`].
pub fn accept(stream: TcpStream, expected_pin: u32) -> io::Result<Conn> {
    let mut store = PairingStore::ephemeral()?;
    accept_with(stream, expected_pin, &mut store, "")
}

/// Accept a connection as the host. Peeks the first bytes to tell:
///
/// - a **v2** client (the [`OPENING`](handshake::OPENING)) — pair it over the
///   code with SPAKE2, or let a remembered device in by its key, per
///   [`handshake`];
/// - a **v1** client (the [`PREAMBLE`] and a real length) — the v0.3 `NNpsk0`
///   handshake keyed by a hash of the PIN, still accepted so old clients keep
///   working;
/// - anything else — a legacy/loopback plaintext peer, returned untouched.
///
/// `store` is this host's identity and paired list; a successful pairing (with
/// a non-zero PIN) is added to it and saved, labelled `peer_label`. It is
/// re-read from disk first, so a "Forget paired devices" made elsewhere takes
/// effect on the next connection.
///
/// Nothing is consumed on the plaintext path, so the caller's `read_framed`
/// sees an intact stream.
///
/// # Errors
/// Returns an error if peeking, the handshake, or the underlying socket fails.
/// A wrong code is marked — test with [`is_pin_rejection`] — and is the only
/// failure that should count towards a [`PinGuard`] lockout.
pub fn accept_with(
    stream: TcpStream,
    expected_pin: u32,
    store: &mut PairingStore,
    peer_label: &str,
) -> io::Result<Conn> {
    if !peek_is_preamble(&stream)? {
        return Ok(Conn::Plain(stream));
    }
    accept_encrypted(stream, expected_pin, store, peer_label)
}

/// Run the responder handshake on a stream already known to start with the
/// [`PREAMBLE`] (peeked by [`accept_with`]): v2 if the version escape follows,
/// v1 otherwise.
fn accept_encrypted(
    mut stream: TcpStream,
    expected_pin: u32,
    store: &mut PairingStore,
    peer_label: &str,
) -> io::Result<Conn> {
    let mut pre = [0u8; PREAMBLE.len()];
    stream.read_exact(&mut pre)?;
    if pre != PREAMBLE {
        return Err(noise_err("client preamble mismatch"));
    }
    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf)?;
    if len_buf == VERSION_ESCAPE {
        return accept_v2(stream, expected_pin, store, peer_label);
    }

    // ---- v1: NNpsk0 keyed by a hash of the PIN (v0.3 clients) ----------------
    let len = u16::from_le_bytes(len_buf) as usize;
    if len > MAX_HANDSHAKE_MSG {
        return Err(noise_err("handshake message exceeds the maximum size"));
    }
    let mut msg = vec![0u8; len];
    stream.read_exact(&mut msg)?;

    let psk = derive_psk(expected_pin);
    let mut hs = snow::Builder::new(noise_params()?)
        .psk(0, &psk)
        .build_responder()
        .map_err(noise_err)?;

    // -> psk, e
    let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
    // ⚠️ This is the line a wrong PIN fails on: the PSK keys this message's
    // AEAD tag. Its error is marked (`PinRejected`) so the host can count it
    // towards a lockout — and nothing above it is, because a peer that hangs
    // up before this point has tested no PIN.
    hs.read_message(&msg, &mut scratch)
        .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, PinRejected(e)))?;

    // <- e, ee
    let mut buf = [0u8; MAX_HANDSHAKE_MSG];
    let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
    write_handshake_msg(&mut stream, &buf[..n])?;

    let transport = hs.into_transport_mode().map_err(noise_err)?;
    Ok(Conn::Secure(SecureStream::new(stream, Session::from_transport(transport), PeerAuth::LegacyPin)))
}

/// The v2 responder. See [`handshake`] for the message sequence.
fn accept_v2(
    mut stream: TcpStream,
    expected_pin: u32,
    store: &mut PairingStore,
    peer_label: &str,
) -> io::Result<Conn> {
    let mut head = [0u8; 2];
    stream.read_exact(&mut head)?;
    let [version, mode] = head;
    if version != handshake::HANDSHAKE_VERSION || !matches!(mode, MODE_PAIR | MODE_KNOWN) {
        let _ = write_handshake_msg(&mut stream, &[STATUS_UNSUPPORTED]);
        return Err(noise_err(format!("unsupported handshake version {version} / mode {mode}")));
    }
    // Pick up a "Forget paired devices" made in the host window since the
    // last connection — before anything below can write the list back.
    let _ = store.reload();
    let mut prologue = OPENING.to_vec();
    prologue.push(mode);
    let secret = store.identity().secret();
    let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
    let mut buf = [0u8; MAX_HANDSHAKE_MSG];

    if mode == MODE_PAIR {
        // -> SPAKE2 A
        let msg_a = read_handshake_msg(&mut stream)?;
        prologue.extend_from_slice(&handshake::frame(&msg_a)?);
        // <- status, SPAKE2 B. Independent of A, and keyed by nothing yet:
        // nothing the host sends before the client's proof can be used to
        // test a code.
        let (pake, msg_b) = Pake::start(Role::Responder, &code_bytes(expected_pin), PAIR_CONTEXT);
        let mut reply = vec![STATUS_GO];
        reply.extend_from_slice(&msg_b);
        prologue.extend_from_slice(&handshake::frame(&reply)?);
        write_handshake_msg(&mut stream, &reply)?;
        // A malformed A is not a guess — nothing about the code was tested.
        let key = pake.finish(&msg_a).map_err(noise_err)?;
        let psk = key.derive(PSK_LABEL);
        let mut hs = snow::Builder::new(handshake::pair_params()?)
            .local_private_key(&secret)
            .psk(0, &psk)
            .prologue(&prologue)
            .build_responder()
            .map_err(noise_err)?;

        // -> psk, e
        let m1 = read_handshake_msg(&mut stream)?;
        // ⚠️ THE guess. The client's first Noise message is keyed by its
        // SPAKE2 result, which matches ours only if its code does. Marked so
        // the host counts it: one wrong code, one counted attempt.
        hs.read_message(&m1, &mut scratch)
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, PinRejected(e)))?;
        // <- e, ee, s, es
        let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
        write_handshake_msg(&mut stream, &buf[..n])?;
        // -> s, se
        let m3 = read_handshake_msg(&mut stream)?;
        hs.read_message(&m3, &mut scratch).map_err(noise_err)?;
        let client_key = remote_static_key(&hs)?;
        let mut session = Session::from_transport(hs.into_transport_mode().map_err(noise_err)?);
        // With pairing switched off (PIN 0) everyone is let in anyway, so
        // there is nothing to remember — and a device that came in while it was
        // off must not stay trusted once a PIN is set.
        if expected_pin != 0 {
            // Best effort: a failed save still lets this session in; the device
            // simply pairs again next time.
            let _ = store.remember(client_key, peer_label);
        }
        stream.write_all(&session.seal(&[VERDICT_OK])?)?;
        stream.flush()?;
        return Ok(Conn::Secure(SecureStream::new(stream, session, PeerAuth::Paired { key: client_key })));
    }

    // ---- MODE_KNOWN: remembered keys, no code ----------------------------------
    let mut hs = snow::Builder::new(handshake::known_params()?)
        .local_private_key(&secret)
        .prologue(&prologue)
        .build_responder()
        .map_err(noise_err)?;
    // -> e
    let m1 = read_handshake_msg(&mut stream)?;
    hs.read_message(&m1, &mut scratch).map_err(noise_err)?;
    // <- status, (e, ee, s, es)
    let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
    let mut reply = vec![STATUS_GO];
    reply.extend_from_slice(&buf[..n]);
    write_handshake_msg(&mut stream, &reply)?;
    // -> s, se. A client that does not recognise this host's key stops before
    // this and hangs up — not a guess, just a stranger.
    let m3 = read_handshake_msg(&mut stream)?;
    hs.read_message(&m3, &mut scratch).map_err(noise_err)?;
    let client_key = remote_static_key(&hs)?;
    let mut session = Session::from_transport(hs.into_transport_mode().map_err(noise_err)?);
    if expected_pin != 0 && !store.is_paired(&client_key) {
        // Say so inside the tunnel, so the client knows to ask for the code
        // rather than reporting a broken connection. Not a guess: no code was
        // tested.
        let _ = stream.write_all(&session.seal(&[VERDICT_NOT_PAIRED])?);
        let _ = stream.flush();
        return Err(Failure::NotPaired.into_io());
    }
    stream.write_all(&session.seal(&[VERDICT_OK])?)?;
    stream.flush()?;
    Ok(Conn::Secure(SecureStream::new(stream, session, PeerAuth::Known { key: client_key })))
}

fn remote_static_key(hs: &snow::HandshakeState) -> io::Result<[u8; 32]> {
    let key = hs.get_remote_static().ok_or_else(|| noise_err("the peer sent no static key"))?;
    <[u8; 32]>::try_from(key).map_err(|_| noise_err("the peer's static key has the wrong length"))
}

/// Turn a connection away while the host is locked after wrong codes, telling
/// a **v2** client why and for how long, so its user sees "try again in 2
/// minutes" rather than a bare refusal. Anything else is simply closed, as
/// before. Tests no code.
///
/// Bounded to about a second, since it runs on the accept loop's own thread.
pub fn refuse_locked(mut stream: TcpStream, left: Duration) {
    // Accepted from a non-blocking listener, the socket may be non-blocking
    // too (macOS and Windows both pass it on); the timeouts need it blocking.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let mut head = [0u8; OPENING.len() + 1];
    if stream.read_exact(&mut head).is_err() || head[..OPENING.len()] != OPENING {
        return;
    }
    // Drain the client's first message too: closing with its bytes unread
    // makes the OS send a reset, which can arrive before our reply is read.
    let _ = read_handshake_msg(&mut stream);
    let seconds = u32::try_from(left.as_secs().max(1)).unwrap_or(u32::MAX);
    let mut reply = vec![STATUS_LOCKED];
    reply.extend_from_slice(&seconds.to_le_bytes());
    let _ = write_handshake_msg(&mut stream, &reply);
    let _ = stream.shutdown(Shutdown::Write);
}

/// Peek (without consuming) enough bytes to tell whether the peer opened with the
/// encrypted [`PREAMBLE`]. Short-circuits as soon as a byte differs, so a plaintext
/// peer is classified from its very first byte with nothing consumed.
fn peek_is_preamble(stream: &TcpStream) -> io::Result<bool> {
    let mut buf = [0u8; PREAMBLE.len()];
    // ⚠️ A peer that sends part of the preamble and stops is NOT bounded by the
    // socket's read timeout: `peek` keeps returning the bytes already queued, at
    // once, so this loop spun a core at 100% for as long as the peer liked. The
    // deadline and the nap are what bound it.
    let deadline = std::time::Instant::now() + stream.read_timeout()?.unwrap_or(HANDSHAKE_TIMEOUT);
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
        // Saw a matching-but-partial prefix; wait a moment for the rest.
        if std::time::Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "peer stalled mid-preamble"));
        }
        std::thread::sleep(Duration::from_millis(5));
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

    /// Lift the [`begin_handshake`] timeouts once the peer is authenticated, so
    /// the session's reads block for as long as the session lasts.
    ///
    /// # Errors
    /// Returns an error if the underlying calls fail.
    pub fn end_handshake(&self) -> io::Result<()> {
        self.set_read_timeout(None)?;
        self.set_write_timeout(None)
    }

    /// Whether this connection is transport-encrypted.
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        matches!(self, Conn::Secure(_))
    }

    /// How the far end was authenticated.
    #[must_use]
    pub fn auth(&self) -> PeerAuth {
        match self {
            Conn::Plain(_) => PeerAuth::Plaintext,
            Conn::Secure(s) => s.auth,
        }
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
    /// The tunnel itself — cipher state *and* the buffers for a record that
    /// spans reads. Shared between the read and write clones, which is what
    /// keeps nonce order equal to wire order.
    session: Arc<Mutex<Session>>,
    /// How the far end proved itself.
    auth: PeerAuth,
}

impl SecureStream {
    fn new(inner: TcpStream, session: Session, auth: PeerAuth) -> Self {
        SecureStream { inner, session: Arc::new(Mutex::new(session)), auth }
    }

    fn try_clone(&self) -> io::Result<SecureStream> {
        Ok(SecureStream {
            inner: self.inner.try_clone()?,
            session: Arc::clone(&self.session),
            auth: self.auth,
        })
    }
}

impl Read for SecureStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            // Hand back anything already decrypted before touching the socket.
            {
                let mut session = self.session.lock().unwrap();
                let n = session.take(buf);
                if n > 0 {
                    return Ok(n);
                }
            }
            // ⚠️ Read WITHOUT the lock held: a blocking read here while a writer
            // waits to seal would deadlock the session.
            let mut chunk = [0u8; 16 * 1024];
            let n = self.inner.read(&mut chunk)?;
            if n == 0 {
                let session = self.session.lock().unwrap();
                return if session.has_partial_record() {
                    // Cut off mid-record: a truncated stream, not a clean close.
                    Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "stream ended inside a Noise record",
                    ))
                } else {
                    Ok(0)
                };
            }
            self.session.lock().unwrap().feed(&chunk[..n])?;
        }
    }
}

impl Write for SecureStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // Seal under the lock (so nonces increment in order), then release it
        // before the socket write so a concurrent reader can decrypt meanwhile.
        // Correct because exactly one thread ever writes a given direction, so
        // encryption order == wire order.
        let out = self.session.lock().unwrap().seal(buf)?;
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

    /// Cap every socket read in these tests. Long enough that a loaded machine
    /// never trips it, short enough that a broken build reports in seconds.
    const TEST_IO_TIMEOUT: Duration = Duration::from_secs(5);

    /// A connected pair: the far end the test plays, and the host's accepted socket.
    fn socket_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (host, _) = listener.accept().unwrap();
        (peer, host)
    }

    #[test]
    fn begin_and_end_handshake_set_and_lift_the_timeouts() {
        let (_peer, host) = socket_pair();
        begin_handshake(&host);
        assert_eq!(host.read_timeout().unwrap(), Some(HANDSHAKE_TIMEOUT));
        assert_eq!(host.write_timeout().unwrap(), Some(HANDSHAKE_TIMEOUT));
        let conn = Conn::Plain(host);
        conn.end_handshake().unwrap();
        assert_eq!(conn.tcp().read_timeout().unwrap(), None);
        assert_eq!(conn.tcp().write_timeout().unwrap(), None);
    }

    #[test]
    fn a_silent_peer_cannot_hold_accept_open() {
        let (_peer, host) = socket_pair();
        // The same bound `begin_handshake` sets, shortened so the test is quick.
        host.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        let started = std::time::Instant::now();
        assert!(accept(host, 1234).is_err(), "a peer that says nothing must not be served");
        assert!(started.elapsed() < TEST_IO_TIMEOUT);
    }

    #[test]
    fn a_peer_stalled_mid_preamble_times_out_instead_of_spinning() {
        let (mut peer, host) = socket_pair();
        peer.write_all(&PREAMBLE[..2]).unwrap();
        host.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        let started = std::time::Instant::now();
        let err = accept(host, 1234).err().expect("a half-sent preamble must fail");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < TEST_IO_TIMEOUT);
    }

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
            // ⚠️ Bounded so a broken record layer FAILS instead of hanging. Every
            // test below waits on bytes the other end sends, so mis-framing them
            // leaves both ends blocked forever — a wedged CI run that names
            // nothing, rather than a failure that names the break.
            sock.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
            accept(sock, pin_host)
        });
        let client_sock = TcpStream::connect(addr)?;
        client_sock.set_read_timeout(Some(TEST_IO_TIMEOUT))?;
        let client = connect_v1(client_sock, pin_client)?;
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
        let mut client = connect_v1(TcpStream::connect(addr).unwrap(), 7777).unwrap();
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
    /// A **browser-shaped** client against the real host `accept`: the handshake
    /// and every byte of framing come from [`session`], and the socket is used
    /// only as an opaque "send these bytes / here are some bytes" carrier — which
    /// is exactly what a WebSocket is.
    ///
    /// ⚠️ This is the test that makes the socket-free core trustworthy. Its own
    /// unit tests drive both halves from the same module, so a mistake shared by
    /// the sealer and the opener would pass them. Here the peer is the shipped
    /// responder, so the bytes have to be right by the *host's* definition.
    #[test]
    fn a_carrier_with_no_socket_api_completes_the_real_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            // ⚠️ Bounded on BOTH sides. Wrong framing makes each end wait for
            // bytes the other will never send, so without these a mutation turns
            // this test into a hang - which in CI is worse than a failure,
            // because it wedges the run instead of naming the break. Found by
            // mutation-testing exactly that.
            sock.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
            let mut conn = accept(sock, 2468).unwrap();
            assert!(conn.is_encrypted(), "the preamble must select the encrypted path");
            // Read one framed message and answer with another, as a host would.
            let mut len = [0u8; 4];
            conn.read_exact(&mut len).unwrap();
            let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
            conn.read_exact(&mut body).unwrap();
            conn.write_all(&(body.len() as u32).to_le_bytes()).unwrap();
            conn.write_all(&body).unwrap();
            conn.flush().unwrap();
            body
        });

        // --- the "browser" side: bytes in, bytes out, nothing else ------------
        let mut carrier = TcpStream::connect(addr).unwrap();
        carrier.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
        let (init, first) = Initiator::start(2468).unwrap();
        carrier.write_all(&first).unwrap(); // one opaque chunk
        carrier.flush().unwrap();

        // The reply arrives as a chunk; a WebSocket would hand over a whole
        // message, so read what the host sent and pass it in untouched.
        let mut reply = [0u8; 2 + MAX_HANDSHAKE_MSG];
        carrier.read_exact(&mut reply[..2]).unwrap();
        let n = u16::from_le_bytes([reply[0], reply[1]]) as usize;
        carrier.read_exact(&mut reply[2..2 + n]).unwrap();
        let mut session = init.finish(&reply[..2 + n]).unwrap();

        // A real framed protocol message, sealed by the browser-side session.
        let mut framed = (11u32).to_le_bytes().to_vec();
        framed.extend_from_slice(b"page down!!");
        carrier.write_all(&session.seal(&framed).unwrap()).unwrap();
        carrier.flush().unwrap();

        // And the echo back, opened by it.
        while session.available() < framed.len() {
            let mut chunk = [0u8; 1024];
            let got = carrier.read(&mut chunk).expect("host echoed within the timeout");
            assert_ne!(got, 0, "host closed before echoing");
            session.feed(&chunk[..got]).unwrap();
        }
        assert_eq!(session.take_all(), framed);
        assert_eq!(host.join().unwrap(), b"page down!!");
    }

    /// The same carrier with the wrong PIN: the host's responder builds a
    /// different PSK, so the reply cannot authenticate. Proves the browser path
    /// inherits the PIN binding rather than merely being encrypted.
    #[test]
    fn a_carrier_with_the_wrong_pin_is_rejected() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            sock.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
            accept(sock, 1111)
        });

        let mut carrier = TcpStream::connect(addr).unwrap();
        carrier.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
        let (init, first) = Initiator::start(2222).unwrap();
        carrier.write_all(&first).unwrap();
        carrier.flush().unwrap();

        // The host fails first (it reads our message under the wrong PSK), so the
        // client either reads a rejected reply or sees the socket close. Both are
        // failures; neither is a session.
        let mut reply = vec![0u8; 2 + MAX_HANDSHAKE_MSG];
        let outcome = carrier
            .read(&mut reply)
            .map_err(|e| e.to_string())
            .and_then(|n| init.finish(&reply[..n]).map_err(|e| e.to_string()));
        assert!(outcome.is_err(), "a wrong PIN must not yield a session");
        let host_err = host.join().unwrap().err().expect("and the host must reject it too");
        assert!(
            is_pin_rejection(&host_err),
            "a wrong PIN must be countable as a guess: {host_err}"
        );
    }

    /// The host side of a wrong PIN through the real `connect`: marked as a
    /// rejection, so the hosts' `PinGuard` counts it.
    #[test]
    fn a_wrong_pin_is_reported_as_a_pin_rejection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            sock.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
            accept(sock, 1111)
        });
        let sock = TcpStream::connect(addr).unwrap();
        sock.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
        assert!(connect_v1(sock, 2222).is_err());
        let err = host.join().unwrap().err().expect("host must reject a wrong PIN");
        assert!(is_pin_rejection(&err), "{err}");
    }

    /// A peer that sends the preamble and hangs up has tested no PIN, so it must
    /// NOT count — otherwise anyone could lock the owner out without guessing.
    #[test]
    fn a_peer_that_hangs_up_mid_handshake_is_not_a_pin_rejection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            sock.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
            accept(sock, 1111)
        });
        let mut sock = TcpStream::connect(addr).unwrap();
        sock.write_all(&PREAMBLE).unwrap();
        sock.flush().unwrap();
        drop(sock);
        let err = host.join().unwrap().err().expect("a truncated handshake is an error");
        assert!(!is_pin_rejection(&err), "{err}");
    }

    /// Ordinary I/O and framing errors are not guesses either.
    #[test]
    fn a_plain_io_error_is_not_a_pin_rejection() {
        let e = io::Error::new(io::ErrorKind::UnexpectedEof, "gone");
        assert!(!is_pin_rejection(&e));
        assert!(!is_pin_rejection(&noise_err("handshake message exceeds the maximum size")));
    }

    // ---- v2: pairing with SPAKE2, reconnecting with remembered keys --------

    /// A v2 host on a loopback port, running `accept_with` against `store`
    /// once, handing back what it returned and the store.
    fn v2_host(
        pin: u32,
        store: PairingStore,
    ) -> (std::net::SocketAddr, thread::JoinHandle<(io::Result<Conn>, PairingStore)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let host = thread::spawn(move || {
            let mut store = store;
            let (sock, peer) = listener.accept().unwrap();
            sock.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
            let conn = accept_with(sock, pin, &mut store, &peer.to_string());
            (conn, store)
        });
        (addr, host)
    }

    fn client_sock(addr: std::net::SocketAddr) -> TcpStream {
        let s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
        s
    }

    fn round_trip(client: &mut Conn, server: &mut Conn) {
        client.write_all(b"ping over v2").unwrap();
        let mut got = [0u8; 12];
        server.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"ping over v2");
        server.write_all(b"pong").unwrap();
        let mut back = [0u8; 4];
        client.read_exact(&mut back).unwrap();
        assert_eq!(&back, b"pong");
    }

    /// Pair `client` with a host whose store is `host_store`; return the host's
    /// store afterwards and the host key the client learned.
    /// What [`pair_once`] hands back: the client's result, the host's, and the
    /// host's store afterwards.
    type PairOutcome = (io::Result<(Conn, [u8; 32])>, io::Result<Conn>, PairingStore);

    fn pair_once(pin_host: u32, pin_client: u32, host_store: PairingStore, client: &Identity) -> PairOutcome {
        let (addr, host) = v2_host(pin_host, host_store);
        let c = connect_pair(client_sock(addr), pin_client, client);
        let (h, store) = host.join().unwrap();
        (c, h, store)
    }

    #[test]
    fn pairing_with_the_right_code_opens_the_tunnel_and_both_ends_learn_the_keys() {
        let host_store = PairingStore::ephemeral().unwrap();
        let host_public = host_store.identity().public();
        let me = Identity::generate().unwrap();
        let (c, h, store) = pair_once(4321, 4321, host_store, &me);
        let (mut client, host_key) = c.unwrap();
        let mut server = h.unwrap();
        assert_eq!(host_key, host_public, "the client learned the host's real key");
        assert_eq!(client.auth(), PeerAuth::Paired { key: host_public });
        assert_eq!(server.auth(), PeerAuth::Paired { key: me.public() });
        assert!(server.auth().settles_pin());
        assert!(store.is_paired(&me.public()), "the host remembers the device it paired");
        round_trip(&mut client, &mut server);
    }

    #[test]
    fn a_remembered_device_reconnects_with_no_code_even_after_the_pin_changed() {
        let me = Identity::generate().unwrap();
        let (c, _h, store) = pair_once(4321, 4321, PairingStore::ephemeral().unwrap(), &me);
        let (_, host_key) = c.unwrap();

        // The host restarted with a fresh PIN; the device has no code at all.
        let (addr, host) = v2_host(9876, store);
        let (mut client, key) = connect_known(client_sock(addr), &me, vec![host_key]).unwrap();
        let (h, _) = host.join().unwrap();
        let mut server = h.unwrap();
        assert_eq!(key, host_key);
        assert_eq!(server.auth(), PeerAuth::Known { key: me.public() });
        round_trip(&mut client, &mut server);
    }

    #[test]
    fn a_forgotten_device_is_told_to_pair_again_and_it_is_not_a_guess() {
        let me = Identity::generate().unwrap();
        let (c, _h, mut store) = pair_once(4321, 4321, PairingStore::ephemeral().unwrap(), &me);
        let (_, host_key) = c.unwrap();
        store.forget_all().unwrap();

        let (addr, host) = v2_host(4321, store);
        let err = connect_known(client_sock(addr), &me, vec![host_key]).err().expect("forgotten");
        assert_eq!(failure(&err), Some(Failure::NotPaired), "{err}");
        let (h, _) = host.join().unwrap();
        let host_err = h.err().expect("the host refuses it");
        assert!(!is_pin_rejection(&host_err), "no code was tested: {host_err}");
    }

    #[test]
    fn a_stranger_answering_is_refused_before_the_client_reveals_its_key() {
        let me = Identity::generate().unwrap();
        let stranger = PairingStore::ephemeral().unwrap();
        let (addr, host) = v2_host(4321, stranger);
        let trusted = vec![[0x11u8; 32]]; // some other host's key
        let err = connect_known(client_sock(addr), &me, trusted).err().expect("not our host");
        assert_eq!(failure(&err), Some(Failure::UnknownHost), "{err}");
        let (h, store) = host.join().unwrap();
        let host_err = h.err().expect("the client hung up before message 3");
        assert!(!is_pin_rejection(&host_err));
        assert!(!store.is_paired(&me.public()));
    }

    /// The online side of the fix: a wrong code is tested by the host exactly
    /// once per connection, reported so the lockout counts it, and the
    /// connection ends there.
    #[test]
    fn a_wrong_code_costs_exactly_one_counted_attempt() {
        let me = Identity::generate().unwrap();
        let mut guard = PinGuard::new();
        let (c, h, store) = pair_once(1111, 2222, PairingStore::ephemeral().unwrap(), &me);
        let err = c.err().expect("a wrong code must not open a tunnel");
        assert_eq!(failure(&err), Some(Failure::WrongCode), "{err}");
        let host_err = h.err().expect("the host refuses it");
        assert!(is_pin_rejection(&host_err), "a wrong code must count: {host_err}");
        guard.record_failure(std::time::Instant::now());
        assert_eq!(guard.failures(), 1);
        assert!(!store.is_paired(&me.public()));
    }

    /// A peer that sends the opening and its SPAKE2 message, takes the host's,
    /// and hangs up has tested nothing — the host sent nothing keyed by the
    /// code — so it must not count, or anyone could lock the owner out.
    #[test]
    fn a_peer_that_hangs_up_after_the_pake_messages_is_not_a_guess() {
        let me = Identity::generate().unwrap();
        let (addr, host) = v2_host(1111, PairingStore::ephemeral().unwrap());
        let mut sock = client_sock(addr);
        let (mut hs, first) = ClientHandshake::pair(2222, &me).unwrap();
        sock.write_all(&first).unwrap();
        let mut chunk = [0u8; 256];
        // The host's reply may arrive in pieces; feed until the handshake has
        // its proof ready to send — and then never send it.
        loop {
            let n = sock.read(&mut chunk).unwrap();
            assert_ne!(n, 0, "the host hung up first");
            if matches!(hs.feed(&chunk[..n]).unwrap(), Step::Continue(ref out) if !out.is_empty()) {
                break;
            }
        }
        drop(sock);
        let (h, _) = host.join().unwrap();
        assert!(!is_pin_rejection(&h.err().expect("an abandoned handshake is an error")));
    }

    #[test]
    fn a_host_with_pairing_off_lets_anyone_in_and_remembers_nobody() {
        let me = Identity::generate().unwrap();
        let (c, h, store) = pair_once(0, 0, PairingStore::ephemeral().unwrap(), &me);
        let (_, host_key) = c.unwrap();
        assert!(h.is_ok());
        assert!(store.peers().is_empty(), "nothing to remember when nobody is kept out");
        // Known mode is let in too, without being on any list.
        let (addr, host) = v2_host(0, store);
        assert!(connect_known(client_sock(addr), &me, vec![host_key]).is_ok());
        assert!(host.join().unwrap().0.is_ok());
    }

    /// The compatibility call: a v0.3 client (v1, `NNpsk0`) still gets in.
    #[test]
    fn a_v1_client_is_still_accepted_and_marked_legacy() {
        let (addr, host) = v2_host(4321, PairingStore::ephemeral().unwrap());
        let mut client = connect_v1(client_sock(addr), 4321).unwrap();
        let (h, store) = host.join().unwrap();
        let mut server = h.unwrap();
        assert_eq!(server.auth(), PeerAuth::LegacyPin);
        assert!(!server.auth().settles_pin(), "a v1 client still sends its PIN in the hello");
        assert!(store.peers().is_empty());
        round_trip(&mut client, &mut server);
    }

    /// What a v0.3 host does with a v2 opening — read the preamble and a
    /// length, find it over its 4 KiB bound, close — reproduced line for line
    /// from v0.3's `accept_encrypted`. The client must say "older host", fast.
    #[test]
    fn a_v2_client_facing_a_v03_host_is_told_the_host_is_older() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let old_host = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut pre = [0u8; PREAMBLE.len()];
            sock.read_exact(&mut pre).unwrap();
            assert!(read_handshake_msg(&mut sock).is_err(), "v0.3 refuses the escape length");
        });
        let me = Identity::generate().unwrap();
        let started = std::time::Instant::now();
        let err = connect_pair(client_sock(addr), 4321, &me).err().expect("v0.3 cannot pair");
        assert_eq!(failure(&err), Some(Failure::OlderHost), "{err}");
        assert!(started.elapsed() < TEST_IO_TIMEOUT, "it must not wait for a timeout");
        old_host.join().unwrap();
    }

    #[test]
    fn a_locked_host_tells_a_v2_client_how_long_to_wait() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let host = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            refuse_locked(sock, Duration::from_secs(120));
        });
        let me = Identity::generate().unwrap();
        let err = connect_pair(client_sock(addr), 4321, &me).err().expect("locked");
        assert_eq!(failure(&err), Some(Failure::HostLocked { seconds: 120 }), "{err}");
        host.join().unwrap();
    }

    #[test]
    fn an_unknown_handshake_version_is_answered_not_misread() {
        let (addr, host) = v2_host(4321, PairingStore::ephemeral().unwrap());
        let mut sock = client_sock(addr);
        let mut opening = OPENING.to_vec();
        *opening.last_mut().unwrap() = 3; // a future v3
        opening.push(MODE_PAIR);
        sock.write_all(&opening).unwrap();
        let reply = read_handshake_msg(&mut sock).unwrap();
        assert_eq!(reply, vec![STATUS_UNSUPPORTED]);
        let (h, _) = host.join().unwrap();
        assert!(!is_pin_rejection(&h.err().unwrap()));
    }

    // ---- the offline attack, on a recording ---------------------------------

    /// Everything that crossed the wire during one pairing, as an eavesdropper
    /// on the network would record it.
    struct Recording {
        /// Client → host bytes.
        up: Vec<u8>,
        /// Host → client bytes.
        down: Vec<u8>,
    }

    /// Pair once with the real client state machine against the real host,
    /// keeping a copy of every byte each way.
    fn record_v2_pairing(pin: u32) -> Recording {
        let (addr, host) = v2_host(pin, PairingStore::ephemeral().unwrap());
        let me = Identity::generate().unwrap();
        let mut sock = client_sock(addr);
        let (mut hs, first) = ClientHandshake::pair(pin, &me).unwrap();
        let mut rec = Recording { up: first.clone(), down: Vec::new() };
        sock.write_all(&first).unwrap();
        let mut chunk = [0u8; 4096];
        let mut session = loop {
            let n = sock.read(&mut chunk).unwrap();
            assert_ne!(n, 0, "the host hung up");
            rec.down.extend_from_slice(&chunk[..n]);
            match hs.feed(&chunk[..n]).unwrap() {
                Step::Continue(out) => {
                    rec.up.extend_from_slice(&out);
                    sock.write_all(&out).unwrap();
                }
                Step::Done { send, established } => {
                    rec.up.extend_from_slice(&send);
                    sock.write_all(&send).unwrap();
                    break established.session;
                }
            }
        };
        // Some application data too, so the recording is a real session.
        let sealed = session.seal(b"\x05\x00\x00\x00hello").unwrap();
        rec.up.extend_from_slice(&sealed);
        sock.write_all(&sealed).unwrap();
        let _ = host.join().unwrap().0.unwrap();
        rec
    }

    /// The v1 recording, for the control experiment.
    fn record_v1_pairing(pin: u32) -> Recording {
        let (init, first) = Initiator::start(pin).unwrap();
        let _ = init; // the attacker never sees the client's state
        Recording { up: first, down: Vec::new() }
    }

    /// The best an offline attacker can do with a v2 recording, for one
    /// guessed code: compute a SPAKE2 key over the guess — the only way to get
    /// one without the secret scalars is to play a side itself — derive the
    /// PSK as the protocol does, and see whether the client's first Noise
    /// message authenticates under it. Returns true if the guess "checks out".
    fn v2_guess_checks_out(rec: &Recording, guess: u32) -> bool {
        // Parse the recording exactly as the host does.
        let up = &rec.up;
        assert_eq!(&up[..OPENING.len()], &OPENING[..]);
        let mut rest = up[OPENING.len() + 1..].to_vec();
        let msg_a = handshake::take_frame(&mut rest).unwrap().unwrap();
        let m1 = handshake::take_frame(&mut rest).unwrap().unwrap();
        let mut down = rec.down.clone();
        let reply = handshake::take_frame(&mut down).unwrap().unwrap();

        let mut prologue = up[..OPENING.len() + 1].to_vec();
        prologue.extend_from_slice(&handshake::frame(&msg_a).unwrap());
        prologue.extend_from_slice(&handshake::frame(&reply).unwrap());

        // Play the responder over the guess against the recorded A.
        let (pake, _) = Pake::start(Role::Responder, &code_bytes(guess), PAIR_CONTEXT);
        let key = pake.finish(&msg_a).unwrap();
        let psk = key.derive(PSK_LABEL);
        // Any 32 bytes are a valid X25519 secret; the attacker's own static
        // key plays no part in authenticating message 1.
        let attacker_static = [0x5Au8; 32];
        let mut hs = snow::Builder::new(handshake::pair_params().unwrap())
            .local_private_key(&attacker_static)
            .psk(0, &psk)
            .prologue(&prologue)
            .build_responder()
            .unwrap();
        let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
        if hs.read_message(&m1, &mut scratch).is_ok() {
            return true;
        }
        // And the v1 attack, in case anything in v2 were still a function of
        // the PIN alone: `NNpsk0` with SHA-256(PIN) against the same message.
        let mut v1 = snow::Builder::new(noise_params().unwrap())
            .psk(0, &derive_psk(guess))
            .build_responder()
            .unwrap();
        v1.read_message(&m1, &mut scratch).is_ok()
    }

    /// The v1 attack on a v1 recording: rebuild the host's side for a guess
    /// and see whether the client's first message authenticates.
    fn v1_guess_checks_out(rec: &Recording, guess: u32) -> bool {
        let mut rest = rec.up[PREAMBLE.len()..].to_vec();
        let m1 = handshake::take_frame(&mut rest).unwrap().unwrap();
        let mut hs = snow::Builder::new(noise_params().unwrap())
            .psk(0, &derive_psk(guess))
            .build_responder()
            .unwrap();
        let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
        hs.read_message(&m1, &mut scratch).is_ok()
    }

    /// **The bug, and the fix, as one test.** Against a v1 recording, trying
    /// all 10,000 codes offline finds the PIN — that is the attack the backlog
    /// reported, and seeing it succeed here proves the attacker below is a
    /// real one. Against a v2 recording of the same PIN the identical search
    /// finds nothing: no guess, the right one included, authenticates
    /// anything that was recorded.
    #[test]
    fn an_offline_brute_force_of_a_recorded_pairing_finds_nothing() {
        const PIN: u32 = 7305;

        let v1 = record_v1_pairing(PIN);
        let found: Vec<u32> = (0..10_000).filter(|&g| v1_guess_checks_out(&v1, g)).collect();
        assert_eq!(found, vec![PIN], "control: the v1 recording gives the PIN away");

        let v2 = record_v2_pairing(PIN);
        let found: Vec<u32> = (0..10_000).filter(|&g| v2_guess_checks_out(&v2, g)).collect();
        assert!(found.is_empty(), "a v2 recording let these codes be tested offline: {found:?}");
    }

    /// And the code itself never crosses the wire in any form a recording
    /// could match against: not as digits, not as the v1 PSK.
    #[test]
    fn a_v2_recording_contains_neither_the_code_nor_a_hash_of_it() {
        let rec = record_v2_pairing(7305);
        let all = [rec.up.as_slice(), rec.down.as_slice()].concat();
        assert!(!all.windows(4).any(|w| w == b"7305"));
        let psk = derive_psk(7305);
        assert!(!all.windows(8).any(|w| w == &psk[..8]));
    }
}
