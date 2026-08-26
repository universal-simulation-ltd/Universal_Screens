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
- **Browser bridge** (`crates/web-bridge`): still forwards raw frames, so the
  browser leg is **not yet end-to-end encrypted** — it relies on `wss://` to the
  cloud rendezvous, which protects the wire but leaves the *relay* able to read
  the stream.
  ⚠️ **The reason given here for years — "a browser can't run Noise on its own" —
  is no longer true**, and was never quite the obstacle. The blocker was that
  this crate's handshake and record layer were welded to `TcpStream`, which a
  browser does not have. Both now live in
  [`transport::session`](../crates/transport/src/session.rs), which is byte-in /
  byte-out and **builds for `wasm32-unknown-unknown`**. What remains for the
  browser leg is the carrier work, not the crypto: WASM bindings, a dual-mode
  bridge, and the JS handshake — see Follow-ups.

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
- **Browser E2E — everything but the JavaScript is built (2026-08-26).**
  - ✅ `transport::session`: the handshake and record layer with no socket,
    compiling for `wasm32-unknown-unknown`.
  - ✅ `protocol-wasm::tunnel`: `Handshake` / `Tunnel` for JS, plus `frame` and
    `FrameReader` for the two jobs the bridge stops doing once encrypted (add the
    4-byte length prefix; re-assemble the downstream byte stream).
  - ✅ `web-bridge`: **both** relay paths — the LAN bridge (`proxy_established`)
    and the cloud one (`dial_room`) — decide per connection from the browser's
    first binary message. Preamble ⇒ relay verbatim; anything else ⇒ today's
    re-framing. Proven by `encrypted_passthrough.rs` against the shipped
    responder, and mutation-tested both ways (force either mode and one of the
    two paths breaks).
  - ❌ **The browser code**, and the question below, which has to be answered
    first.

  ⚠️ **The open problem is capability discovery, not cryptography.** The bridge
  ships **inside the host binary**, and the web client updates the instant it is
  deployed. A tab that simply started encrypting would send the preamble to every
  host already installed on someone's machine, whose older bridge would re-frame
  it into a garbage `ClientHello` — so the session dies rather than falling back.
  Three routes, none free:
  1. **WebSocket subprotocol** (`Sec-WebSocket-Protocol`). Clean and standard for
     the **LAN** bridge — an old bridge simply doesn't echo it, so `ws.protocol`
     is empty and the tab stays plaintext. ⚠️ It does **not** work for the room
     path: that negotiation is with the *Worker*, not with the host on the far
     side of it.
  2. **A capability signal in the room.** The host announces itself after pairing
     and an old host says nothing. Needs the portal Worker to relay an unknown
     text frame between peers untouched — unverified, and it lives in another
     repo.
  3. **Try encrypted, fall back on failure.** Works everywhere with no
     negotiation, at the cost of a reconnect against old hosts, and needs a
     failure signal that cannot be confused with a wrong PIN — which also
     manifests as the session closing.
  Route 1 for the LAN plus route 3 for the room is probably the answer, but it
  should be decided before any JS is written, not after.

  ⚠️ **A test that hangs is worse than a test that fails**, and this work found
  two of them by mutation-testing: breaking the record framing left both ends of
  the socket tests waiting forever, and forcing the relay mode wedged
  `dial_roundtrip`. Every socket in `transport` and `web-bridge`'s tests is now
  bounded by a read timeout. The same exercise also showed `dial_roundtrip` had
  the **host speaking first**, an order no real session takes.

  ⚠️ **`getrandom` needs its `js` feature** for any wasm build in this workspace,
  or `snow`'s key generation refuses to compile with an error that names neither
  Noise nor the browser. It is declared in `crates/transport/Cargo.toml` under a
  `cfg(target_arch = "wasm32")` target block, so it costs native builds nothing.
