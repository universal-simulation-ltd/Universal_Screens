# M8 — Browser receiver ("the browser tab is the screen an app connects *to*")

**Status:** M8a + M8b + M8c + M8d-transport + M8g ✅ — M8e/M8f 🚧 (hardware-gated). This doc is the milestone
plan for the inverse of M7: instead of the browser being a *client* that dials a
native host, the browser tab becomes a **receiver** the apps connect *into*. The
rendezvous gate (M8a) is built and verified; see the M8a sub-increment below.
**Prereq:** M7 (browser client: WebCodecs decode, WASM `postcard` shim,
`apps/web/`) ✅ for the render/codec half; M5 (shared `crates/core` session +
mobile protocol) ✅; M6 (`ControlOnly` / clicker) ✅. The hosts already capture +
encode (Windows/macOS) ✅.

> **Why this doc exists.** The question was: *"Can the Universal Screens website
> have a receiver page — you open it in a browser, it shows a QR / connection
> details, and one of the apps connects to the browser?"* Short answer: **yes,
> and it fits the existing QR/`/screens/connect` plumbing and the M7 codec work**
> — but it inverts the connection model and needs one piece of new infrastructure
> (a cloud rendezvous). This is the spec for that.

## Goal

Open `opensource.unisim.co.uk/screens` (or a dedicated `/screens/receive`) in any
browser — on a TV, a laptop, a projector-connected machine — and have the tab act
as a **receiver**: it shows a short code + QR, an app pairs with it, and then
**the user chooses what role the browser plays** (mirror target, remote-controlled
surface, second screen, …). The browser is the *screen*; the app is the *source /
remote*.

This is the symmetric twin of M7. M7 made a browser tab a **client that reaches
out to** a native host. M8 makes a browser tab a **receiver that an app reaches
into**. The user picks the direction per session (the "let the user choose"
requirement) — the receiver page presents the same mode rows the mobile app
already shows, and the choice decides which peer captures and which renders.

## The hard constraint: a browser tab cannot be a LAN server

Everything in the product today hangs off one fact: **the native host is a TCP
server on the LAN** (`:9000`, length-prefixed `postcard` frames, `crates/protocol`).
It listens; clients dial in; the QR just carries the host's `ip:port` + PIN so the
client knows where to dial. M7's browser *client* fits this fine — it is the side
that opens the connection (a `ws://` to the host, via `crates/web-bridge`).

A **receiver** is the *listening* side, and a browser tab **cannot listen**. It has
no inbound socket of any kind — only outbound `WebSocket`, `WebTransport`,
`WebRTC`, and `fetch`. So "an app connects to the browser" can **never** be a
direct LAN link the way "a phone connects to the host" is today. Both ends must
**dial out** to a shared meeting point in the cloud and be **matched by the code**
shown on the receiver page.

That meeting point — a code-keyed rendezvous — is the one genuinely new piece of
infrastructure M8 needs. It does not exist anywhere in the umbrella today (no
Durable Objects, no WebRTC, no signaling service). Everything else (codec, render,
input, protocol, QR trampoline) already exists and is reused.

So M8 has exactly two problems to solve:

1. **Rendezvous** — match two devices by a short code (and exchange a little
   setup data).
2. **Transport** — carry the media + input between the two matched peers.

## Problem 1 — Rendezvous (match two devices by a code)

The receiver page mints a short code (e.g. `7Q4K` or a 6-digit number), renders it
as text **and** as a QR, and joins a room named by that code. The app scans the QR
(or the user types the code) and joins the same room. The room pairs them and
relays the setup handshake.

### Options considered

| Option | Infra | Reach from native apps | Verdict |
|---|---|---|---|
| **Cloudflare Durable Object** (room keyed by code, on the portal Worker) | Same platform as the site; one new Worker binding | Any WebSocket client — browser, Android (OkHttp), Rust host (`tungstenite`, already a dep of `web-bridge`) | **Chosen** |
| Supabase Realtime broadcast (channel per code) | Already in the umbrella — *precedent: the Ergo mobile-signature handoff pairs two devices over `mobile-sig:{token}` in `UNI_SIM_Assess/.../sign-mobile/[token]/page.tsx`* | Needs a Supabase client in the **Rust host** and the Android app, neither of which uses Supabase today | Fallback / alternative |
| Worker + KV + polling | Trivial | Anything that can `fetch` | Too laggy for ICE; only ok for a one-shot SDP swap |

