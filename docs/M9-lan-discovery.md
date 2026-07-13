# M9 — LAN discovery (host ↔ host, no QR)

Lets two Universal Screens **hosts** on the same network find each other without
scanning a QR — the PC → PC and PC → Mac case, where there's no camera to point at
a code. A discovered host shows up under **"Nearby"** in the control window with a
one-click **Connect**.

## Wire protocol

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

## Code layout

- **`crates/discovery` (`extender-discovery`)** — the whole thing, platform-agnostic
  (pure `std`, no UI). `start_listener(peers, stop, own_ip, on_change)` and
  `start_beacon(name, port, stop)`. The listener calls `on_change()` whenever the
  visible peer set changes, so a host can repaint without this crate depending on any
  UI toolkit. `DiscoveredPeer { name, addr, port, last_seen }`.
- **Per-host adapter** — `crates/host-{windows,macos}/src/discovery.rs` is a thin
  wrapper that re-exports `DiscoveredPeer` + `start_beacon` and wraps `start_listener`
  to turn `on_change` into `egui::Context::request_repaint()`. The GUI keeps the peer
  list in an `Arc<Mutex<Vec<DiscoveredPeer>>>` and renders a "Nearby" row per peer.

## Status

- ✅ **macOS host** — discovery (this was the original implementation; now delegates
  to the shared crate).
- ✅ **Windows host** — parity added (shared crate + "Nearby" section).
- ⏳ **Mobile / web clients** — no host browsing yet. Android → NSD, iOS → Bonjour/
  `NWBrowser`; a browser can't do raw multicast, so the web client would need the host
  to expose the peer list over its existing channel.
- ⏳ **Orbit visualisation** — the "current device in the centre, peers orbiting"
  graphic (like the portal) is still to design.
- ⏳ **Cross-network** — a separate backlog item; would reuse the cloud rendezvous /
  dial-the-room bridge (see `M8-browser-receiver.md`), not multicast.

## Testing

Unit tests for the beacon format live in `crates/discovery` (`cargo test -p
extender-discovery`). End-to-end needs **two machines on the same LAN** each running a
host — start serving on both and each should list the other under "Nearby".
