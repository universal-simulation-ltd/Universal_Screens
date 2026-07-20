# M10 — Transport encryption (Noise over the LAN TCP protocol)

**Status:** shipped for the native TCP path (client ↔ Windows/macOS host, incl. the
Android/iOS FFI clients). Verified building + unit-tested on Windows. macOS host and
the mobile shells compile the same code but need their platforms to build/run.

## The gap this closes

The LAN protocol (`crates/protocol`) is length-prefixed `postcard` frames over
**plaintext TCP**, gated by a 4-digit pairing PIN. Historically "the PIN is a gate,
not encryption": anyone on the same network could passively read the mirror video
and the injected keystrokes/text, or tamper on-path. Deskreen (a comparator) is
end-to-end encrypted; this milestone brings the native path to parity for
confidentiality + MITM resistance.

## Design

A new crate, **`crates/transport`** (`extender-transport`), wraps the TCP stream in
a **Noise** tunnel using the [`snow`](https://crates.io/crates/snow) crate:

- **Pattern:** `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s`.
  - `NN` = ephemeral-ephemeral: no static keys to distribute, and **forward secrecy**
    (a passive capture stays unreadable even if the PIN leaks later).
  - `psk0` = the **pairing PIN is folded in as the pre-shared key** (`derive_psk` =
    `SHA-256(domain ‖ pin)`), so an active on-path attacker can't complete or relay
    the handshake without knowing the PIN. This is what turns the PIN into
    *encryption*, not just a gate.
- **`Conn`** is an enum (`Plain(TcpStream)` | `Secure(SecureStream)`) implementing
  `Read + Write`, so `protocol::{read_framed, write_framed}` run over it **unchanged**.
  It mirrors the `TcpStream` surface the callers use (`try_clone`, `shutdown`,
  `set_nodelay`).
- **`SecureStream`** transparently splits the byte stream into Noise transport
  messages (each ≤ 64 KiB, the Noise limit), carried as `u16`-length-prefixed
  ciphertext. The read + write halves share one `snow::TransportState` behind a
  mutex; each `Conn` clone drives a single direction (one reader thread, one writer
  thread — the pattern the client session and both hosts already use), so nonce
  order always matches wire order.

### Who encrypts

- **Native client** (`extender-core::Session::connect`, and therefore the desktop
  client, the Android JNI, and the iOS/Android FFI): **always** runs the Noise
  initiator handshake, keyed by `hello.pin`, before sending the `ClientHello`.
- **Host** (`serve_loop` on Windows + macOS): `transport::accept` peeks the first
  bytes. A `PREAMBLE` marker ⇒ run the Noise responder (keyed by the host's PIN) ⇒
  encrypted `Conn`. Anything else ⇒ a legacy/loopback plaintext peer ⇒ plaintext
  `Conn` (logged with a warning).
- **Browser bridge** (`crates/web-bridge`): unchanged. It forwards raw frames from a
  browser tab to the host over **loopback**, and a browser can't run Noise on its
  own, so that path stays plaintext and the host auto-detects it. The browser leg is
  therefore **not** yet end-to-end encrypted (it relies on `wss://` to the cloud
  rendezvous).

### What is deliberately unchanged

- The existing **plaintext-`ClientHello` PIN check** still runs inside the tunnel
  (belt and suspenders). This layer never removes or weakens the existing auth.
- The `postcard` **wire format** and `PROTOCOL_VERSION` (10): the message bytes are
  identical; only the transport wrapping is new (versioned by the `PREAMBLE`'s own
  version byte, `0x01`).

## Behavioural notes / edge cases

- **PIN mismatch** now fails at the handshake (AEAD tag failure) *and*, as before, at
  the in-tunnel PIN check. The outcome (reject) is unchanged; it just happens
  earlier.
- **PIN 0** ("no pairing") derives a fixed, well-known PSK: the channel is still
  encrypted against passive eavesdroppers, but carries no authentication — matching
  the existing "PIN 0 = accept anyone" semantics.
- One stricter-than-before case: a host with pairing **off** (PIN 0) that a client
  nonetheless connects to with a **non-zero** PIN now fails the handshake (PSKs
  differ), where it previously connected. This is safe (refuse, not accept-insecure)
  and effectively unreachable in the normal connect flows (a client only carries a
  PIN it got from a paired host's QR/URL).

## Verification

- `cargo test -p extender-transport` — unit tests cover: PSK determinism +
  PIN-sensitivity; the `PREAMBLE` can't collide with a plaintext hello length
  prefix; matching-PIN round-trip both ways; **wrong-PIN handshake failure**;
  PIN-0 still encrypts; a >64 KiB payload spanning multiple Noise messages
  (keyframe-sized); **ciphertext on the wire is not the cleartext**; and a plaintext
  peer passed through untouched.
- `cargo test -p extender-core` / `-p extender-mobile-ffi` — the client-session and
  FFI round-trip tests were updated so their fake hosts run the responder handshake.
- `cargo build -p extender-host-windows` / `-p extender-client`.

## Follow-ups

- **Require encryption from non-loopback peers** (reject remote plaintext) once every
  shipped client speaks Noise — currently plaintext is still accepted for
  compatibility + the browser bridge.
- **macOS host + mobile shells:** compile/run on their platforms (this box is
  Windows-only for those targets). The Rust is the shared `Conn` path the Windows
  host exercises.
- **Browser E2E:** the browser client still relies on `wss://` transport rather than
  the Noise tunnel; bringing it under encryption needs a browser-side Noise (WASM) or
  a different scheme.