**Decision: a Cloudflare Durable Object room, keyed by the short code, on the
existing `opensource-portal` Worker** (`backoffice/opensource-portal/src/worker.js`,
which already owns `opensource.unisim.co.uk/*` and already serves
`/screens/connect`). Rationale:

- It lives **exactly where `/screens` already is** — no new vendor, no new domain.
- A WebSocket room is reachable by **everything that needs to join**: the browser
  (native `WebSocket`), the Android app (OkHttp/`okhttp3.WebSocket`), and the Rust
  host (`tungstenite`, already pulled in by `crates/web-bridge`). Adding Supabase to
  the Rust host + Android app would be a heavier dependency than one WebSocket URL.
- A DO is **stateful per code** — natural for "reserve this code, hold the two
  sockets, expire after N minutes" — and can double as the **fallback relay**
  (Problem 2) without a second service.

Supabase Realtime stays the documented fallback: it is proven in-repo for exactly
this device-pairing shape, so if standing up a DO proves fiddly we have a known-good
path for the browser↔web peers (the native side would still need a client).

### Code → QR → join, reusing the existing trampoline

The QR encodes the **same style of URL** the host QR already uses, pointed at a
receiver variant of the deep-link trampoline:

```
https://opensource.unisim.co.uk/screens/connect?code=7Q4K&role=receiver
```

`serveScreensConnect()` (worker.js:202) already parses query params and bounces
into the `unisimscreens://` app scheme with a download fallback — it grows a
`code`/`role` branch rather than a whole new page. Phone-camera scan → app opens
pre-filled with the code → app joins the DO room. (For typed entry the receiver
also shows the bare code.)

## Problem 2 — Transport (carry media + input between the matched peers)

Once paired, the two peers need a pipe. Two real options, same trade-off M7 already
reasoned about (and resolved in favour of "simple first, WebRTC as an upgrade"):

| Option | Latency | Reuse | Cost |
|---|---|---|---|
| **Cloud relay through the DO room** (forward `postcard` frames peer-to-peer) | + one cloud hop | **Maximum** — the *entire* existing wire protocol + M7's WASM decoder + Android transport ride it unchanged | All media transits Cloudflare (DO CPU + egress); fine for tiny input, heavier for video |
| **WebRTC** (DO room only signals; media goes P2P/LAN-direct) | Best — hardware media path, no cloud hop for media | Browser WebRTC is built-in; **new WebRTC stacks needed in the Rust host + Android/iOS apps** | Needs a STUN/TURN fallback (Cloudflare Calls TURN or coturn) for NAT |

**Decision: hybrid, phased — relay first, WebRTC as the latency/video upgrade.**
Exactly mirroring M7's "WebSocket now, WebTransport/WebRTC later (M7g)" call:

- **Control-only modes (clicker / trackpad / browser-as-remote)** carry trivial
  bandwidth (a few `Input` frames per second). They ride the **DO relay** directly —
  zero WebRTC, ships first, works through any NAT.
- **Live-video modes (mirror / second-screen / cast)** negotiate **WebRTC** for the
  media path via the same room, with the relay as a fallback. This is the Phase-2
  upgrade; it is where the new app-side WebRTC work lands.

Reusing the relay path means the *protocol is unchanged*: it already ships one
`postcard` body per WebSocket binary message on the M7 path; the DO just forwards
those bytes between the two sockets instead of a loopback bridge forwarding them to
a local TCP host.

## Role negotiation — the protocol is already direction-agnostic

The wire format does not care which physical device is which. One peer plays
**host** (emits the `Message` stream — `StreamStart` / `Frame` / `Snapshot` — and
consumes `Input`); the other plays **client** (renders, emits `Input`).
`ClientHello.capture_mode` (`CaptureMode::{VirtualDisplay, MirrorPrimary,
ControlOnly}` in `crates/protocol/src/lib.rs`) already encodes the intent. So
"the user chooses the role" is a **UI + a host/client flag**, not new protocol.

