# M9 — discovery (Nearby hosts, no QR) + cross-network Remote access

Lets Universal Screens clients find hosts without scanning a QR. Two transports,
one per audience:

- **Host ↔ host (PC → PC / PC → Mac):** a custom **UDP multicast beacon** — the
  original case, where there's no camera to point at a code. Discovered hosts
  show under **"Nearby"** (drawn as an orbit) in the desktop control window.
- **Phone / web → host:** the hosts also advertise a standard **DNS-SD/mDNS**
  service (`_usscreens._tcp`) so the mobile + web clients can browse for them
  with their platform APIs. iOS can't join a raw multicast group without a
  restricted Apple entitlement, but Bonjour browsing needs none — so DNS-SD is
  the mobile-facing half.

A separate **Remote access** feature (bottom of this doc) reaches a host on a
*different* network via the cloud rendezvous, not multicast.

## Wire protocol — UDP beacon (host ↔ host)

- **Transport:** UDP multicast, group `224.0.0.251`, port `9001`.
- **Beacon:** every running host that is *serving* broadcasts every **2s**. TTL is
  set to 1 so beacons stay on the local link (don't cross routers).
- **Packet:** `USSCREENS\t{port}\t{name}` — tab-separated so the machine name may
  contain colons. `{port}` is the host's TCP serving port; `{name}` is the host name.
- **Listener:** always on (from app launch, even before this host serves), joins the
  group on `0.0.0.0`, collects peers, and **prunes** any not heard from in **6s**.
  It filters out this host's own beacon by comparing the source IP to the host's own
  LAN IP.

Because the beacon only runs while a host is *serving*, "Nearby" lists hosts that are
actually ready to accept a connection.

## Wire protocol — DNS-SD/mDNS (phone / web → host)

- **Service type:** `_usscreens._tcp` (constant `MDNS_SERVICE_TYPE` in
  `crates/discovery`; iOS declares it under `NSBonjourServices`). Instance name =
  the host machine name; port = the TCP serving port.
- **Advertised only while serving** (registered in the host's `start()`, withdrawn
  in `stop()`/`on_exit()`), so a browsed host is ready to accept a connection —
  same guarantee as the beacon.
- **Why a second transport:** a browser tab can't do raw multicast, and iOS can't
  join a multicast group without the restricted
  `com.apple.developer.networking.multicast` entitlement — but Android NSD and iOS
  Bonjour browsing over DNS-SD need no special permission.

## Code layout

- **`crates/discovery` (`extender-discovery`)** — platform-agnostic, no UI.
  - Beacon: `start_listener(peers, stop, own_ip, on_change)` +
    `start_beacon(name, port, stop)`. `on_change()` fires when the visible peer set
    changes so a host can repaint without a UI dependency.
    `DiscoveredPeer { name, addr, port, last_seen }`.
  - DNS-SD: `advertise_mdns(name, port) -> MdnsAd` (withdraws on `shutdown()`) and
    `start_mdns_browser(peers, stop, on_change)` (used by the web bridge — see below).
    Backed by the `mdns-sd` responder.
- **Per-host adapter** — `crates/host-{windows,macos}/src/discovery.rs` re-exports the
  beacon + mDNS API and wraps `start_listener` to turn `on_change` into
  `egui::Context::request_repaint()`. The GUI keeps the peer list in an
  `Arc<Mutex<Vec<DiscoveredPeer>>>` and renders it as the **orbit** (`nearby_orbit` in
  `gui.rs`): this machine centred, each peer a node circling it, click to connect.
- **Android** — `apps/android/.../NearbyDiscovery.kt`: `NsdManager` browse of
  `_usscreens._tcp` with a serial resolve queue; the connect screen shows a **NEARBY**
  section, tap → PIN prompt → mode picker.
- **iOS** — `NearbyBrowser` in `apps/ios/.../ConnectView.swift`: `NWBrowser` +
  `NetService` resolve (query-only, no TCP probe); same **NEARBY** section + PIN prompt.
- **Web** — the browser can't multicast, so **`crates/web-bridge` browses DNS-SD for
  it**: `GET /peers` returns the discovered hosts as JSON, and the WS upgrade honours
  `?host=ip:port` to retarget the proxy at a *discovered* host (undiscovered targets
  refused). `apps/web` renders the **Nearby orbit** and polls `/peers`.

## Remote access (across networks)

Reaches a host on a *different* network via the cloud rendezvous (the M8
Durable-Object room — see `M8-browser-receiver.md`), not multicast:

- **Host** (`gui.rs` "Remote access" panel): *Enable remote access* mints a 6-char
  code (`gen_room_code`) and dials the room as **sender** via
  `extender_web_bridge::dial_room`, bridging its own `serve()` to the room.
- **Web client** (`apps/web` "Remote (across networks)"): enter the host's code to
  join the room as **receiver** (`RoomTransport`) and view/control it. Relayed
  through the cloud, so it warns that it's slower than LAN. `?remote=CODE` prefills.
- This is the inverse of *"cast to a browser"* (M8d), where the host enters a
  receiver's code; here the host publishes its own for a remote client.

## Status

- ✅ **Beacon** — macOS host (original) + Windows host, via the shared crate.
- ✅ **DNS-SD advertising** — both desktop hosts.
- ✅ **Mobile browsing** — Android (NSD, compiled) + iOS (Bonjour, reviewed-not-compiled).
- ✅ **Web browsing** — bridge `/peers` + `?host=` retarget + Nearby orbit.
- ✅ **Orbit visualisation** — both desktop host GUIs + the web client.
- ✅ **Cross-network Remote access** — host publishes a code, web client connects by it.
- ⏳ **Hardware verify** — recompile the macOS host on a Mac; two-machine LAN test
  (beacon + mDNS); a phone browsing a real serving host; a real two-network Remote
  session. All gated on hardware not present on the dev box.

## Testing

`cargo test -p extender-discovery` covers the beacon format **and** a live
advertise→browse mDNS round-trip. `cargo test -p extender-web-bridge` includes
`peers_endpoint` — a real end-to-end run (advertise → `/peers` lists it → `?host=`
proxies → undiscovered target refused). Host GUIs: `gen_room_code` + `truncate_label`
unit tests. End-to-end still needs **two machines on the same LAN** (each should list
the other under Nearby) and **two networks** for a Remote session.
