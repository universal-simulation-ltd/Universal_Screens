//! The **v2** handshake's wire constants and failure kinds, shared by the host
//! (`lib.rs`, which has sockets) and the client state machine (`session.rs`,
//! which has none).
//!
//! ## Why there is a v2
//!
//! v1 (`Noise_NNpsk0`, PSK = `SHA-256(domain ‖ PIN)`) let anyone who recorded
//! one connection try all 10,000 PINs offline: the first handshake message is
//! authenticated with a key that is a fixed function of the PIN. v2 never puts
//! anything derived from the code alone on the wire:
//!
//! - **Pairing** (`MODE_PAIR`): SPAKE2 over the code (`unisim-pake`), then
//!   `Noise_XXpsk0` keyed by the SPAKE2 result. The PSK depends on secret
//!   scalars that never cross the wire, so a recording is useless for guessing,
//!   and an active guesser gets one try per connection — which the host counts.
//!   Inside it the two exchange their long-term static keys and remember them.
//! - **Reconnecting** (`MODE_KNOWN`): `Noise_XX` with the remembered static
//!   keys, no code at all. Each end checks the other's key against its list.
//!
//! ## The opening, and why it starts like v1
//!
//! ```text
//! v1:  "USCR" 0x01 | u16 LE len (48) | Noise msg 1
//! v2:  "USCR" 0x01 | 0xFF 0xFF       | 0x02 | mode | …
//! ```
//!
//! A v2 opening starts with the v1 [`PREAMBLE`](crate::PREAMBLE), then a length
//! of `0xFFFF` — reserved as an **escape**, since no v1 handshake message is
//! anywhere near that long — then the handshake version. Two consequences, both
//! deliberate:
//!
//! - **A v0.3 host refuses a v2 client at once and cleanly.** It reads the
//!   preamble, sees a handshake message longer than its 4 KiB bound, and closes —
//!   not counted as a wrong PIN. (A fresh preamble byte would have looked like a
//!   plaintext frame of 1.4 GB to it, which it would try to allocate, then wait
//!   for until the client gave up.)
//! - **Every bridge already relays it verbatim.** The web bridge inside a host
//!   switches to byte-pipe mode on the v1 preamble, so a browser's v2 opening
//!   passes through any bridge that can pass v1.
//!
//! A future v3 bumps the version byte; a v2 host answers it with
//! [`STATUS_UNSUPPORTED`] rather than misreading it.

use std::fmt;
use std::io;

use crate::{noise_err, PREAMBLE};

/// The version byte that follows the escape.
pub const HANDSHAKE_VERSION: u8 = 2;

/// The length value a v1 client never sends, reserved as the version escape.
pub const VERSION_ESCAPE: [u8; 2] = [0xFF, 0xFF];

/// The first eight bytes of every v2 connection.
pub const OPENING: [u8; 8] = [
    PREAMBLE[0],
    PREAMBLE[1],
    PREAMBLE[2],
    PREAMBLE[3],
    PREAMBLE[4],
    VERSION_ESCAPE[0],
    VERSION_ESCAPE[1],
    HANDSHAKE_VERSION,
];

/// Mode byte: pair over a code (SPAKE2, then `XXpsk0`).
pub const MODE_PAIR: u8 = b'P';
/// Mode byte: reconnect with remembered keys (`XX`, no code).
pub const MODE_KNOWN: u8 = b'K';

/// First byte of the host's first reply: carry on.
pub const STATUS_GO: u8 = 0;
/// First byte of the host's first reply: locked after wrong codes; a `u32` LE
/// count of seconds left follows.
pub const STATUS_LOCKED: u8 = 1;
/// First byte of the host's first reply: a version or mode it does not speak.
pub const STATUS_UNSUPPORTED: u8 = 2;

/// The host's first encrypted record, once the handshake is done: welcome.
pub const VERDICT_OK: u8 = 0;
/// The host's first encrypted record: the keys checked out, but this host has
/// not paired with this device (or has forgotten it). Pair with the code.
pub const VERDICT_NOT_PAIRED: u8 = 1;

/// SPAKE2 context: a code typed for anything else cannot complete this.
pub const PAIR_CONTEXT: &[u8] = b"universal-screens/pairing/v2";

/// The pairing handshake: XX (both static keys, sent encrypted) with the
/// SPAKE2-derived key mixed in from the first message.
pub const NOISE_PAIR: &str = "Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s";

/// The reconnecting handshake: XX, keys only.
pub const NOISE_KNOWN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// HKDF label for the Noise PSK, derived from the SPAKE2 key.
pub(crate) const PSK_LABEL: &[u8] = b"noise-psk";

