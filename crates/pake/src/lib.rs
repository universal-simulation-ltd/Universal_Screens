//! Short-code pairing: turn a code a person reads off one screen and types into
//! another into a strong shared key — without ever giving an eavesdropper
//! something to test codes against.
//!
//! ## The problem it solves
//!
//! Hashing a short code into a key (`key = H(code)`) and encrypting with it looks
//! like it works, and fails quietly: anyone who records **one** exchange can try
//! every code offline against what they recorded. A 4-digit code is 10,000
//! tries, which takes milliseconds. A wrong-guess lockout does not help, because
//! the guessing never touches the device.
//!
//! A PAKE (password-authenticated key exchange) closes that hole. Each side
//! mixes the code into a fresh Diffie-Hellman exchange, so:
//!
//! - **A passive eavesdropper learns nothing it can test a code against.** The
//!   key also depends on a secret random number each side never sends.
//! - **An active attacker gets one guess per attempt.** To test a code it has to
//!   run the exchange with a real device, which can count the failure and
//!   lock it out — the same device the attacker has to reach.
//!
//! ## Shape
//!
//! Bytes in, bytes out — no socket, no async, no transport. Both sides call
//! [`Pake::start`], send the 33-byte message it returns, and pass what they
//! receive to [`Pake::finish`]. Neither message depends on the other, so they
//! can cross on the wire.
//!
//! ⚠️ **`finish` succeeds even when the codes differ.** That is how a PAKE works:
//! a wrong code does not fail the exchange, it produces a *different key*. The
//! caller MUST then prove both sides hold the same key before trusting it —
//! either by running an authenticated protocol keyed by it (Universal Screens
//! keys its Noise handshake with [`SharedKey::derive`]) or with the explicit
//! [`SharedKey::confirmation`] tags here. And to keep "one guess per attempt"
//! true, the side that *shows* the code should insist on seeing the other side's
//! proof **first**, before it sends anything keyed by the result, and should
//! count a bad proof as a wrong guess.
//!
//! ## Why SPAKE2, and this crate
//!
//! See `README.md` beside this file.

#![forbid(unsafe_code)]

use std::fmt;

use hkdf::Hkdf;
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use subtle::ConstantTimeEq;

#[cfg(feature = "wasm")]
pub mod wasm;

/// Length of each side's message: one side-tag byte plus a 32-byte point.
pub const MESSAGE_LEN: usize = 33;

/// Version tag mixed into everything this crate derives. Bump it if the
/// derivation ever changes, and two builds that disagree will fail to pair
/// rather than pair with different keys.
const DOMAIN: &[u8] = b"unisim-pake/v1";

/// Which end of the exchange this is. The two ends must differ.
///
/// By convention the **initiator** is the device the person types the code
/// into, and the **responder** is the device that shows it — the one that
/// should count wrong guesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Types the code (a phone, a browser tab).
    Initiator,
    /// Shows the code (the computer being joined).
    Responder,
}

impl Role {
    fn label(self) -> &'static [u8] {
        match self {
            Role::Initiator => b"initiator",
            Role::Responder => b"responder",
        }
    }
}

/// Why an exchange could not finish. None of these mean "wrong code" — a wrong
/// code finishes, with a different key (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PakeError {
    /// The peer's message is not [`MESSAGE_LEN`] bytes.
    WrongLength,
    /// The peer's message claims the same role as ours, so both sides started
    /// as initiator (or both as responder).
    SameRole,
    /// The peer's message is not a valid curve point.
    Corrupt,
}

impl fmt::Display for PakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PakeError::WrongLength => "pairing message has the wrong length",
            PakeError::SameRole => "both devices started the pairing from the same side",
            PakeError::Corrupt => "pairing message is not valid",
        })
    }
}

impl std::error::Error for PakeError {}

/// One side of an exchange in progress.
pub struct Pake {
    inner: Spake2<Ed25519Group>,
    context: Vec<u8>,
}

impl Pake {
    /// Begin an exchange as `role`, over `code`, within `context`.
    ///
    /// `context` names what is being paired — `b"universal-screens/pairing"`,
    /// say — so a code typed for one app can never complete an exchange for
    /// another. Both sides must pass the same bytes.
    ///
    /// Returns the [`MESSAGE_LEN`]-byte message to send to the peer.
    ///
    /// Normalise typed codes first ([`normalise_code`]) so that `12 34` and
    /// `1234` pair.
    #[must_use]
    pub fn start(role: Role, code: &[u8], context: &[u8]) -> (Pake, Vec<u8>) {
        let password = Password::new(code);
        let id_a = Identity::new(&identity(Role::Initiator, context));
        let id_b = Identity::new(&identity(Role::Responder, context));
        let (inner, msg) = match role {
            Role::Initiator => Spake2::<Ed25519Group>::start_a(&password, &id_a, &id_b),
            Role::Responder => Spake2::<Ed25519Group>::start_b(&password, &id_a, &id_b),
        };
        (Pake { inner, context: context.to_vec() }, msg)
    }

