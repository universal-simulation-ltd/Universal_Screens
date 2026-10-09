# Universal Screens — browser client

The install-free receiver: a browser tab that connects to an `extender-host`,
decodes its H.264 stream, and forwards keyboard/mouse/touch back. See the design
and milestone plan in [`docs/M7-browser-client.md`](../../docs/M7-browser-client.md).

## Status

- **M7a — transport** ✅ — `crates/web-bridge` fronts the native TCP host with a
  WebSocket. `spike.html` is the transport proof (handshake + downstream decode).
- **M7b — protocol WASM shim** ✅ — `crates/protocol-wasm` compiled with
  `wasm-pack`; the canonical Rust `postcard` codec in the browser (no TS drift).
  `verify-wasm.mjs` checks the built artifact against canonical Rust bytes.
- **M7c+ — decode / render / input / UI** ✅ — `src/client.js` and friends
  (plain JS modules, no build). Verified 2026-10-09 against a real
  `extender-host-windows` through the live relay: H.264 decoded and drawn.

## Where it is served

**`https://opensource.unisim.co.uk/screens/app/`** (since 2026-10-09). The
portal (`backoffice/opensource-portal`) is a static-assets Worker with no build
step, so it holds a BUILT copy in `public/screens/app/`, made by:

```sh
scripts/publish-web-client.sh   # wasm-pack --release, verify-wasm, copy, BUILD.txt
```

then commit + push the portal (its workflow deploys on push). `BUILD.txt` there
names the Screens commit the copy came from. ⚠️ Nothing republishes it
automatically: change `apps/web` or `crates/protocol-wasm`, and the hosted copy
is stale until someone runs the script. (A CI job would need a token that can
push to the portal repo; none exists.)

On that https copy only **Remote (across networks)** is offered (`HOSTED` in
`src/client.js`): the LAN path dials a bridge with `ws://`, which a browser
refuses from an https page (and the portal's CSP upgrades it to `wss://`), no
released host runs the bridge, and polling `http://<bridge>/peers` from a public
page would trip Chrome's local-network-access prompt. `node serve.mjs` on
`localhost` still shows everything.

The host's **Remote access (other networks)** panel (host-ui `remote.rs`, all
three hosts) sends people straight here, and its **Copy link** button gives
`/screens/app/?remote=CODE`. The portal's `/screens#remote` section and
`/screens/connect` also hand a 6-character code to this page as `?remote=CODE`.

## One-time toolchain

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

## Build the WASM shim

From the repo root:

```sh
# --dev skips the wasm-opt download; use --release for a shipped build.
wasm-pack build crates/protocol-wasm --dev --target web \
  --out-dir ../../apps/web/pkg --out-name extender_protocol
```

This regenerates `apps/web/pkg/` (git-ignored): `extender_protocol.js` +
`extender_protocol_bg.wasm` + `.d.ts`, importable from the browser client.

## Verify the shim (Node)

```sh
node apps/web/verify-wasm.mjs   # loads pkg/ and asserts against canonical bytes
```

## Verify the room's encryption negotiation (Node, no server)

```sh
node apps/web/room-caps.test.mjs   # stubs WebSocket; needs no Worker and no WASM
```

Covers the browser half of the room negotiation: a host that announces
`{"type":"caps","e2ee":true}` gets an encrypted tunnel, one that announces
nothing still pairs and runs plaintext, and the tab reports which it got.

⚠️ The second case is why this exists — **you cannot ask a real host to be an old
one**, so the compatibility path has no other way to be tested.

## Verify the ENCRYPTED leg, end to end (Node)

```sh
node apps/web/secure.test.mjs   # real bridge + real host, driven by src/secure.js
```

Spawns `cargo run -p extender-web-bridge --example e2ee_testbed` (a real bridge
in front of the shipped `transport::accept`) and drives it with the browser's own
`SecureChannel` over a real WebSocket. It covers the negotiation, a round trip, a
200 KB payload that spans many Noise records, message boundaries in a burst, a
wrong PIN, and the plaintext fall-back.

⚠️ **Rebuild `pkg/` first** if `crates/protocol-wasm` changed — the test loads the
built artifact, not the Rust source, so a stale `pkg/` silently tests the old
bindings.

## Run the transport spike (manual, end-to-end)

1. Start a host on the target machine: `extender-host` (macOS) /
   `extender-host-windows` (Windows), listening on `:9000`.
2. Start the bridge (same machine as the host, or anywhere that can reach it):
   `cargo run -p extender-web-bridge` (WS `:9002` → host `127.0.0.1:9000`).
3. Open `spike.html`, set the bridge `host:port`, pick a mode, and **Connect**.
   The log shows the handshake, the decoded `Message` stream, and a WebCodecs
   config probe. (Render is M7c.)

> Mixed content: a browser blocks `ws://` from an `https://` page. Serve the
> client over plain `http://` on the LAN (or `file://` for `spike.html`). The
> packaging decision is tracked in M7f.
