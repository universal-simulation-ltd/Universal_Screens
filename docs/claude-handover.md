# Claude session handover

Newest entry first. Each dated `## Update` overrides anything older that conflicts.
A `SessionStart` hook injects the top ~150 lines into new sessions, so keep the
newest entry at the top.

## Update — 2026-07-13 (LAN discovery: shared crate + Windows host parity with macOS)

Started the backlog's "discovery mode across all apps" item by making the LAN
peer-discovery that already existed **only in the macOS host** a shared,
cross-platform foundation and wiring it into the **Windows host**.

- **New crate `crates/discovery` (`extender-discovery`).** The UDP multicast
  beacon + listener + wire format (`USSCREENS\t{port}\t{name}` on `224.0.0.251:9001`,
  2s beacon, 6s TTL) lifted out of `host-macos/src/discovery.rs` into a pure-`std`,
  UI-agnostic crate. It reports peer-set changes through an `on_change: Fn()`
  callback instead of poking egui, so any host can wrap it. Beacon/parse unit tests
  moved with it (3 tests, green).
- **macOS host** — `host-macos/src/discovery.rs` is now a thin egui adapter that
  delegates to `extender-discovery`, bridging `on_change` → `ctx.request_repaint()`.
  `gui.rs` is byte-identical (same `crate::discovery::{DiscoveredPeer, start_listener,
  start_beacon}` API). **Not recompiled here (no macOS toolchain)** — needs a
  `cargo build -p extender-host-macos` on a Mac to confirm, but it's a mechanical
  delegation mirroring the Windows adapter, which does compile.
- **Windows host** — added the same adapter (`host-windows/src/discovery.rs`,
  `mod discovery;`) and wired it into `gui.rs` exactly like macOS: an always-on
  listener started in `new()`, the beacon started/stopped alongside serving in
  `start()`/`stop()`, an `on_exit` that stops both threads, and a **"Nearby"**
  section (device icon · `name · ip:port` · **Connect**) rendered above *More
  details*. Connect opens the deep-link `connect_url` via `ctx.open_url`.
  `cargo build -p extender-host-windows` green (only the pre-existing `scaled`
  warning in `stream.rs`); host + discovery tests pass (22 + 3).

**Left / next (rest of the backlog item):**
- **Recompile + verify the macOS host** on a Mac, then verify discovery end-to-end
  with a Windows host + macOS host on the same LAN (two-machine test).