After pairing, the receiver page shows the **same mode rows as the iOS/Android
picker** (Remote control / Mirror / Clicker / Second screen). The choice maps to
who-is-host:

| User picks (on the receiver) | Browser role | App role | Reuse |
|---|---|---|---|
| **Mirror a desktop here** | client — renders the desktop (M7 decode) | desktop = host (existing capture) | **High** — desktop just dials the room instead of LAN-listening |
| **Cast my phone here** | client — renders the phone | phone = host (**NEW**: self-capture) | Phone capture is net-new (MediaProjection / ReplayKit) |
| **Use this as a remote-controlled screen** | host-surface — renders app/web content, consumes `Input` | phone = client (clicker/trackpad `Input`) | Control-only; tiny bandwidth; ships first |

Modes whose sender capability doesn't exist yet (phone self-capture) are shown
**disabled** until Phase 2, so the picker is honest about what's live.

## Architecture

```
   browser tab (receiver, no install)        Cloudflare              an app (sender / remote)
 ┌──────────────────────────────────┐   ┌──────────────────┐   ┌────────────────────────────┐
 │ TS app (extends apps/web)        │   │ portal Worker     │   │ desktop host  OR  phone     │
 │  • shows CODE + QR  ─────────────┼──►│  /screens/connect │◄──┤  • scan QR / type code      │
 │  • joins DO room by code         │◄═►│  Durable Object   │◄═►│  • join same DO room        │
 │  • protocol WASM shim (M7)       │   │   "room:7Q4K"     │   │  • capture + encode (host)  │
 │  • WebCodecs decode + canvas (M7)│   │   • pairs sockets │   │    OR send Input (remote)   │
 │  • or renders local content +    │   │   • relays frames │   └────────────────────────────┘
 │    consumes Input (remote mode)  │   │   • (signals WebRTC          ▲
 └──────────────────────────────────┘   │     for video, P2P) │        │  WebRTC media (Phase 2)
              ▲                          └──────────────────┘         │  bypasses the cloud hop
              └──────────────────────── direct P2P media ─────────────┘
```

The DO room is **both** the rendezvous (Problem 1) and the Phase-1 relay
(Problem 2). WebRTC (Phase 2) uses it only to signal, then media goes direct.

## Sub-increments

