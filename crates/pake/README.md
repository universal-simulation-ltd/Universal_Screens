# unisim-pake

Short-code pairing: turn a code a person reads off one screen and types into
another into a strong shared key, without giving an eavesdropper anything to test
codes against. Bytes in, bytes out. No socket, no async, no transport. It builds
for `wasm32-unknown-unknown`, with JavaScript bindings behind `--features wasm`.

Universal Screens pairs its devices with it (see
`crates/transport/src/handshake.rs`). Nothing in this crate is specific to Screens.

## Why a PAKE

`key = H(code)` looks fine and isn't. Anyone who records one exchange can try every
code offline against the recording: a 4-digit code takes milliseconds, and a
wrong-guess lockout never sees it happen. A PAKE (password-authenticated key
exchange) mixes the code into a fresh Diffie-Hellman exchange, which has two effects:

- a recording contains nothing to test a guess against, because the key also
  depends on secret scalars that never cross the wire;
- an active attacker gets **one guess per attempt**, made against a real device
  that can count it.

## Using it

```rust
use unisim_pake::{normalise_code, Pake, Role};

let code = normalise_code("48 21");
let (me, msg) = Pake::start(Role::Initiator, &code, b"my-app/pairing/v1");
send(&msg);                                  // 33 bytes
let key = me.finish(&receive())?;            // a wrong code still finishes!
send(&key.confirmation(Role::Initiator));    // prove it, initiator first
if !key.verify_confirmation(Role::Responder, &receive()) { /* wrong code */ }
let aead_key = key.derive(b"my-app/aead");
```

⚠️ **`finish` succeeds even when the codes differ.** It then gives a different key.
Before trusting the key, prove that both sides hold it. Either key an authenticated
protocol with `derive` (Screens keys a Noise handshake with it), or exchange the
`confirmation` tags. To keep "one guess per attempt" true, the side that **shows**
the code must see the other side's proof first. It should send nothing keyed by the
result until then, and it should count a bad proof as a wrong guess.

In a browser (`--features wasm`, built with `wasm-pack build --target web -- --features wasm`):

```js
const me = new PakeSession(true, normaliseCode("4821"), ctx);
send(me.message);
const key = me.finish(await receive());
send(key.confirmation(true));
if (!key.verify(false, await receive())) throw new Error("wrong code");
```

## Why SPAKE2, and this crate (2026-10-04)

| | `spake2` 0.4 (RustCrypto) — **chosen** | CPace crates (`pake-cpace`, `cpace`, `pakery-cpace`) |
|---|---|---|
| Maintainer | RustCrypto organisation (many maintainers, same home as `sha2`/`hkdf`) | single individuals |
| Third-party audit | **none** of the crate itself (its README says so); built on `curve25519-dalek`, which was audited (Quarkslab, 2019) | none |
| Spec | SPAKE2, compatible with python-spake2 / magic-wormhole (predates RFC 9382) | various drafts of draft-irtf-cfrg-cpace, still a moving target |
| In production | magic-wormhole.rs, which pairs over exactly this kind of short code | little |
| wasm32 | yes, with `getrandom`'s `js` feature | yes |

CPace is the CFRG's recommended balanced PAKE, and it avoids SPAKE2's fixed
blinding points M and N. SPAKE2's security rests on nobody knowing their discrete
logarithms; here they are hash-derived, "nothing up my sleeve" constants. On its own
merits CPace would be the better choice. It lost because no Rust implementation has
both a stable spec and an organisation behind it. `pakery-*` (0.6) tracks the
current drafts and RFC 9382, but it is one person's months-old project. `spake2`
0.5 is a pre-release.

**No PAKE crate is audited.** If one ever is (or `pakery` matures), switching is
contained: `Pake` and `SharedKey` are this crate's own types, and the wire carries
only their messages. Changing the algorithm means bumping the crate's `DOMAIN` tag and the Screens
handshake version, so old and new builds fail to pair instead of pairing wrongly.

## Reusing it elsewhere in the suite (not built)

- **Family QR joining.** If a QR carries a bearer invite token, anyone who
  photographs it can join. With this crate, the QR carries a public room ID plus
  a short code, and the joining phone and the inviting phone run `PakeSession` over
  any relay, such as a Supabase Realtime channel or a Durable Object room. Use the
  confirmation tags, the inviter as `Responder`, and the inviter counting failed
  confirmations. A relay, or a photo of the room ID alone, then learns nothing it
  can brute-force. The invite token goes across *encrypted* under
  `key.derive(b"family/invite")`.
- **A Jukebox remote.** The desktop Jukebox shows a 4-digit code. The phone types
  it, both run the exchange through the existing relay, and every later command
  is sealed with a key derived from the result. To remember the phone, keep
  `derive(b"jukebox/device")` on both ends. For a public-key identity like
  Screens', reuse `extender-transport`'s `PairingStore` pattern.
- **How to ship it to those apps:** build `--features wasm` with wasm-pack and
  publish it as an npm package (for example `@unisim/pake`), or vendor the
  `pkg/`. ⚠️ The crate inherits this repository's AGPL-3.0 licence. A suite app
  that isn't AGPL needs the crate relicensed (for example MIT OR Apache-2.0, like
  its `spake2` dependency) by the copyright holder first.