/// The code as SPAKE2 sees it: the PIN as the four digits a person types.
#[must_use]
pub fn code_bytes(pin: u32) -> Vec<u8> {
    format!("{pin:04}").into_bytes()
}

pub(crate) fn pair_params() -> io::Result<snow::params::NoiseParams> {
    NOISE_PAIR.parse().map_err(noise_err)
}

pub(crate) fn known_params() -> io::Result<snow::params::NoiseParams> {
    NOISE_KNOWN.parse().map_err(noise_err)
}

/// Why a v2 handshake did not produce a session, in terms a person can act on.
/// Carried inside the `io::Error`; read it back with [`failure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The code did not match the host's.
    WrongCode,
    /// The host is pausing after too many wrong codes.
    HostLocked {
        /// Seconds until it takes another attempt.
        seconds: u32,
    },
    /// The host does not know this device (never paired, or forgotten).
    NotPaired,
    /// The far end proved a key this device has not paired with.
    UnknownHost,
    /// The far end hung up without a word on the first message: an older
    /// Universal Screens (v0.3 or before), or not Universal Screens.
    OlderHost,
    /// The host speaks a different handshake version or mode.
    Unsupported,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::WrongCode => f.write_str("the code does not match the one the host shows"),
            Failure::HostLocked { seconds } => write!(
                f,
                "the host is pausing after too many wrong codes; try again in {seconds} s"
            ),
            Failure::NotPaired => f.write_str("not paired: this device must pair again with the code"),
            Failure::UnknownHost => f.write_str("this is not a computer this device has paired with"),
            Failure::OlderHost => f.write_str(
                "the host closed the connection at once; it may run an older Universal Screens (update it)",
            ),
            Failure::Unsupported => f.write_str("the host speaks a different version of the pairing handshake"),
        }
    }
}

impl std::error::Error for Failure {}

impl Failure {
    /// As an `io::Error` whose inner error is this `Failure`.
    #[must_use]
    pub fn into_io(self) -> io::Error {
        let kind = match self {
            Failure::WrongCode | Failure::NotPaired | Failure::UnknownHost => io::ErrorKind::PermissionDenied,
            Failure::HostLocked { .. } => io::ErrorKind::WouldBlock,
            Failure::OlderHost => io::ErrorKind::ConnectionAborted,
            Failure::Unsupported => io::ErrorKind::Unsupported,
        };
        io::Error::new(kind, self)
    }
}

/// The [`Failure`] inside `e`, if it is one.
#[must_use]
pub fn failure(e: &io::Error) -> Option<Failure> {
    e.get_ref().and_then(|inner| inner.downcast_ref::<Failure>()).copied()
}

/// `u16`-LE length + body, for one handshake message.
pub(crate) fn frame(body: &[u8]) -> io::Result<Vec<u8>> {
    let len = u16::try_from(body.len()).map_err(|_| noise_err("handshake message too large"))?;
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(body);
    Ok(out)
}

/// Take one whole `u16`-LE-framed message off the front of `buf`, or `None`
/// while it is still incomplete.
pub(crate) fn take_frame(buf: &mut Vec<u8>) -> io::Result<Option<Vec<u8>>> {
    let Some(len_bytes) = buf.get(..2) else { return Ok(None) };
    let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
    if len > crate::MAX_HANDSHAKE_MSG {
        return Err(noise_err("handshake message exceeds the maximum size"));
    }
    if buf.len() < 2 + len {
        return Ok(None);
    }
    let body = buf[2..2 + len].to_vec();
    buf.drain(..2 + len);
    Ok(Some(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_opening_starts_with_the_v1_preamble() {
        assert_eq!(&OPENING[..PREAMBLE.len()], &PREAMBLE[..]);
    }

    #[test]
    fn the_escape_is_longer_than_any_handshake_message_a_v1_host_reads() {
        assert!(u16::from_le_bytes(VERSION_ESCAPE) as usize > crate::MAX_HANDSHAKE_MSG);
    }

    #[test]
    fn codes_keep_their_leading_zeros() {
        assert_eq!(code_bytes(42), b"0042");
        assert_eq!(code_bytes(0), b"0000");
        assert_eq!(code_bytes(9999), b"9999");
    }

    #[test]
    fn a_failure_survives_the_trip_through_io_error() {
        let e = Failure::HostLocked { seconds: 9 }.into_io();
        assert_eq!(failure(&e), Some(Failure::HostLocked { seconds: 9 }));
        assert_eq!(failure(&io::Error::other("x")), None);
    }

    #[test]
    fn the_noise_patterns_parse() {
        assert!(pair_params().is_ok());
        assert!(known_params().is_ok());
    }
}
