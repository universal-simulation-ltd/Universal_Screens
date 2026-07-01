# M8e — WebRTC media upgrade (take video off the cloud relay)

**Status:** M8e-a (TURN endpoint) + M8e-b (data-channel spike) ✅ — M8e-c/d/e 🚧 (host/app WebRTC, hardware). The latency/cost upgrade flagged in
[M8-browser-receiver.md](M8-browser-receiver.md). **Prereq:** M8a (rendezvous DO)
✅, M8c (control round-trip + the JSON signal channel) ✅, M8d (desktop→browser
relay transport) ✅, M7 (WebCodecs decode + WASM shim) ✅.

> **One-line:** keep the rendezvous Durable Object as **signaling only**, and move
> the actual media to a **WebRTC peer connection** so frames go P2P / LAN-direct
> instead of transiting Cloudflare. The relay (M8c/M8d) stays as the fallback.

## Why

M8c/M8d carry **all** media through the DO relay. That's fine for control (a few
`Input` bytes/sec) and acceptable for modest mirror bitrates, but for full video
it means: every frame pays a Cloudflare hop (latency), the DO bills wall-time +
egress (cost), and there's no congestion control. WebRTC fixes all three — it's a
hardware media path with built-in congestion control, and once connected the
media never touches our infrastructure (STUN) or only touches a thin TURN relay
(symmetric NAT).

## The design

### Signaling: reuse the rendezvous DO (no new service)

The DO already pairs two peers by code and relays JSON frames (M8c). WebRTC
signaling is just three more relayed message types, namespaced under `t` like the
control protocol so they ride the same channel:

```
{ t:"sdp", kind:"offer"|"answer", sdp:"<sdp>" }
{ t:"ice", cand:"<candidate>", mid:"<sdpMid>", idx:<sdpMLineIndex> }
{ t:"webrtc-fail" }     // give up → peer falls back to the relay
```

Offer/answer roles: the **media source** (desktop host in M8d, phone in M8f) is
the WebRTC **offerer**; it creates the offer on `paired` and sends it through the
room. The **browser receiver** answers. Browser ICE/answer is built-in
(`RTCPeerConnection`); the source uses its platform's WebRTC stack.

### Media path: data channel first, media track later

Two ways to carry H.264 over the peer connection:

