# M7 — Browser client ("use the second machine from a browser tab")

**Status:** M7a (transport) ✅ + M7b (WASM shim) ✅ + M7c (decode + render) ✅ +
M7d (input) ✅ + M7e (connect UX) 🚧 — the transport, the canonical codec in the
browser, the WebCodecs decode→canvas pipeline (proven via a host-independent
self-test in a real browser), full input (mouse/keyboard/touch/gestures/
pointer-lock/IME), and a mobile-parity connect+session UI are all built. **What
remains before a live demo: run it against a real host** (M7f folds the bridge
into the host + adds the macOS listener). Code lives in `crates/web-bridge`,
`crates/protocol-wasm`, `apps/web/`. This doc is the milestone plan and the
transport decision the backlog asked for.
**Prereq:** M1 (streaming) ✅, M2 (input) ✅, M3 (virtual display) ✅,
M5 (shared `crates/core` session + mobile protocol additions) ✅,
M6 (clicker / `ControlOnly`) ✅.

> **Committed:** M7a–M7e landed in `feat/phone-present-upstream` (commit
> `7068d00`, pushed). Built WASM artifact (`apps/web/pkg`) is git-ignored.

## Pick up next (open question to answer)

**Q (James): can we avoid typing the host in manually — like the QR code does for
the mobile app?** Yes, and the path already exists in the backlog: have the host's
connect QR encode a URL to the web client carrying the host (+ Wi-Fi + PIN), e.g.
`https://unisim.co.uk/screens/connect?host=192.168.0.2:9002&pin=1234` (or a LAN
`http://…`). Scanning it with the **phone camera** opens the browser client with
the fields **pre-filled** (and it can auto-connect) — the browser parallel of the
mobile app's QR pairing, and the same dual-purpose QR the backlog already wants
(camera → website, in-app scan → direct connect). Open sub-questions to resolve
next session: (a) which origin serves the client (host-bundled `http://` vs.
`unisim.co.uk/screens`, tied to the mixed-content decision in M7f/Open-Q1);
(b) whether the host advertises a `ws://host:9002` URL in its QR alongside the
existing `ip:9000`; (c) LAN discovery (mDNS/Bonjour) as a no-QR fallback so the
client can *list* hosts instead of taking a typed address. See M7e's deep-link
item and Open question 6.

## Goal

Port the native receiver **client** into the browser so someone can drive a
second machine from a browser tab with **no install** on the *client* side. The
host still runs the native `extender-host` (macOS) / `extender-host-windows`
(Windows) app on the machine being controlled — "no install" is about the device
you're sitting at, not the one you're reaching.

Concretely the tab must:

1. Connect to a host, decode the live H.264 stream, render it to a `<canvas>`.
2. Forward keyboard / mouse / touch input back, reusing the existing HID mapping
   (`crates/client` `key_to_hid`, mirrored in `HidKeys.kt` / `HidKeys.swift`).
3. Reach product parity with the native client for the **Mirror / control** and
   **Extend** modes (clicker `ControlOnly` is a nice-to-have, not the headline).

This is the "browser as a computer" item under **Screens App** in
`Docs_UNI_SIM/backlog-unisim.md`. The marketing wiring (`/screens` page:
"Direct from your browser" + "Download the apps") is **step 2** here and depends
on the sibling "/screens marketing page" task existing first.

## The hard constraint: browsers can't open raw TCP

The whole native stack speaks **plaintext TCP on :9000** with length-prefixed
`postcard` frames (`crates/protocol`). A browser tab cannot open a raw TCP
socket — it only gets WebSocket, WebTransport, WebRTC, and `fetch`. So the
transport is the spike, and everything else (decode, render, input) is
comparatively well-trodden browser API.

### Transport options considered

| Option | Latency | Browser support (2026) | Host work | Verdict |
|---|---|---|---|---|
| **WebSocket (binary)** | TCP-level (head-of-line blocking, but LAN RTT is sub-ms) | Universal (incl. Safari/iPadOS) | Add a WS listener that re-frames the *existing* protocol | **Chosen** |
| WebTransport (QUIC) | Best — unreliable datagrams, no HOL blocking | Chrome/Edge/Firefox; **no Safari** as of early 2026 | New QUIC server + cert story | Rejected: Safari gap kills "works on any tab", esp. iPad |
| WebRTC (datachannel + media) | Best, hardware media path | Universal | SDP/ICE/STUN signalling + a media or data pipeline — large | Rejected for v1: heavy negotiation, big surface, slow to ship |

