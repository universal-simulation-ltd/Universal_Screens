# M8 — Browser receiver ("the browser tab is the screen an app connects *to*")

**Status:** 🚧 Planning (design only — no code). This doc is the milestone plan
for the inverse of M7: instead of the browser being a *client* that dials a
native host, the browser tab becomes a **receiver** the apps connect *into*.
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

- **M8a — rendezvous spike.** A Durable Object on the portal Worker: `joinRoom(code,
  role)` over WebSocket; reserves a code, holds ≤2 sockets, pairs them, expires
  after N minutes / on disconnect. *Gate:* two browser tabs join `room:7Q4K` and
  exchange a hello. Pure web; no app changes.
- **M8b — receiver page + QR.** New `apps/web` view: mint a code, render it as text
  + branded QR (reuse the host's QR style), join the room, wait for a peer. Extend
  `serveScreensConnect()` with the `code`/`role` branch (worker.js:202) and the
  `unisimscreens://` scheme with `connect?code=…`.
- **M8c — control-only round-trip (relay).** "Use this as a remote-controlled
  screen": browser joins as host-surface, the Android app joins as a clicker/
  trackpad client, `Input` frames relay through the DO. Reuses the whole `Input`
  side of the protocol + the app's existing clicker/trackpad UI. **First end-to-end
  win, zero WebRTC, zero new capture.**
- **M8d — desktop → browser viewer (relay).** The desktop host learns to **dial the
  room** (outbound `tungstenite` WebSocket) instead of only LAN-listening, then runs
  the *existing* `serve()` over it. Browser renders with the M7 decode pipeline.
  This is "remote access to my desktop from any browser tab, across networks" — the
  highest-reuse video case, still over the relay.
- **M8e — WebRTC media upgrade (optional, big).** Swap the video path to WebRTC
  (DO signals SDP/ICE; add a TURN fallback — Cloudflare Calls). New WebRTC stacks in
  the Rust host (`webrtc-rs`) + Android/iOS. Relay stays the fallback. This is the
  M7g-equivalent latency/cost upgrade; do **not** block earlier increments on it.
- **M8f — phone → browser cast (NEW capability).** Phone becomes a *sender*:
  self-capture via Android **MediaProjection** / iOS **ReplayKit**, encode, and
  stream into the room (relay, then WebRTC). Phones are client-only today, so this
  is the most net-new app work — sequence it last.
- **M8g — marketing wiring.** On `/screens`, add "**Use this screen as a
  receiver**" alongside the existing "Direct from your browser" (M7h) CTA.

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

- `backoffice/opensource-portal/src/worker.js` — extend `serveScreensConnect()`
  with the `code`/`role` branch; add the **Durable Object** room class + binding in
  `wrangler.jsonc`. (M8a/M8b)
- `apps/web/` — receiver view (mint code, render QR, join room), reusing
  `src/decoder.js` / `src/renderer.js` / `src/input.js` / the WASM shim; add a
  `src/rendezvous.js` (DO WebSocket client) + the post-pair mode picker. (M8b–M8d)
- `crates/host` + `crates/host-windows` — outbound "dial the room" mode (reuse
  `serve()` over a `tungstenite` socket, like `crates/web-bridge` does in reverse).
  (M8d)
- `crates/*` (host) + `apps/android` / `apps/ios` — WebRTC media stack. (M8e)
- `apps/android` / `apps/ios` — self-capture sender (MediaProjection / ReplayKit).
  (M8f)
- `Docs_UNI_SIM` `/screens` page — "use this screen as a receiver" CTA. (M8g)
</content>
</invoke>
