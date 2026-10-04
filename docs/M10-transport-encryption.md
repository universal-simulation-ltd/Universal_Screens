# M10 — Transport encryption (Noise over the LAN TCP protocol)

**Status:** shipped for the native TCP path (client ↔ Windows/macOS host, incl. the
Android/iOS FFI clients). Verified building + unit-tested on Windows. macOS host and
the mobile shells compile the same code but need their platforms to build/run.

## v2 — pairing with SPAKE2, and remembered devices (2026-10-04)

> Everything below this section describes **v1**, which v0.3 and earlier spoke.
> Where the two differ, this section wins.

**The hole in v1.** The PSK was `SHA-256(domain ‖ PIN)`, and the first handshake
message is authenticated under it. So anyone who recorded **one** connection
could try all 10,000 PINs offline (`an_offline_brute_force_of_a_recorded_pairing_finds_nothing`
in `crates/transport` shows it finding the PIN in a v1 recording), and the
wrong-PIN lockout never saw it happen. They then held the PIN for later sessions.

**v2** (`crates/transport/src/handshake.rs`, `session.rs::ClientHandshake`,
`lib.rs::accept_with`):

- **Pairing** (`connect_pair`): SPAKE2 over the 4-digit code
  ([`crates/pake`](../crates/pake/README.md), RustCrypto `spake2`), then
  `Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s`, with the PSK derived (HKDF) from the
  SPAKE2 key and the PAKE transcript as the Noise prologue. A recording gives
  nothing to test codes against. An active guesser gets one guess per
  connection: the host sends nothing keyed by the code until the client's
  Noise message 1 (its proof) authenticates, and a failure there is a
  `PinRejected`, counted by `PinGuard`. A peer that takes the host's SPAKE2
  message and hangs up has tested nothing and is not counted.
- **Remembering**: inside `XX` the two swap long-term X25519 keys.
  `PairingStore` keeps this device's identity and the peers it trusts, in
  `host-pairing.txt` / `client-pairing.txt` (`0600`) in the per-user config
  folder. iOS uses the app's `Library/Application Support`. Android is told by
  the app (`nativeSetPairingDir(filesDir)`). The browser uses `localStorage`
  (`apps/web/src/pairing.js`). Nothing is remembered when the code was 0 (a
  host with pairing off).
- **Reconnecting** (`connect_known`): `Noise_XX_25519_ChaChaPoly_BLAKE2s` with the
  remembered keys and **no code**, so a paired device survives the host's PIN
  changing at every start. The client checks the host's key *before* sending
  message 3 (its own key). The host answers an unknown key with an encrypted
  `NOT_PAIRED` verdict; that is not a guess, and the client then pairs with its
  code. A v2 peer skips the hosts' in-tunnel `ClientHello` PIN check
  (`PeerAuth::settles_pin`).
- **Host UI**: *Paired devices* (beside *Use my own PIN*) lists them and has
  **Forget all paired devices** — the revocation. A locked host now tells a v2
  client how long to wait (`refuse_locked`) instead of a bare close.

**Wire versioning.** A v2 opening is `"USCR" 0x01 | 0xFF 0xFF | 0x02 | mode`. It
starts with the v1 preamble, so every existing bridge relays it verbatim, and its
"length" of 0xFFFF is over a v0.3 host's 4 KiB handshake bound. A v0.3 host
therefore refuses it **at once and cleanly**, without counting a guess. A new
preamble byte would have made a v0.3 host try to allocate a 1.4 GB "frame" and
then wait. A future v3 bumps the version byte, and a v2 host answers it with
`STATUS_UNSUPPORTED`.

**Compatibility — the call, made explicitly.**

- **v0.3 clients → new hosts: still accepted.** This is the same call as for
  plaintext peers: refusing them would strand every installed v0.3 app. The host
  logs a warning per v1 connection (`PeerAuth::LegacyPin`). **The residual risk:**
  a v1 connection recorded on the network still gives away the PIN, and with it
  the ability to pair. Refusing v1 (and plaintext) from non-loopback peers is the
  follow-up once no v0.3 client is left.
- **New native clients (desktop, Android, iOS) → v0.3 hosts: no fall-back.** They
  never send v1. Falling back when v2 is refused would hand a brute-forceable
  recording to anyone who can make v2 fail (a forged reset is enough). Against a
  v0.3 host they report `Failure::OlderHost` ("update the host").
- **Browser → v0.3 host: falls back only on the host's own word.** The page
  deploys instantly while bridges ship inside installed hosts, so stranding them
  is not an option. The tab offers `usscreens-e2ee.v2` and `.v1`. A current bridge
  answers v2; a v0.3 bridge answers v1, and only then does the tab run v1, saying
  so in its log. Rooms do the same with `{"type":"caps","e2ee":true,"handshake":2}`.
  **A target that has spoken v2 to this tab is pinned** (`pairing.markV2`): a later
  offer of v1 or plaintext from it is refused as a downgrade. **The residual
  risk:** an attacker able to rewrite the WebSocket upgrade (LAN `ws://`) or the
  relay could downgrade a tab's *first* contact with a host. A "Nearby" retarget
  through a new bridge to a v0.3 machine fails with "update that computer".
- **4ccf6f0's bounds are kept:** the 10 s `HANDSHAKE_TIMEOUT` covers the whole v2
  exchange (`begin_handshake` before `accept_with`), handshake messages are capped
  at 4 KiB, and frames at 64 MiB.