**Decision: WebSocket, binary frames, carrying the existing `postcard`
protocol verbatim.** Rationale:

- **Reuses the entire wire protocol unchanged.** Every `Message` (downstream) and
  `Input` (upstream) already serializes to a `postcard` body. The WS path ships
  **one `postcard` body per binary WS message** — WS already delimits messages,
  so we drop the redundant 4-byte length prefix on this path (the host's
  `write_framed`/`read_framed` length prefix is only needed on the byte-stream
  TCP transport). No protocol version bump, no new message types for transport.
- **Universal reach.** The headline is "open a tab, anywhere" — that includes
  Safari on iPad/Mac, which rules out WebTransport for v1.
- **LAN latency is fine.** The product today is LAN-first (M5f off-LAN is its own
  track). On a LAN, TCP head-of-line blocking is a non-issue at sub-ms RTT; the
  dominant latency is encode + decode, identical to the native client.
- **Smallest host change.** A WS listener that hands the same `Message`/`Input`
  framing to the same `serve()` code, rather than a parallel media stack.

WebTransport/WebRTC stay open as a **later latency upgrade** (M7g, optional),
gated behind capability detection with the WebSocket path as the fallback.

## Decode: WebCodecs `VideoDecoder` (hardware H.264)

The browser decodes H.264 with the **WebCodecs** `VideoDecoder` — hardware
accelerated where available, the same win the native mobile apps get from
VideoToolbox / MediaCodec. We do **not** ship a software decoder (no `openh264`
in WASM): WebCodecs is supported in Chrome/Edge, Safari 16.4+, and Firefox 130+,
which covers the install-free target.

The host already emits **AVCC** frames (4-byte big-endian length-prefixed NALs)
and sends raw **SPS/PPS** NALs in `Message::StreamStart { parameter_sets }`. That
maps onto WebCodecs cleanly in **AVC (`avcC`) mode**:

- Build an `avcDecoderConfigurationRecord` (the `avcC` box) from the SPS/PPS in
  `StreamStart` and pass it as `VideoDecoder.configure({ codec: 'avc1.…',
  description })`. The `avc1.PPCCLL` codec string is derived from the SPS's
  `profile_idc` / `constraint_flags` / `level_idc` (first 3 bytes after the SPS
  NAL header) — e.g. `avc1.42e01f` for Constrained Baseline @ 3.1.
- Then feed each `Message::Frame.data` **straight through** as an
  `EncodedVideoChunk` (`type: keyframe ? 'key' : 'delta'`, `timestamp` from
  `pts_value`/`pts_timescale`). No Annex-B conversion needed — `avcC` mode
  consumes the AVCC bytes as-is. (`crates/protocol`'s `append_annex_b` /
  `annex_b_parameter_sets` are the Annex-B fallback if a browser only accepts
  that form; not expected.)

Render the `VideoFrame` callback output to a `<canvas>` via
`canvasCtx.drawImage(frame, …)` (or a `WebGL`/`WebGPU` texture path if profiling
demands it), letterboxed to preserve aspect ratio, then `frame.close()`.

## Input: reuse the HID map, add browser sources

Input is the cheap half — the protocol already supports everything the browser
produces:

- **Mouse:** `pointermove`/`mousemove` → `Input::MouseMove { x, y }` (normalized
  `[0,1]` to the rendered frame rect, exactly as the mobile apps do — the host
  maps normalized→display bounds). Buttons → `Input::MouseButton`. Wheel →
  `Input::Scroll`. **Pointer Lock API** gives `movementX/Y` for
  `Input::MouseMoveRelative` in a future "lock the cursor" control mode (parity
  with the desktop client's pointer-lock).
- **Keyboard:** `KeyboardEvent.code` (a physical-key identifier, e.g. `"KeyA"`,
  `"ArrowLeft"`, `"F5"`) maps **directly** onto the same USB-HID usage ids the
  native client uses in `key_to_hid` — the table is portable. Printable
  characters / IME commits go through `Input::Text` (the soft-keyboard path from
  M5a), so we don't force composed characters through the HID map.
  `preventDefault()` on captured keys so the tab doesn't act on them; document
  that some combos (⌘W, Ctrl-T, the browser's own shortcuts) **cannot** be
  intercepted from a tab — a known, documented limitation of the browser client
  vs. the native app.
- **Touch:** `Touch`/`Pointer` events → `Input::Touch { id, phase, x, y }` and
  pinch → `Input::Gesture(Gesture::Pinch)` / long-press →
  `Gesture::SecondaryClick`, mirroring the mobile gesture classification.

A shared **`hid.ts`** table (browser `KeyboardEvent.code` → HID usage id) is the
fourth copy of the same map (`crates/client` Rust, `HidKeys.kt`, `HidKeys.swift`).
Keep them in sync; consider generating all four from one source later.

## Where the protocol codec lives: WASM shim, TS glue

`crates/core`'s `Session` uses `std::net::TcpStream` + threads — neither exists
in browser WASM — so we **cannot** reuse `Session` directly. Two ways to get
`postcard` (de)serialization in the tab:

1. **Hand-port `postcard` framing to TypeScript.** Small surface (varint
   discriminants, LE ints, length-prefixed `Vec<u8>`/`String`), but a
   non-self-describing format the wire has already revised 10 times — easy to
   drift, painful to debug.
2. **Compile a thin `crates/protocol` `wasm-bindgen` shim to WASM.** ~100 lines:
   `decode_message(&[u8]) -> JsValue` and `encode_input`/`encode_hello(JsValue)
   -> Vec<u8>` over `serde-wasm-bindgen`, plus the `avcC`/NAL helpers. The
   canonical Rust types are the single source of truth — **zero wire drift** — and
   the bundle cost is tiny (`protocol` has no heavy deps).

**Decision: option 2 (WASM shim).** All browser glue — WebSocket, WebCodecs,
canvas, input capture, UI — stays idiomatic **TypeScript**; only the wire format
is WASM. This is *not* "winit/wgpu in the browser" (rejected for the same
reasons M5 rejected it for mobile: the platform's own decode/render/UI is far
smoother).

## Architecture

```
   browser tab (no install)                       host (native, installed)
 ┌───────────────────────────┐   WebSocket    ┌──────────────────────────────┐
 │ TS app                    │   (binary,     │ existing TCP :9000  ── native │
 │  • WebSocket client       │◄═ postcard ═══►│ NEW  WS  :9002  ── browser    │
 │  • protocol WASM shim     │    bodies)     │      └ loopback bridge ↔ :9000│
 │  • WebCodecs VideoDecoder │                │        reuses read_hello/serve│
 │  • canvas render          │   H.264 ◄──────│  ScreenCaptureKit/DXGI + HW   │
 │  • pointer/key/touch ─────┼── Input ──────►│  encode (unchanged)           │
 └───────────────────────────┘                │  inject (unchanged)           │
                                              └──────────────────────────────┘
```

The host gains a **second listener** (a distinct port, e.g. `:9002`, so detection
is trivial and the raw-TCP path is untouched).

**Transport-adapter reality (verified against the code, 2026-06-18).** The serve
path is *not* generic over `Read + Write` — `read_hello`, `serve`, `serve_clicker`,
`serve_mirror`, `stream::run`, and `snapshot_loop` all take a concrete
`std::net::TcpStream`, and they rely on `stream.try_clone()` to get a **second
independent handle** so one thread reads upstream `Input` frames while another
writes downstream `Message` frames concurrently. A WS connection is neither a
`TcpStream` nor independently clonable that way, so "implement `Read + Write` and
reuse `serve()` verbatim" is not free. Two ways to bridge:

- **(A) Loopback TCP bridge — recommended for the spike.** A WS server thread
  accepts the browser socket on `:9002` and, per connection, dials a fresh
  *loopback* TCP connection to the existing `:9000` serve path, then pumps bytes
  both ways with a tiny reframe: **WS→TCP** prepend the 4-byte little-endian length
  to each `postcard` body; **TCP→WS** read each length-prefixed frame and emit it
  as one binary WS message. `serve()`, `try_clone()`, capture, encode, and inject
  stay **literally untouched** — they just see a normal localhost `TcpStream`. All
  WS complexity is isolated in one bridge module. Cost: one extra loopback hop
  (negligible — localhost easily carries the LAN H.264 bitrate) and the peer logs
  as `127.0.0.1`.
- **(B) Generic-transport refactor.** Make the whole serve path generic over a
  `Transport` trait that can split into independent reader/writer halves (replacing
  `try_clone`). Cleaner long-term, no double hop, real peer address — but it
  rewrites a load-bearing concurrency primitive across **both** hosts. Higher risk,
  slower to the M7a gate.

**Decision: (A) for M7a** — it reaches the "browser decodes real frames" proof with
near-zero risk to the shipping native path. (B) stays open as a later cleanup if the
loopback hop ever matters (it won't on LAN). Either way the WS framing/reframe lives
in one place (`crates/protocol` or a small `crates/transport`) so both hosts share
it.

## Sub-increments

- **M7a — transport spike.** ✅ Done as the **loopback TCP bridge** (option A):
  `crates/web-bridge` (`tungstenite`) accepts a browser WS connection and pumps
  the *existing* `postcard` frames to/from a running `extender-host` (one body per
  WS binary message; the 4-byte length prefix is added on the TCP side only), so
  `serve()` and the whole capture/encode/inject path stay untouched. *Verified:*
  (1) an integration test pushes a real `ClientHello` from a WS client through the
  bridge to a fake host and a real `Message` back, asserting byte-identical
  framing; (2) `apps/web/spike.html` completes the handshake and decodes the
  downstream `Message` stream (a minimal hand-rolled JS `postcard` decoder,
  cross-checked in a browser against canonical Rust-encoded bytes — the WASM shim
  replaces it in M7b) and probes `VideoDecoder.isConfigSupported` for the
  SPS/PPS-derived `avc1` config to de-risk M7c. Render is still M7c.
- **M7b — protocol WASM shim.** ✅ `crates/protocol-wasm` (`wasm-bindgen`, built
  with `wasm-pack --target web` into `apps/web/pkg/`): `decode_message` (typed
  getters; byte buffers as `Uint8Array`), `encode_hello` / `encode_*` for every
  upstream `Input`, `protocol_version`, and the `avc_codec_string` /
  `avcc_description` WebCodecs-config helpers. *Verified:* native round-trip tests
  (`cargo test -p extender-protocol-wasm`) **and** the built WASM loaded in Node
  (`apps/web/verify-wasm.mjs`) reproduce/parse the canonical Rust `postcard` bytes
  exactly. This replaces the spike page's hand-rolled JS decoder for M7c onward.
- **M7c — decode + render.** ✅ The real client (`apps/web/`, ES modules over the
  WASM shim): `Transport` (WebSocket) → `H264Decoder` (WebCodecs `VideoDecoder`,
  configured from `StreamStart` via the WASM `avc_codec_string`/`avcc_description`
  helpers) → `CanvasRenderer` (letterboxed `drawImage`, with pointer→normalized
  coordinate mapping). *Verified in a real browser* (`index.html` on a static dev
  server, via the Preview): the WASM inits (`protocol v10`), and a host-independent
  **decode self-test** — `VideoEncoder` makes a real H.264 keyframe, the config is
  rebuilt from its SPS/PPS through the WASM helpers, a real `VideoDecoder` decodes
  it, and the resulting `VideoFrame` draws to the canvas ("decode OK", letterboxed,
  no console errors). *Still needs a live host* for full-rate streaming + a
  glass-to-glass latency measurement vs. the native client.
- **M7d — input.** ✅ `apps/web/src/input.js` (`InputController`): mouse
  (move/button/scroll, normalized coords) + **pointer-lock relative** mode;
  physical keyboard via `hid.js` (the 4th copy of the HID map; skips IME
  composition); **touch** → `Input::Touch` with deferred-begin so a tap is a
  click, a drag is a drag; **gestures** — two-finger **pinch** → `Gesture::Pinch`,
  **long-press** → `Gesture::SecondaryClick`; and **IME text** via a hidden field's
  `compositionend` → `Input::Text`. Input is gated per mode (view-only sends
  nothing; clicker sends keys only; control sends everything). *Remaining for a
  live pass:* confirm against a host, enumerate the captured-key list, and
  document un-interceptable browser shortcuts (⌘W/Ctrl-T).
- **M7e — connect UX + modes.** 🚧 Done: a mobile-parity connect screen
  (`index.html` + `client.js`) — app icon, "Universal Screens" wordmark, the same
  glyph/label/blurb mode rows as the iOS/Android picker (Remote control / Mirror
  screen / Clicker), `host:port` + PIN entry (the v9 `pin`), **saved connections**
  in `localStorage` (`saved.js`, labelled from `HostInfo`, most-recent-first,
  forgettable), a session screen with **fullscreen** + **lock-cursor** controls,
  and **reconnect** (disconnect returns to the picker; saved rows reconnect).
  Remaining: resolution picker and the `connect?wifi=…&pin=…` QR deep link (the
  sibling backlog QR item).
- **M7f — second host + packaging.** Port the WS listener to the macOS host;
  decide where the static client is served from (bundled by the host on a local
  HTTP port for true zero-config LAN use, vs. hosted at `unisim.co.uk/screens`).
- **M7g — latency upgrade (optional).** WebTransport/WebRTC datachannel behind
  capability detection, WebSocket as fallback. Its own spike; do not block v1.
- **M7h — marketing wiring (step 2, depends on `/screens` page).** On `/screens`,
  replace "get it on GitHub" with **"Direct from your browser"** (orange) +
  **"Download the apps"** (Windows / Mac / Linux / Android / iOS / GitHub
  placeholder links) on the same page. Coordinate with the sibling "/screens
  marketing page" task. Re-point the Opensource Portal "Geeky" Screens tile from
  the GitHub repo to `/screens` once live.

M7a is the gate. M7b–M7d are the real client. M7e–M7f make it usable. M7g/M7h are
follow-ons.

## Security / reachability

Same posture as M5f and unchanged by this milestone: **LAN-first, plaintext.**
A browser will refuse a `ws://` (insecure) connection from an `https://` page
(mixed content) — so either the client is served over plain `http://` on the LAN
(host-bundled), or the host must offer `wss://` with a cert (self-signed →
browser trust friction; the real fix is the M5f pairing/TLS track). **Flag for
the spike:** confirm the page-origin vs. `ws/wss` mixed-content rule against the
intended serving model before building UI — it can force the
host-bundled-`http://` packaging choice in M7f. The existing 4-digit PIN (`pin`
in `ClientHello`, v9) is the pairing primitive and carries over.

## Open questions / risks

1. **Mixed content (above).** `https://unisim.co.uk/screens` + `ws://lan-host`
   is blocked by browsers. Likely answer: host serves the client + WS over
   `http://`/`ws://` on the LAN (no external origin), with the marketing page
   linking/redirecting to it. Resolve in M7a/M7f.
2. **WebCodecs config from SPS/PPS.** Deriving the `avc1.PPCCLL` codec string and
   the `avcC` box from raw SPS/PPS NALs must be exact or `configure()` throws —
   prime target for the M7b round-trip tests.
3. **Latency under WebSocket.** Expected fine on LAN; measure in M7c before
   committing against the WebTransport upgrade.
4. **Un-interceptable browser shortcuts.** ⌘W/Ctrl-T/etc. can't be captured from
   a tab — a real capability gap vs. the native app. Document, don't fight it.
5. **Keep four HID maps in sync.** Adding `hid.js`/`hid.ts` is the 4th copy;
   consider a single generated source of truth.
6. **Avoid typing the host manually (the QR question).** Today the connect screen
   takes a typed `host:port`. The fix is a QR/deep-link flow — see
   [Pick up next](#pick-up-next-open-question-to-answer): the host's QR encodes a
   URL to the web client carrying host/Wi-Fi/PIN so a phone-camera scan opens it
   pre-filled, plus optional mDNS discovery as a no-QR fallback. **Answer/spec
   this next session.**

## Surface

- `crates/web-bridge/` (new) ✅ — standalone WS↔TCP loopback bridge
  (`tungstenite`) + round-trip and canonical-bytes tests. (M7a)
- `apps/web/spike.html` (new) ✅ — manual transport-proof page: handshake,
  downstream decode, WebCodecs config probe. (M7a)
- `crates/protocol-wasm/` (new) ✅ — `wasm-bindgen` shim over `crates/protocol`
  (`decode_message` / `encode_*` / `avcC` + codec-string helpers) with native +
  Node-against-WASM tests; built into `apps/web/pkg/` (git-ignored). (M7b)
- `crates/host-windows/src/`, `crates/host/src/main.rs` — fold the WS listener
  into each host process (retire the standalone bridge). (M7f)
- `apps/web/` ✅ — the browser client (ES modules over the WASM shim):
  `src/transport.js` (WebSocket), `src/decoder.js` (WebCodecs), `src/renderer.js`
  (canvas + coordinate mapping), `src/input.js` (mouse/keyboard/touch/gestures/
  pointer-lock/IME), `src/hid.js` (key map), `src/saved.js` (saved connections),
  `src/client.js` (connect + session orchestration + self-test), `index.html`
  (mobile-parity UI), plus `serve.mjs` (dev static server) and `verify-wasm.mjs`.
  (M7c–M7e)
- `Docs_UNI_SIM` `/screens` marketing page — browser-client CTA + download
  buttons (depends on the sibling page task). (M7h)
</content>
</invoke>