| Option | Reuse | Cost | Verdict |
|---|---|---|---|
| **Data channel carrying the existing `postcard` `Frame`/`StreamStart`** | **Maximum** — the browser's M7 WebCodecs pipeline + the host's encode/`serve()` are unchanged; only the *transport under them* changes from WS-relay to an RTCDataChannel | We own jitter/flow (use an unreliable/unordered channel for frames, reliable for control) | **v1 (M8e-c)** |
| **Media track (RTP H.264)** | Browser gets a hardware `<video>` decode + jitter buffer for free | Host must RTP-packetize H.264 (the protocol's `append_annex_b`/`annex_b_parameter_sets` help); renegotiation surface | **Later optimization (M8e-d)** |

**Decision: data channel first.** The whole point of M8e is the *P2P transport*,
not RTP — and the data-channel path drops straight under the M7 decode pipeline
that already works (`apps/web` `decoder.js`). The media-track path is a later
quality/latency refinement once the P2P transport is proven.

### NAT traversal: STUN + a TURN fallback

- **STUN** (public Google/Cloudflare STUN) resolves most cases → true P2P, zero
  infra in the media path.
- **TURN** is needed for symmetric NAT. Use **Cloudflare Calls TURN** (same
  account/edge as everything else) or self-host coturn. Credentials are short-lived
  and must be minted server-side — a small Worker route on the portal:
  `GET /screens/turn` → `{ iceServers:[…] }` (TURN with time-limited HMAC creds).
- **Fallback:** if no candidate pair connects within a timeout, send
  `{t:"webrtc-fail"}` and both peers fall back to the M8c/M8d **WS relay** (which
  already works). So M8e is strictly an *upgrade* — it can never regress below the
  relay.

## Architecture

```
   browser receiver                 portal Worker (Cloudflare)        media source (host / phone)
 ┌────────────────────┐   signaling  ┌──────────────────────┐  signaling ┌────────────────────────┐
 │ RTCPeerConnection  │◄═ sdp/ice ══►│ Durable Object room   │◄═ sdp/ice ═►│ webrtc-rs (host) / app  │
 │  • RTCDataChannel  │              │  (relays JSON only)    │            │  • H.264 encode (as is) │
 │  • M7 WebCodecs ◄──┼─ H.264 frames ── DIRECT P2P (STUN) or thin TURN ──┼─ postcard Frame/Start   │
 │    decode→canvas   │              │  GET /screens/turn → creds          │  • serve() over the chan │
 └────────────────────┘              └──────────────────────┘            └────────────────────────┘
                         media bypasses the cloud; only signaling + (maybe) TURN touch our infra
```

## Sub-increments

- **M8e-a — TURN/STUN + creds.** ✅ **Done** (`opensource-portal` PR #10).
  `GET /screens/turn` returns ICE servers — always public STUN, + short-lived
  Cloudflare Realtime TURN creds when `TURN_KEY_ID`/`TURN_API_TOKEN` secrets are set.
  STUN path verified via `wrangler dev`; the TURN request shape needs a real key to
  confirm end-to-end.
- **M8e-b — browser↔browser data channel over the DO.** ✅ **Done** (same PR).
  `public/screens/webrtc-spike.html`: two tabs pair over the room, exchange SDP/ICE
  *through the room* (namespaced `t:sdp`/`t:ice`), open a **P2P `RTCDataChannel`**.
  Serving + wiring verified via `wrangler dev`; the **actual P2P connection is
  browser-verified** (two tabs) — `RTCPeerConnection` isn't in Node.
- **M8e-c — host data-channel sender.** `webrtc-rs` in the host: on `paired`, offer
  via the room, open a data channel, run the existing `serve()` over it. Relay
  fallback on failure. Desktop→browser video, P2P. *Highest-value increment.*
- **M8e-d — media-track path (optional).** RTP H.264 track instead of the data
  channel, for the browser's free hardware decode + jitter buffer.
- **M8e-e — phone WebRTC.** Android/iOS WebRTC (libwebrtc) for the phone source
  (pairs with M8f); relay fallback meanwhile.

M8e-b is the verifiable gate; M8e-c is the headline (video off the relay).

## Open questions / risks

1. **`webrtc-rs` maturity / build weight.** It's a large dep; validate H.264 over a
   data channel + ICE on macOS *and* Windows hosts before committing M8e-c.
2. **Data channel framing.** One `postcard` body per channel message (like the WS
   path), unreliable+unordered for `Frame`, reliable+ordered for control/`Input`.
   Two channels, or one with per-message reliability? Decide in M8e-b.
3. **TURN cost.** TURN-relayed sessions still use bandwidth we pay for; most
   sessions should be STUN-direct. Measure the TURN hit rate.
4. **Congestion control on the data-channel path.** Media tracks get GCC for free;
   a data channel needs us to drop/skip frames under loss. The media-track path
   (M8e-d) is the real fix; M8e-c should at least drop stale frames.
5. **Security.** WebRTC media is DTLS-SRTP (E2E between peers) — *better* privacy
   than the relay (which sees plaintext-to-Cloudflare). Note this as a reason to
   prefer WebRTC for screen contents; keep the 4-digit PIN as the pairing gate.

## Surface (planned)

- `backoffice/opensource-portal` — `GET /screens/turn` Worker route (ICE creds);
  the DO already relays JSON, so signaling needs no DO change. (M8e-a)
- `apps/web` — `RTCPeerConnection` + data-channel transport behind the existing
  `RoomTransport` seam; capability-detect + relay fallback. (M8e-b/c)
- `crates/host*` (+ maybe `crates/web-bridge`) — `webrtc-rs` offerer + data-channel
  `serve()` transport. (M8e-c/d)
- `apps/android` / `apps/ios` — WebRTC stack for the phone source. (M8e-e, with M8f)