- **M8a — rendezvous spike.** ✅ **Done** (in the `opensource-portal` repo, not this
  one — that's where the site Worker lives). `RendezvousRoom` Durable Object
  (`src/rendezvous.js`): one instance per code (`idFromName`), ≤2 **hibernatable**
  WebSockets, verbatim relay, `waiting`/`paired`/`peer-left` control frames, 10-min
  TTL via an alarm. Routed by an exact-match `/screens/room` in `src/worker.js`
  (`RENDEZVOUS` binding + `v1 new_sqlite_classes` migration in `wrangler.jsonc`).
  *Gate met:* an automated two-client test against `wrangler dev` passes 9/9
  (pairing, role reporting, verbatim relay both ways, room-full rejection,
  peer-left, bad-code rejection); `deploy --dry-run` clean. Two-tab demo at
  `public/screens/room-spike.html`. **Not deployed** (live site untouched). Pure
  web; no app changes.
- **M8b — receiver page + QR.** ✅ **Done** (in `opensource-portal`, PR #7). Built as a
  **static page** `public/screens/receive.html` (not `apps/web` — the site is a
  static-assets Worker with no build step): mints a 4-char code, renders it big + as
  a **QR** (vendored MIT QR generator `public/screens/vendor/qrcode-generator.js`, so
  no build/CDN), joins the room as `role=receiver`, shows waiting → connected →
  peer-left. The QR encodes `/screens/connect?code=…&role=sender` so a phone camera
  lands in the app. **Routing gotcha learned:** `serveScreensConnect()` in the Worker
  is *dead* for `/screens/connect` — Cloudflare serves the matching static asset
  (`public/screens/connect.html`) **before** the Worker runs (assets-first default),
  so the `code`/`role` deep-link branch went into **`connect.html`**, not the Worker.
  Verified against `wrangler dev` (QR API correct, both connect branches serve,
  `/screens/receive` 200). No video yet — that's M8c. **Not deployed.**
- **M8c — control-only round-trip (relay).** ✅ **Done** (both halves, two repos).
  "Use this as a remote-controlled screen": the browser receiver renders a control
  surface (slide deck + cursor + clicks + blank) and the sender drives it. **First
  end-to-end win, zero WebRTC, zero new capture.**
  - *Wire format note:* we did **not** reuse the binary `postcard` `Input` enum here
    (that needs the WASM shim in the page + FFI/Rust on the phone — heavy for
    control-only and cross-repo). Instead a tiny **JSON control protocol** keyed by
    `t` (`move`/`click`/`btn`/`scroll`/`key`/`hello`), namespaced apart from the
    room's `type` signals. Full `postcard` reuse can come with the video path
    (M8d/M8e), where the WASM shim is needed anyway.
  - *Browser* (`opensource-portal` PR #8): `control.js` (pure `applyControl` reducer
    + 17 unit tests), `receive.html` control stage, `control-sender.html` (a
    browser sender — also a real control-from-another-browser tool). Verified via a
    live relay-through-the-DO round-trip.
  - *Android* (PR #23): `InputTarget` interface (`ExtenderSession` + new
    `RoomSession` implement it); `RoomSession` (OkHttp WS → control JSON);
    `CastFlow`/`CastModePicker`/`CastClickerScreen` **reusing the existing
    `TrackpadScreen`**; "Cast to a browser" entry + `?code=` deep-link/scan routing.
    `compileDebugKotlin` green; **needs an on-device pass + the Worker deployed.**
- **M8d — desktop → browser viewer (relay).** ✅ **Transport done** (PR #25). The
  desktop **dials the room** instead of LAN-listening, then runs the *existing*
  `serve()` over it; the browser renders with the M7 decode pipeline. "Remote access
  to my desktop from any browser tab, across networks" — the highest-reuse video
  case, over the relay.
  - *Rust* (`crates/web-bridge`): `dial_room()` = the web-bridge inside out —
    dials `wss://…/screens/room?code=…&role=sender`, waits for `paired`, bridges the
    room to a loopback connection to the local `serve()` (untouched). `native-tls`
    for `wss`; `signal_type()` JSON-less signal parse; `--room CODE` CLI mode.
  - *Browser* (`apps/web`): `RoomTransport` = the M7 `Transport` adapted to the room
    (text → signals, binary → decoder; decode injected, WASM-free).
  - *Verified:* `cargo test -p extender-web-bridge` (7 green, incl. a `dial_room`
    fake-room↔fake-host integration test) + a `RoomTransport` Node test against the
    real DO. **Live `wss` + host capture + real-stream decode need an on-hardware pass.**
  - *Host-GUI entry:* ✅ done (PR #28) — a "Cast to a browser screen" field in both
    host GUIs (`crates/host-macos` + `crates/host-windows`) spawns `dial_room` on a
    thread; macOS compiles, Windows mirrors it (reviewed-not-compiled here).
  - *Remaining wiring (follow-up):* **where the video viewer is served** — `apps/web`
    at `/screens` (M7f/M7h) vs bundling the WASM decode into the portal receiver
    page — plus an **on-hardware desktop→browser video pass**. The packaging decision
    M7f already flagged.
- **M8e — WebRTC media upgrade (optional, big).** Swap the video path to WebRTC
  (DO signals SDP/ICE; add a TURN fallback — Cloudflare Calls). New WebRTC stacks in
  the Rust host (`webrtc-rs`) + Android/iOS. Relay stays the fallback. This is the
  M7g-equivalent latency/cost upgrade; do **not** block earlier increments on it.
  **Full design: [M8e-webrtc-media.md](M8e-webrtc-media.md).**
- **M8f — phone → browser cast (NEW capability).** Phone becomes a *sender*:
  self-capture via Android **MediaProjection** / iOS **ReplayKit**, encode, and
  stream into the room (relay, then WebRTC). Phones are client-only today, so this
  is the most net-new app work — sequence it last.
  **Full design: [M8f-phone-capture.md](M8f-phone-capture.md).**
- **M8g — marketing wiring.** ✅ **Done** (`opensource-portal` PR #9). A hero CTA
  "**Use this screen as a receiver**" + a "make this screen a receiver" section on
  `/screens`, pointing at `/screens/receive`. Copy reflects the shipped control
  modes (M8c); not deployed.

M8a is the gate. M8c is the first usable win. M8d is the headline (browser as a
desktop viewer over any network). M8e/M8f are the heavy follow-ons.

## Security / reachability

This milestone **changes the reachability posture** vs. the LAN-first native path,
because traffic now traverses a cloud rendezvous:

- **The code is the pairing secret.** A short code is guessable; the DO must
  rate-limit join attempts, expire codes fast (minutes), and refuse a third joiner.
  Carry the existing **4-digit PIN** (`ClientHello.pin`, v9) *through* the room as a
  second factor so a guessed code alone can't connect.
- **No mixed-content problem (unlike M7).** Both peers dial **`wss://`** to the
  Cloudflare Worker from `https://` pages — no `http://`-LAN packaging gymnastics
  that M7's Open-Q1 wrestles with. This is actually *simpler* to serve than M7.
- **Relayed media transits Cloudflare** — call out the privacy/cost implication in
  the UI for video modes, and prefer the WebRTC (P2P) path once M8e lands so media
  leaves the cloud out of the loop. TURN-relayed WebRTC still touches a relay; STUN
  (direct) does not.
- **No E2E encryption beyond TLS-to-Cloudflare** in the relay path. If that's not
  acceptable for screen contents, gate video modes behind WebRTC-only (DTLS-SRTP,
  E2E between peers) and keep the relay for control-only.

## Open questions / risks

1. **DO vs. Supabase for rendezvous.** Chosen DO for native reach; confirm the
   Android + Rust WebSocket clients connect cleanly to a DO room before committing
   (M8a). Supabase Realtime is the fallback (precedent: Ergo `mobile-sig`).
2. **Relay cost/latency budget.** Measure DO relay throughput for a real mirror
   bitrate before promising video-over-relay; it may be control-only until M8e.
3. **NAT traversal for WebRTC (M8e).** STUN covers most; some networks need TURN
   (Cloudflare Calls or coturn) — a real running cost. Decide before M8e.
4. **Phone self-capture (M8f) is net-new.** MediaProjection/ReplayKit + an encoder
   on the phone is a milestone in itself; don't underestimate it.
5. **Code format + collision/expiry.** Short enough to type, long enough to resist
   guessing under rate-limiting; define the alphabet, length, and TTL in M8a.
6. **Which origin serves the receiver client.** Reuse `apps/web` served from
   `opensource.unisim.co.uk/screens` (clean here — no mixed content), vs. a separate
   `/screens/receive` route. Lean toward the existing app + a route.

## Surface (planned)

- `backoffice/opensource-portal/` — ✅ (M8a) `src/rendezvous.js` (`RendezvousRoom`
  DO), exact-match `/screens/room` route + class re-export in `src/worker.js`,
  `RENDEZVOUS` binding + `v1` migration in `wrangler.jsonc`, demo
  `public/screens/room-spike.html`. Still to do (M8b): extend `serveScreensConnect()`
  with the `code`/`role` branch.
- `opensource-portal/public/screens/` — ✅ (M8b) `receive.html` (mint code + QR +
  join room as `role=receiver`), `connect.html` (the `code`/`role` deep-link branch —
  the live lander, *not* the dead Worker `serveScreensConnect()`), vendored
  `vendor/qrcode-generator.js`. The post-pair **mode picker** and the actual
  **decode/render** (reusing `apps/web` `decoder.js`/`renderer.js`/WASM shim) arrive
  with the video path in M8c/M8d.
- `crates/host` + `crates/host-windows` — outbound "dial the room" mode (reuse
  `serve()` over a `tungstenite` socket, like `crates/web-bridge` does in reverse).
  (M8d)
- `crates/*` (host) + `apps/android` / `apps/ios` — WebRTC media stack. (M8e)
- `apps/android` / `apps/ios` — self-capture sender (MediaProjection / ReplayKit).
  (M8f)
- `Docs_UNI_SIM` `/screens` page — "use this screen as a receiver" CTA. (M8g)
</content>
</invoke>
