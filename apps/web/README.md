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
- **M7c+ — decode / render / input / UI** — pending (the real TS client).

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