**Also fixed on the way:** the browser's LAN transport fired `onOpen`, and with it
sent the hello, before the tunnel's handshake had finished. `send()` dropped it,
so an encrypted LAN session sent no hello, and the host timed out. `onOpen` now
waits for the tunnel. (The room path already waited.)

**Tests.** `cargo test -p unisim-pake -p extender-transport -p extender-core -p
extender-protocol-wasm -p extender-web-bridge`:

- SPAKE2 success and failure, malformed messages, and context separation;
- a wrong code costing exactly one counted attempt, and an abandoned exchange
  costing none;
- reconnecting by key after a PIN change, a forgotten device, a stranger host,
  PIN 0, a v1 client accepted, a v0.3 host refusing fast, a locked host, and an
  unknown version;
- **the offline brute force**: all 10,000 codes against a recorded v2 pairing find
  nothing, while the same search on a v1 recording finds the PIN (the control);
- no v1 fall-back from the native client;
- SPAKE2 through the real bridge;
- **native ↔ WASM interop**: `apps/web/secure.test.mjs` pairs, reconnects by key,
  fails with a wrong PIN, and fails as an unpaired tab, using the wasm-pack
  artifact against the shipped `accept_with`. `apps/web/room-caps.test.mjs`
  covers the tab's choice of handshake and the downgrade refusal.

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

- **Require v2 from non-loopback peers** (reject remote plaintext *and* v1) once
  no v0.3 client is left — both are still accepted for compatibility (see the
  v2 section's "the call"). The same change lets the browser drop its v1 path.
- **macOS host + mobile shells:** compile/run on their platforms (this box is
  Windows-only for those targets). The Rust is the shared `Conn` path the Windows
  host exercises.
- **Browser E2E — ✅ DONE (2026-08-26).** A browser tab runs the same PIN-keyed
  Noise tunnel as every native client:
  - `transport::session` — the handshake and record layer with no socket, built
    for `wasm32-unknown-unknown`.
  - `protocol-wasm::tunnel` — `Handshake` / `Tunnel` for JS, plus `frame` and
    `FrameReader` for the two jobs the bridge stops doing once encrypted.
  - `apps/web/src/secure.js` — the browser plumbing, used by both the LAN
    (`transport.js`) and room (`room.js`) transports.
  - `web-bridge` — both relay paths pass an encrypted connection through
    untouched, chosen from the browser's first binary message.

  ⚠️ **Encryption is negotiated, never assumed, and the two paths negotiate
  differently — that is forced, not stylistic.** The bridge ships **inside the
  host binary** while the web client updates the instant it deploys, so a tab
  that simply started encrypting would break every already-installed host.
  - **LAN:** the tab offers the `usscreens-e2ee.v1` WebSocket subprotocol; a
    bridge that can relay verbatim echoes it. Settled in the handshake — no
    timeout, no reconnect. An older bridge doesn't know the token, doesn't echo,
    and the tab stays plaintext.
  - **Room:** the tab's socket terminates at **Cloudflare**, not at the host, so
    no header it sends can reach the other end. The host announces
    `{"type":"caps","e2ee":true}` on pairing instead; the room relays it
    verbatim and every older peer ignores an unknown `type`. A tab that hears
    nothing within `CAPS_WAIT_MS` (1.5 s) runs plaintext — a cost paid only
    against an out-of-date host.

  ⚠️ **The PIN has to be known at `connect()`, not at `sendHello()`.** The
  handshake starts when the socket opens, before any hello exists, and keying the
  tunnel with one PIN while announcing another fails as an unreadable handshake —
  so both transports now throw a clear error instead. The hello's PIN check is
  kept on top of the tunnel, unchanged.

  ⚠️ **The tab says which it got.** "Encrypted" is a promise about someone's
  screen, and it depends on a machine this page cannot see, so the session log
  states it either way rather than implying the good case.

  **Verified end to end with the real browser code in the chain:**
  `apps/web/secure.test.mjs` spawns a real bridge in front of the shipped
  `transport::accept` (`--example e2ee_testbed`) and drives it through
  `src/secure.js` and the wasm-pack artifact over a real WebSocket — the
  negotiation, a round trip, a 200 KB payload spanning many Noise records,
  message boundaries in a burst, a wrong PIN failing to open a tunnel, and the
  plaintext fall-back still working. Plus 46 Rust tests across the three crates.

  ⚠️ **A test that hangs is worse than a test that fails**, and mutation-testing
  found two of them here: breaking the record framing left both ends of the
  socket tests waiting forever, and forcing the relay mode wedged
  `dial_roundtrip`. Every socket in `transport` and `web-bridge`'s tests is now
  bounded by a read timeout. The same exercise showed `dial_roundtrip` had the
  **host speaking first**, an order no real session takes.

  ⚠️ **Two Node-on-Windows traps in the harness**, both of which turned a passing
  run into a broken one: killing the child and calling `process.exit()` in the
  same tick **aborts** Node inside libuv (every check printed PASS, then the
  process crashed — any CI would call that a failure), and `shell: true` makes
  the child a shell wrapping cargo, so `kill()` reaches the shell and **leaves
  the testbed running** (verified: orphaned processes still holding ports).

  ⚠️ **`getrandom` needs its `js` feature** for any wasm build in this workspace,
  or `snow`'s key generation refuses to compile with an error that names neither
  Noise nor the browser. It is declared in `crates/transport/Cargo.toml` under a
  `cfg(target_arch = "wasm32")` target block, so it costs native builds nothing.