- **Mobile + web discovery UI** — the phones/web client don't yet browse for hosts
  (Android could use NSD/`_usscreens._udp`, iOS Bonjour/`NWBrowser`, web can't do raw
  multicast so it'd need the host to expose a list). Plus the "orbit graphic"
  (current device centre, peers orbiting, like the portal) is still to design.
- **Cross-network ("remote across two networks")** — separate item; needs the cloud
  rendezvous/relay (the existing "cast to a browser" dial-the-room bridge is the
  natural basis).

## Update — 2026-07-10 (mobile connect-screen UX — reorder modes, hide cast, device-name saved rows)

Three related UX fixes on the phone clients (Android Compose + iOS SwiftUI, at
parity). No native/Rust/SDK change. Files: `apps/android/.../MainActivity.kt`
+ `ConnectionStore.kt`; `apps/ios/ScreenExtender/{ConnectView,ContentView,ConnectionStore}.swift`.

- **Mode picker reordered** (`ModePickerScreen`). A phone acting as the receiver is
  unlikely to be a *Second screen*, so the picker now leads with the likely modes —
  **Clicker · Trackpad · Mirror · Remote control** — and tucks **Second screen**
  behind a collapsed **"More options"** row (Android) / **"More options"** disclosure
  (iOS). Any future unlikely mode joins that group.
- **Cast-to-browser moved off the main screen.** The "Cast to a browser screen"
  button + the "Point at the host's…" helper copy are gone from the primary connect
  screen (people just scan the host QR). The cast affordance now lives under
  **Advanced** as an **inline "Web code" text box + button beneath it** (no popup —
  the old `AlertDialog` / `.alert` was removed). The button stays disabled until a
  4–8 char code is typed. The scan / deep-link cast path is unchanged.
- **Saved-host first line = the device name, never the IP.** Rows used to show the
  IP on both lines when no hostname was known. Top line is now the custom name →
  else the host machine name → else a friendly OS fallback (`deviceFallback(os)`:
  "Windows device" / "Apple device" / "Linux device" / "iOS device" / "Android
  device" / "Saved host"). The IP (plus remembered mode) sits on the second line.
- **Verified (Android):** `gradlew assembleDebug` BUILD SUCCESSFUL; installed +
  driven on the **Medium_Phone_API_36.1 emulator** (the physical device `9d084305`
  was not attached this session — `adb devices` empty — so the emulator stood in).
  Screenshots confirmed: clean main screen (no cast button/helper), mode picker with
  the 4 primary modes + collapsed "More options", the Advanced "Web code" box above
  the cast button, and two saved rows ("Kyjams-iMac" / "Windows device" fallback,
  IPs only on line 2). **iOS is reviewed-not-compiled** (no Mac/Xcode here) — the
  Swift mirrors the Android changes 1:1.
  - Re-check on the device: `adb -s 9d084305 install -r
    apps/android/app/build/outputs/apk/debug/app-debug.apk` then launch
    `com.universalsim.extender/.MainActivity`.

## Update — 2026-07-03 (iOS cast-to-browser — M8c parity with Android)

The iOS app can now scan/enter a receiver code and drive a browser `/screens/receive`
tab, closing the gap where only Android supported the M8c "cast to a browser" flow.
Before this, iOS `parseConnectPayload` required a `host` param and silently rejected
the receiver's `?code=&role=sender` QR.

- **New files** (`apps/ios/ScreenExtender/`):
  - `InputTarget.swift` — protocol (`sendMouseMoveRelative`/`sendMouseButton`/
    `sendScroll`/`tapKey`) mirroring Android's `InputTarget`; `ExtenderSession`
    conforms for free (already had those signatures).
  - `RoomSession.swift` — the cast session over `URLSessionWebSocketTask`. Dials
    `wss://opensource.unisim.co.uk/screens/room?code=…&role=sender`, handles
    `waiting`/`paired`/`peer-left` on the main queue, pings every 20s, and serialises
    input to the shared JSON control protocol (`hello`/`move`/`btn`/`scroll`/`key`,
    per `opensource-portal/public/screens/control.js`). Conforms to `InputTarget`.
  - `CastFlow.swift` — `CastController` (owns the RoomSession), `CastFlow`
    (waiting → mode picker → drive), `CastModePicker`, lightweight `CastClickerView`.
    Trackpad mode **reuses the existing `TrackpadView`**.
- **Edits:** `TrackpadView` now drives `InputTarget` (shared native-host + cast);
  `ContentView` gained `parseRoomCode` + a `castCode` branch + deep-link routing;
  `ConnectView` gained a "Cast to a browser screen" button + manual code-entry alert
  + room-code routing from the scanner. `project.pbxproj` regenerated (XcodeGen;
  `sources` globs the folder, so `xcodegen generate` picks up new files).
- **Verified:** `xcodebuild` simulator build green; app launches + renders the new
  entry point; `unisimscreens://…&role=sender` deep link routes in. The **exact wire
  contract** (URL + all four control-frame shapes) was checked against the **live
  deployed worker** — pairing + verbatim relay confirmed (bools/numbers preserved).
- **Gap (same as Android M8c shipped with):** an in-app *live* pair wasn't confirmed
  end-to-end — that needed a Simulator "Open in app?" tap that required computer-use
  approval unavailable in the autonomous run. Everything up to the wire protocol is
  verified. **Ships with the next app build (no `wrangler deploy`).** On-device pass
  pending.
- **Shipped:** Universal_Screens PR (branch `ios-cast-to-browser`). Suite changelog
  entry `2026.07.03.1`.

## Update — 2026-07-02 (receiver pairing QR — branded with the Universal QR studio style)

The `/screens/receive` pairing QR now matches the **Universal QR studio** look
instead of a plain black-and-white code, per the user's request ("use the QR
generator app for the QR style with the unisim logo icon in the middle").

- **Where:** `opensource-portal` repo (the site Worker that owns
  `opensource.unisim.co.uk/*`), **not** this repo. File:
  `public/screens/receive.html`.
- **What:** swapped the vendored `qrcode-generator.js` for the **`qr-code-styling`**
  engine, mirroring `Universal_QR`'s `src/lib/qr.ts` `DEFAULT_CONFIG`: rounded
  orange (`#fe8c01`) modules on black, extra-rounded finder squares, dot
  corner-dots, error-correction `H` + `hideBackgroundDots` (so the centre logo
  never breaks scanning), and the **UNI·SIM globe mark** centred at 28%. The `.qr`
  panel became the studio's black rounded frame; `renderQR()` keeps one instance
  and calls `.update()`, so **New code** re-renders in place (no stacked canvases).
- **Vendored** `qr-code-styling.js` (standalone UMD browser build, from
  `Universal_QR/node_modules`) + `unisim-icon.png` into `public/screens/vendor/`.
  Note the icon is the 1080×1080 / 245 KB PNG; the studio itself inlines a 256×256
  `UNISIM_MARK` data-URI — swap to that if page weight matters.
- **Verified** by serving `public/` statically + headless render: styled 256×256
  canvas draws with the logo centred, no console errors, `.update()` path clean.
- **Shipped:** merged to `main` as **opensource-portal PR #12** (squash `46d0cb1`).
  Suite changelog entry `2026.07.02.2` pushed. **Not yet deployed** — needs a
  `wrangler deploy` from `opensource-portal` to reach the live site.

## Update — 2026-06-30 (M8 browser receiver — built M8a–M8d + M8g; M8e/M8f specced)

Answered the question *"can the website have a receiver page — open it in a
browser, it shows a QR / code, and an app connects **to** the browser?"* Yes —
but it's the **inverse** of M7 and needs one new piece of infra. Wrote
`docs/M8-browser-receiver.md` (planning, **no code shipped**).

- **The crux:** a browser tab **cannot be a LAN server** (no inbound socket), so
  "an app connects to the browser" can't be a direct LAN link the way the host's
  `:9000` listener is. Both peers must **dial out to a cloud rendezvous** and be
  **matched by the code** the receiver page shows.
- **Decision (rendezvous):** a **Cloudflare Durable Object** room keyed by the
  short code, on the existing `opensource-portal` Worker (it already owns
  `opensource.unisim.co.uk/*` + `/screens/connect`). Reachable by browser, Android
  (OkHttp), and the Rust host (`tungstenite`, already a `web-bridge` dep).
  *Fallback:* Supabase Realtime broadcast (precedent — Ergo `mobile-sig:{token}`).
- **Decision (transport):** hybrid, phased — **DO relay first** (reuses the whole
  `postcard` protocol + M7 WASM decode unchanged), **WebRTC as the later video/
  latency upgrade** (mirrors M7's "WebSocket now, WebRTC = M7g"). Control-only
  modes ride the relay; live video negotiates WebRTC via the same room.
- **"User chooses the role":** protocol is already direction-agnostic
  (`ClientHello.capture_mode`), so the receiver shows the app's mode rows after
  pairing and the choice sets who-is-host. Phasing: **M8c** control-only relay
  (first win, no WebRTC/capture) → **M8d** desktop→browser viewer (host dials the
  room, highest reuse) → **M8e** WebRTC media → **M8f** phone self-capture
  (MediaProjection/ReplayKit — net-new, last).
- **M8a SHIPPED** (gate done). `RendezvousRoom` Durable Object in the
  **`opensource-portal` repo** (where the site Worker lives — *not* this repo):
  `src/rendezvous.js` (one DO per code, ≤2 hibernatable WebSockets, verbatim relay,
  10-min alarm TTL), `/screens/room` route + `RENDEZVOUS` binding + `v1` migration,
  two-tab demo `public/screens/room-spike.html`. Verified 9/9 against `wrangler dev`;
  `deploy --dry-run` clean. **Merged (opensource-portal PR #6), NOT deployed** — the
  live site is untouched until someone runs `wrangler deploy`.
- **M8b SHIPPED** (receiver page + QR). In `opensource-portal` (PR #7): static
  `public/screens/receive.html` (mints a 4-char code, renders it + a QR, joins the
  room as `role=receiver`, shows waiting→connected→peer-left), the `code`/`role`
  deep-link branch added to **`public/screens/connect.html`**, and a vendored MIT QR
  lib (`public/screens/vendor/qrcode-generator.js`, no build step). Verified against
  `wrangler dev`. **Not deployed.**
  - **Routing gotcha (write this down):** the Worker's `serveScreensConnect()` is
    *dead* for `/screens/connect` — Cloudflare serves the matching **static asset**
    (`connect.html`) before the Worker runs (assets-first default). So the live
    connect page is the static file; the Worker route never fires. That's why the
    code handling went into `connect.html`, and `src/worker.js` was untouched in M8b.
- **M8c SHIPPED** (control round-trip — first real end-to-end win). The browser
  receiver is now a *remote-controlled screen* and the phone drives it.
  - *Wire format:* a small **JSON control protocol** keyed by `t`
    (`move`/`click`/`btn`/`scroll`/`key`/`hello`), NOT the binary `postcard` `Input`
    enum (that needs the WASM shim + FFI — saved for the video path M8d/M8e).
  - *Browser* (`opensource-portal` PR #8): `control.js` (pure `applyControl` reducer
    + `control.test.mjs`, 17 cases), `receive.html` control stage, `control-sender.html`
    (browser sender / another-browser remote). Verified via a live relay-through-the-DO
    round-trip + reducer unit tests.
  - *Android* (PR #23): `InputTarget` interface (`ExtenderSession` + new `RoomSession`
    implement it); `RoomSession` = OkHttp WS → control JSON; `CastFlow` reuses the
    existing `TrackpadScreen` + a `CastClickerScreen`; "Cast to a browser" button +
    `?code=` deep-link/scan routing. **`compileDebugKotlin` green** (Gradle 8.7 / JBR
    21). Needs an **on-device pass** + the Worker **deployed** to confirm the live link.
    Build with `JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
    ANDROID_HOME=~/Library/Android/sdk ./gradlew :app:compileDebugKotlin`.
- **M8d SHIPPED** (transport) + **M8g SHIPPED** (marketing).
  - *M8d* (Universal_Screens PR #25): desktop → browser viewer over the cloud.
    `crates/web-bridge::dial_room()` (the bridge inside out — host dials
    `wss://…/screens/room?code=…&role=sender`, waits `paired`, bridges to local
    `serve()`; `native-tls` for wss; `--room CODE` CLI). `apps/web/src/room.js`
    `RoomTransport` (M7 Transport adapted to the room; decode injected, WASM-free).
    Verified: `cargo test -p extender-web-bridge` (7 green incl. a `dial_room`
    fake-room↔fake-host test) + a `RoomTransport` Node test vs the real DO. **Live
    wss + host capture + real-stream decode need an on-hardware pass.**
  - *M8g* (opensource-portal PR #9): "Use this screen as a receiver" hero CTA +
    section on `/screens` → `/screens/receive`.
- **Deploy state:** the portal Worker is **DEPLOYED + verified live** (2026-07-01,
  version `f9d36b67`) — `/screens/turn` (M8e), the M8g receiver CTA on `/screens`,
  and `/screens/{receive,webrtc-spike,control.js}` all confirmed 200/correct on
  `opensource.unisim.co.uk`. The `RendezvousRoom` DO went out in the user's earlier
  (2026-06-30) deploy; M8c cast control confirmed on-device then. M8d's `dial_room`
  is a **host/CLI binary** (not the site) — it ships with a host build, not a
  `wrangler deploy`. *(Pre-existing unpushed commits sit in the sibling
  `Docs_UNI_SIM` repo — untouched this session, not mine to ship.)*
- **M8d host-GUI entry SHIPPED** (PR #28): a "Cast to a browser screen" field in
  both host GUIs (`crates/host-macos` + `crates/host-windows`) spawns `dial_room` on
  a thread, bridging the host's listener to the room. `cargo build -p
  extender-host-macos` green; Windows mirrors it (reviewed-not-compiled here). Build
  the macOS host with `JAVA_HOME` not needed — just `cargo build -p
  extender-host-macos` (uses native-tls via Security.framework).
- **M8e + M8f DESIGN SPECS written** (PR #27): `docs/M8e-webrtc-media.md` (DO =
  signaling only; WebRTC data channel carrying postcard frames first, media track
  later; STUN + Cloudflare-Calls TURN; relay fallback) and `docs/M8f-phone-capture.md`
  (MediaProjection/ReplayKit → same StreamStart/Frame; reuses the M8d viewer; extend
  mobile-ffi with frame encoders). Both phased with a verifiable gate.
- **Next / remaining (hardware-gated, each its own session):**
  - **M8d finish:** decide where the video viewer is served (`apps/web` at `/screens`
    vs bundling the WASM decode into the portal receiver) + an on-hardware
    desktop→browser **video** pass (transport + host-GUI entry are in).
  - **M8e — WebRTC:** M8e-a (`GET /screens/turn` ICE endpoint) + M8e-b
    (`webrtc-spike.html` browser↔browser data channel over the DO) **SHIPPED**
    (opensource-portal PR #10; STUN + serving verified, P2P is browser-verified in
    two tabs). **`webrtc-spike.html` also got a 2-PC pairing UX** (PR #11, deployed
    version 922e7998): a 4-digit code minted on load + a scannable QR (vendored
    generator) encoding the code + the *opposite* role + `auto=1`, so the second PC
    scans/opens the link and auto-connects. (Browser can't read the Wi-Fi SSID, so
    no network details in the QR — WebRTC handles same/cross network via STUN/TURN.)
    Remaining: **M8e-c** (host `webrtc-rs` offerer + data-channel `serve()` →
    desktop→browser video P2P) + M8e-d (RTP media track) — need hardware.
  - **M8f — phone self-capture** (spec ready): start at M8f-a/b (Android
    MediaProjection + mobile-ffi frame encoders); browser viewer is free (M8d reuse).

## Update — 2026-06-28 (Trackpad click-and-drag)

Backlog item *"with the trackpad we need to be able to do a click and drag"*.
Added two complementary ways to drag, with parity across iOS + Android, plus the
host-side fix that makes a held-button drag actually register on macOS.

- **Tap-and-a-half gesture** — a one-finger move that closely follows a tap
  (within 300 ms) presses the left button at the start of the move and releases
  it on lift, so you tap, then tap-hold-drag. A plain quick double-tap still
  double-clicks (the second, stationary tap clicks normally).
- **Drag-lock button** — a new **Drag / Drop** button between Left/Right click
  holds the left button down so any one-finger move drags; tap **Drop** (or the
  centre lock, or leave the screen) to release. The hint text + a `DisposableEffect`
  / `onDisappear` safety release cover the held state.
- **Host fix (macOS):** `crates/host` + `crates/host-macos` now track the held
  left button and post moves as `LeftMouseDragged` (not `MouseMoved`) while it's
  down — Quartz only treats the former as a drag, so without this a held-button
  move wouldn't select text / drag windows. The **Windows host needs no change**
  (`MOUSEEVENTF_MOVE` + a held button drags natively).
- **No protocol change** — uses the existing `Input::MouseButton`/`MouseMoveRelative`,
  so it's backward compatible. For the best macOS drag, release host + app
  together (an old macOS host degrades gracefully — moves just may not drag).
- **Build:** Android `:app:compileDebugKotlin` green. iOS `TrackpadView.swift` and
  the macOS host changes are reviewed-not-compiled on this Windows box (no Xcode /
  no macOS toolchain) — verify the drag on device next macOS+phone session.
- Files: `apps/android/.../MainActivity.kt` (`TrackpadScreen`),
  `apps/ios/ScreenExtender/TrackpadView.swift`, `crates/host/src/main.rs`,
  `crates/host-macos/src/host.rs`.

## Update — 2026-06-28 (Rename saved hosts on every client + capture-teardown fix)

On-device test session (Mac host + iPhone JPM). Follow-ups to the virtual-display
work below.

- **Capture no longer wedges the accept loop.** Removing the streamed display (or
  any SCStream error) killed frame delivery, but `stream_to_client` blocked on
  `rx.recv()` forever, so `serve_video` never returned and the next connect did
  nothing. `serve_video` now attaches an SCStream delegate
  (`new_with_delegate` + `StreamCallbacks`) that flips a `dead` flag on
  error/stop, and `stream_to_client` polls with `recv_timeout` and returns when
  dead/disconnected. **Confirmed on device:** connect → stream → Remove →
  `SCStream stopped` → reconnect creates a fresh display and streams. (`d1ab9dc`)
- **Display rename label = `Friendly (Device)`** e.g. "Screen (iPhone)". The
  virtual-displays panel's per-row **Rename** sets the row's main name (no separate
  "override" line, no Clear button — blank resets). `resolved_name(friendly,
  device)` is the single source of truth; `Display` stores `device_base` so the
  live label updates immediately and re-renaming doesn't nest brackets. (`f8806f2`)
- **Rename saved hosts — shipped on ALL surfaces** (same friendly-name pattern,
  shown as `Custom (host)`):
  - **macOS host** Recent connections list — per-row Rename + inline editor;
    `RecentConn.name` (serde-default), preserved across reconnect. (`0491eee`)
  - **iPhone** Saved Connections — `SavedConnection.customName` +
    `ConnectionStore.setCustomName`; row ⋯ menu → Rename → alert+TextField.
    Built for device + **installed on iPhone JPM** (`xcodebuild` device build,
    `devicectl install`). (`7b1b57b`)
  - **Web** client — `saved.js` `customName`/`setCustomName`; `renderSaved` shows
    a ✎ rename (prompt) + × forget. (`4f23661`)
  - **Android** — `SavedConnection.customName` + `setCustomName` (model was
    already there); added the Rename button + AlertDialog in `SavedConnectionRow`.
    `:app:compileDebugKotlin` clean. (`dcd60e3`)
- **iOS build/install recipe (works):** `xcodegen generate` then
  `xcodebuild -project ScreenExtender.xcodeproj -scheme ScreenExtender -configuration
  Debug -destination 'id=<device-udid>' -allowProvisioningUpdates -derivedDataPath
  build/dd build`, then `xcrun devicectl device install app --device <udid>
  build/dd/Build/Products/Debug-iphoneos/ScreenExtender.app`. Team ZH9C5TS86A,
  automatic signing. The **simulator** build fails to link (xcframework has no
  x86_64 slice — only `ios-arm64` + `ios-arm64-simulator`); device builds are fine.

## Update — 2026-06-27 (macOS host: list / rename / remove virtual displays)

Backlog "rename + delete virtual displays from the PC side" — done for the
**macOS** host (`extender-host-macos`). `cargo build -p extender-host-macos` clean
(one pre-existing `listener_stop` dead-code warning, unrelated). **Needs an
on-device test** (Mac host + iPhone in Second-screen mode) to confirm create →
list → rename → remove behaves.

- **Shim** (`shim/virtual_display.m`): replaced the single `g_display` global with
  an `NSMutableDictionary` keyed by `CGDirectDisplayID` (`@synchronized`-guarded),
  and added `extender_vdisplay_destroy(id)` — removing the dict entry drops the
  last ARC ref so the window server tears the display down.
- **Host** (`host.rs`): new shared `VDisplays` registry (`Arc<Mutex<…>>`):
  `entries: Vec<Display>` (now `Clone`, fields `pub(crate)`) + a `friendly_name`
  override. `ensure_display` rewritten to work against the registry — reconciles
  against `CGDisplay::active_displays()`, reuses a live match (size + resolved
  name), tears down stale/mismatched ones (no leak), and the resolved name is the
  user's `friendly_name` override when set else the connecting device name. New
  `remove_display()` (calls destroy + drops the entry — callable from the GUI
  thread) and `set_friendly_name()`. `serve_session`/`serve_loop`/`run_cli`
  thread the `Arc<Mutex<VDisplays>>` through instead of a server-thread-local
  `Option<Display>`.
- **GUI** (`gui.rs`): a "Virtual displays (n)" collapsing panel — lists each live
  display (name · WxH · id) with a **Remove** button, plus a **Friendly name**
  field (Apply / Clear). The override applies on the next display (re)create
  (a CGVirtualDisplay can't be renamed live), which also stops the label flipping
  per connected device.
- **Single-display reality:** the host still serves one virtual display at a time,
  so the list shows 0–1 entries; it's a `Vec`/registry so the UI + a future
  multi-display host need no reshaping.
- **Windows host:** intentionally NOT changed — it captures a pre-existing
  secondary monitor (whose name belongs to the display driver) rather than
  creating a `CGVirtualDisplay`, so "rename/delete a virtual display we made"
  doesn't map to it. Backlog item is macOS-complete; Windows N/A by design.

## Update — 2026-06-27 (Viewer transparent overlay top bar — web + Android)

Backlog sweep. Web + Android viewers now match the iPhone's transparent overlay;
the input/host-display items still need on-device hardware testing (see below).

- **Android viewer top bar is now a translucent overlay too** (`MainActivity.kt`,
  `AppRoot`). The streaming modes (Mirror / Remote control / Second screen) were a
  `Column { opaque bar; StreamScreen }` — the bar pushed the video down and a tap
  removed it entirely. They're now a `Box { StreamScreen(fillMaxSize); overlay bar
  aligned TopCenter }` with a `Brush.verticalGradient(Black 55% → Transparent)` +
  `statusBarsPadding()`, so the video keeps full height and the bar floats over it
  (tap still toggles `chrome`). The control modes (Clicker / Trackpad) keep the
  normal `Column` flow (their button UIs need the bar above, not overlaid). Added
  imports `Brush`, `statusBarsPadding`. `:app:compileDebugKotlin` BUILD SUCCESSFUL.
- **Web client top bar is now a transparent overlay** (`apps/web/index.html`,
  CSS only). The session-view `.topbar` was a solid `--card` strip above the
  canvas; it's now `position: absolute` over the top of `#stage` with a
  translucent dark gradient (`rgba(0,0,0,.55)→0`) + safe-area top padding, so the
  streaming canvas gets the full height by default — matching the iPhone client.
  `pointer-events: none` on the bar with `pointer-events: auto` on the buttons
  means only the controls capture clicks; the rest of the strip passes through to
  the canvas (important for remote-control mode). Buttons got a translucent
  blurred pill style so they read over bright video. Committed to `main`.

### Screens backlog items still open (need a host + device to verify — NOT done)
- **Trackpad click-and-drag** (input protocol, client+host).
- **Remote control viewer can't click/interact** (input forwarding bug).
- **Host rename/delete of virtual displays** (macOS `CGVirtualDisplay` can't be
  renamed live — needs recreate; + GUI in `host-macos/gui.rs` / Windows host).
- **Android parity + connection-quality audit** vs. the iPhone client.
These touch live input/streaming on a working tool, so they want real hardware in
the loop rather than a blind edit. Branches `feat/ios-device-named-displays`,
`fix/v10-client-recompile`, `build/android-gradlew-exec` remain unmerged.

## Update — 2026-06-27 (v10 client recompile — web, desktop, Android)

Follow-up to the protocol v9→v10 bump below: all clients recompiled against v10.

- **Desktop client** (`extender-client`): rebuilt clean.
- **Web** (`protocol-wasm` → `apps/web/pkg`): rebuilt with `wasm-pack --dev --target
  web`; `node apps/web/verify-wasm.mjs` passes ALL OK at v10. (Stale `encode_hello`
  byte expectation + 3 five-arg `extender_session_connect` test calls fixed — PR #14.)
- **Android**: full toolchain set up on this Mac and the APK built against v10.
  - Installed **NDK r27c** at `~/Library/Android/sdk/android-ndk-r27c` (downloaded
    directly from Google — there was no `sdkmanager`/`cmdline-tools`). Point
    `cargo-ndk` at it with `ANDROID_NDK_HOME=~/Library/Android/sdk/android-ndk-r27c`.
  - Installed Rust targets `aarch64/armv7/x86_64-linux-android` + `cargo-ndk` v4.1.2.
  - Build: `ANDROID_NDK_HOME=… cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o
    apps/android/app/src/main/jniLibs build -p extender-android-jni --release`, then
    `cd apps/android && JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
    ./gradlew assembleDebug`. APK → `apps/android/app/build/outputs/apk/debug/app-debug.apk`.
  - **Fixed:** `apps/android/gradlew` was committed non-executable (100644); restored
    the exec bit so the documented `./gradlew` build works.
  - **Not installed:** no Android device was connected (`adb devices` empty). APK is
    built but needs `adb install -r …` on a device.

All clients are now v10-consistent. The Android Rust targets / `cargo-ndk` / NDK are
one-time installs — future Android builds just need the `cargo ndk` + `./gradlew` steps.

## Update — 2026-06-27 (device-named virtual displays + emoji host icons)

**Shipped this session (iOS + macOS host):**

1. **Saved-host row icons → OS emoji.** `ConnectionStore.deviceEmoji(_:)` replaces
   the old SF-Symbol `deviceSymbol`: macOS → 🍎, Windows → 🪟, Linux → 🐧, unknown
   → 🖥️. Rendered in `ConnectView.savedRow` (kept the orange-tinted tile). The row
   title is the host's `hostname` (PC name) with `ip:port` underneath — unchanged.

2. **Virtual displays named after the connecting device.** Protocol bumped
   **v9 → v10**: added `device_name: String` to `ClientHello` (so it is no longer
   `Copy`) and `ClientPlatform::device_label()`. The macOS host threads the name
   `read_hello → serve_session → ensure_display → extender_vdisplay_create`, and the
   ObjC shim (`virtual_display.m`) sets `descriptor.name` from it. The display is
   **recreated when the name changes** (a `CGVirtualDisplay` can't be renamed live),
   so swapping between two same-model devices relabels the macOS display.
   - **Tier A** (no name sent) → generic label (`iOS device`, `Windows PC`, …).
   - **Tier B** → iOS app has a **"This device's name"** field in the connect
     screen's *Advanced* section (`ConnectionStore` `deviceDisplayName` in
     UserDefaults; defaults to `UIDevice.current.name`, i.e. "iPhone" on iOS 16+).
     Sent via the FFI: `extender_session_connect(..., device_name)`.
   - **Windows host:** intentionally ignores the name — it captures a pre-existing
     secondary monitor whose name belongs to the display driver, not our code.

**Deploy state:**
- Branch `feat/ios-device-named-displays` (NOT yet merged to `main`, NOT pushed as
  of writing — confirm before relying on this).
- iOS app **built (Release) and installed on "iPhone JPM" (iPhone 15 Pro)** via
  `devicectl` over the network tunnel. xcframework rebuilt (FFI signature changed):
  `libextender_mobile_ffi.a`, slices `ios-arm64` + `ios-arm64-simulator`.
- macOS host rebuilt (`cargo build -p extender-host-macos --release`); whole
  workspace `cargo check --all-targets` is green.

**⚠️ Breaking protocol change (v9 → v10).** iOS + macOS host are rebuilt and
consistent. **Android app, web client, and desktop client have stale binaries** —
their source is updated (they send an empty `device_name`) but they must be
**recompiled** to interoperate with a v10 host. Old builds will fail the handshake.

**Left / next:**
- Rebuild + redeploy Android / web / desktop client against protocol v10.
- Optional: have Android send `Build.MODEL` and the web client send a name (both
  currently send `""`); would need their respective FFI/JS call sites extended.
- The iOS "device name" field lives under *Advanced* — consider surfacing it more
  prominently if users don't find it.

## 1. Project baseline

Universal Screens: a Rust core (`crates/`) driving native clients (iOS, Android,
web, desktop) that connect to a host (`extender-host-macos`, `extender-host-windows`)
to act as a second screen / remote control / presentation clicker. The iOS app
(`apps/ios`) is assembled with `xcodegen` from `project.yml` and links the Rust core
through the C ABI in `crates/mobile-ffi` (`extender_ffi.h`) via
`ExtenderMobile.xcframework`. Build/run notes live in `apps/ios/README.md`.