    /// Finish with the peer's message, producing the shared key.
    ///
    /// ⚠️ A wrong code is **not** an error here: it yields a key the other side
    /// does not have. Prove agreement before trusting it — see the module docs.
    ///
    /// # Errors
    /// A malformed message, or one from a peer playing the same role.
    pub fn finish(self, peer_message: &[u8]) -> Result<SharedKey, PakeError> {
        let key = self.inner.finish(peer_message).map_err(|e| match e {
            spake2::Error::WrongLength => PakeError::WrongLength,
            spake2::Error::BadSide => PakeError::SameRole,
            spake2::Error::CorruptMessage => PakeError::Corrupt,
        })?;
        let mut out = [0u8; 32];
        // spake2 returns SHA-256(transcript): exactly 32 bytes.
        out.copy_from_slice(&key[..32]);
        Ok(SharedKey { key: out, context: self.context })
    }
}

/// The SPAKE2 identities, bound to the role and the caller's context.
fn identity(role: Role, context: &[u8]) -> Vec<u8> {
    let mut id = Vec::with_capacity(DOMAIN.len() + 12 + context.len());
    id.extend_from_slice(DOMAIN);
    id.push(b'/');
    id.extend_from_slice(role.label());
    id.push(b'/');
    id.extend_from_slice(context);
    id
}

/// The result of an exchange. Equal on both sides **only if the codes matched**.
///
/// Never use it directly as a cipher key: [`derive`](SharedKey::derive) one key
/// per purpose, so a key used for one job can never be replayed into another.
pub struct SharedKey {
    key: [u8; 32],
    context: Vec<u8>,
}

impl SharedKey {
    /// A 32-byte key for one purpose, named by `label` (HKDF-SHA256).
    #[must_use]
    pub fn derive(&self, label: &[u8]) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(DOMAIN), &self.key);
        let mut info = Vec::with_capacity(self.context.len() + 1 + label.len());
        info.extend_from_slice(&self.context);
        info.push(0);
        info.extend_from_slice(label);
        let mut out = [0u8; 32];
        hk.expand(&info, &mut out).expect("32 bytes is a valid HKDF-SHA256 output length");
        out
    }

    /// The tag `role` sends to prove it holds this key. For a consumer that
    /// does not key an authenticated protocol with the result.
    ///
    /// The two roles' tags differ, so a tag cannot simply be reflected back.
    #[must_use]
    pub fn confirmation(&self, role: Role) -> [u8; 32] {
        let mut label = b"confirm/".to_vec();
        label.extend_from_slice(role.label());
        self.derive(&label)
    }

    /// Check the tag the peer, playing `peer_role`, sent. Constant-time.
    #[must_use]
    pub fn verify_confirmation(&self, peer_role: Role, tag: &[u8]) -> bool {
        tag.len() == 32 && bool::from(self.confirmation(peer_role).ct_eq(tag))
    }
}

impl Drop for SharedKey {
    fn drop(&mut self) {
        // Best effort: not a guarantee against copies the optimiser made, but
        // it keeps the key out of freed memory in the ordinary case.
        self.key = [0u8; 32];
    }
}

impl fmt::Debug for SharedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SharedKey(..)")
    }
}

/// A typed code as the bytes to pair over: spaces, dashes and dots removed,
/// letters upper-cased. So `ab-12 3` and `AB123` pair.
///
/// Digits-only codes come out as their ASCII digits, **leading zeros kept**:
/// `0042` is not `42`.
#[must_use]
pub fn normalise_code(typed: &str) -> Vec<u8> {
    typed
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '.' | '_'))
        .flat_map(char::to_uppercase)
        .collect::<String>()
        .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: &[u8] = b"test/pairing";

    fn run(code_i: &[u8], code_r: &[u8]) -> (SharedKey, SharedKey) {
        let (i, msg_i) = Pake::start(Role::Initiator, code_i, CTX);
        let (r, msg_r) = Pake::start(Role::Responder, code_r, CTX);
        (i.finish(&msg_r).unwrap(), r.finish(&msg_i).unwrap())
    }

    #[test]
    fn the_same_code_gives_the_same_key() {
        let (ki, kr) = run(b"4821", b"4821");
        assert_eq!(ki.derive(b"x"), kr.derive(b"x"));
        assert!(kr.verify_confirmation(Role::Initiator, &ki.confirmation(Role::Initiator)));
        assert!(ki.verify_confirmation(Role::Responder, &kr.confirmation(Role::Responder)));
    }

    #[test]
    fn a_different_code_gives_a_different_key_and_confirmation_fails() {
        let (ki, kr) = run(b"4821", b"4822");
        assert_ne!(ki.derive(b"x"), kr.derive(b"x"));
        assert!(!kr.verify_confirmation(Role::Initiator, &ki.confirmation(Role::Initiator)));
    }

    #[test]
    fn messages_are_fresh_every_time() {
        let (_, a) = Pake::start(Role::Initiator, b"4821", CTX);
        let (_, b) = Pake::start(Role::Initiator, b"4821", CTX);
        assert_eq!(a.len(), MESSAGE_LEN);
        assert_ne!(a, b, "a message that repeated would be a fixed function of the code");
    }

    #[test]
    fn labels_and_contexts_separate_keys() {
        let (ki, _) = run(b"1", b"1");
        assert_ne!(ki.derive(b"a"), ki.derive(b"b"));
        assert_ne!(ki.confirmation(Role::Initiator), ki.confirmation(Role::Responder));

        let (i, msg_i) = Pake::start(Role::Initiator, b"1", b"app-one");
        let (r, msg_r) = Pake::start(Role::Responder, b"1", b"app-two");
        let (ki, kr) = (i.finish(&msg_r).unwrap(), r.finish(&msg_i).unwrap());
        assert_ne!(ki.derive(b"x"), kr.derive(b"x"), "a code typed for one app must not pair another");
    }

    #[test]
    fn two_initiators_are_refused() {
        let (a, _) = Pake::start(Role::Initiator, b"1", CTX);
        let (_, other) = Pake::start(Role::Initiator, b"1", CTX);
        assert_eq!(a.finish(&other).unwrap_err(), PakeError::SameRole);
    }

    #[test]
    fn malformed_messages_are_errors_not_panics() {
        let (a, _) = Pake::start(Role::Initiator, b"1", CTX);
        assert_eq!(a.finish(&[0x42; 5]).unwrap_err(), PakeError::WrongLength);
        // Right length and side tag, but a y-coordinate with no point on the
        // curve. About half of all y values are like that; find one rather
        // than hard-code it.
        let corrupt = (2u8..=255).any(|y| {
            let mut bad = [0u8; MESSAGE_LEN];
            bad[0] = b'B';
            bad[1] = y;
            let (a, _) = Pake::start(Role::Initiator, b"1", CTX);
            a.finish(&bad).err() == Some(PakeError::Corrupt)
        });
        assert!(corrupt, "an off-curve point must be refused");
    }

    #[test]
    fn a_short_tag_never_verifies() {
        let (ki, _) = run(b"1", b"1");
        assert!(!ki.verify_confirmation(Role::Responder, &[]));
        assert!(!ki.verify_confirmation(Role::Responder, &[0u8; 31]));
    }

    /// The property the whole crate exists for. A passive eavesdropper holds
    /// both messages and nothing else. For every possible 4-digit code, the
    /// best it can do is run its own side of the exchange over that guess
    /// against the recorded message — and no guess, **including the right
    /// one**, reproduces the key, because the key also depends on a secret
    /// scalar that never crossed the wire.
    #[test]
    fn recorded_messages_do_not_let_anyone_test_codes_offline() {
        let code = b"7305";
        let (i, msg_i) = Pake::start(Role::Initiator, code, CTX);
        let (r, msg_r) = Pake::start(Role::Responder, code, CTX);
        let real = i.finish(&msg_r).unwrap().confirmation(Role::Initiator);
        let _ = r.finish(&msg_i).unwrap();

        for guess in 0..10_000u32 {
            let guess = format!("{guess:04}");
            let (fake, _) = Pake::start(Role::Responder, guess.as_bytes(), CTX);
            let key = fake.finish(&msg_i).unwrap();
            assert_ne!(
                key.confirmation(Role::Initiator),
                real,
                "code {guess} reproduced the session key from a recording"
            );
        }
    }

    #[test]
    fn typed_codes_normalise() {
        assert_eq!(normalise_code(" 12 34 "), b"1234");
        assert_eq!(normalise_code("ab-12.3"), b"AB123");
        assert_eq!(normalise_code("0042"), b"0042");
    }
}
