# Linux host — feasibility and scope

**Status: scoping only. No Linux code exists yet.** The backlog said a Linux host
"should be scoped as one before anyone starts" — this is that scope.

**Verdict: feasible, and the shared spine is already portable — but it is not one
port.** Linux has two display stacks with different capabilities, and the app's
five modes do *not* all survive the crossing. The recommendation is a staged
Clicker-first host, not a "build the Linux host" project.

## 1. What is already portable (measured, not assumed)

`cargo check --target x86_64-unknown-linux-gnu` on the Windows dev box:

| Crate | Result |
|---|---|
| `extender-protocol` | ✅ clean |
| `extender-transport` (Noise) | ✅ clean |
| `extender-core` | ✅ clean |
| `extender-discovery` (beacon + mDNS) | ✅ clean |
| `extender-client` | ❌ toolchain only — see below |
| `extender-web-bridge` | ❌ toolchain only — see below |

There is **not one `cfg(target_os)` / `cfg(windows)` / `cfg(unix)`** in any of
`protocol`, `transport`, `discovery`, `core`, `web-bridge` or `client`. The wire
format, the PIN-keyed Noise handshake, LAN discovery, the cloud rendezvous and
the browser bridge all come to Linux for free.

Both failures are **cross-compiling from Windows**, not portability:

- `extender-client` — `openh264-sys2` wants `x86_64-linux-gnu-g++`. NASM
  assembled the x86 source fine; only the C++ compiler is missing. Natively on
  Linux this is `build-essential` + `nasm`, the same NASM requirement the Windows
  host already documents.
- `extender-web-bridge` — `openssl-sys` can't find a Linux libssl.

⚠️ **`crates/web-bridge/Cargo.toml` is wrong about Linux.** Its comment reads
"macOS uses Security.framework, Windows uses SChannel — **no OpenSSL to vendor**".
On Linux, `native-tls` *is* OpenSSL: the build needs `libssl-dev` and the binary
then carries a distro-specific runtime dependency — poison for a
download-and-run AppImage. Switch the `tungstenite` feature to
`rustls-tls-native-roots` before packaging anything. Worth doing regardless; it
removes a C dependency from all three hosts.

## 2. What genuinely has to be written

The Windows host is 3,429 lines. Subtract the parts that are already OS-agnostic
in substance — the 1,598-line egui GUI, QR rendering, the discovery adapter, and
the openh264 encode and framing in `stream.rs` — and the actual platform surface
is **about 880 lines across six primitives**:

| Primitive | Windows | Lines |
|---|---|---|
| Input injection + HID→keycode map | `SendInput` (`main.rs` 448–767) | ~320 |
| Screen grab → BGRA (+ cursor composite) | GDI `BitBlt`/`GetDIBits` (`snapshot.rs`) | 149 |
| Monitor enumeration (mirror vs extend region) | `EnumDisplayMonitors` (`stream.rs`) | ~120 |
| Window list + raise to foreground | `EnumWindows`/`SetForegroundWindow` (`winlist.rs`) | 68 |
| Current Wi-Fi SSID + PSK (the join-QR) | `netsh wlan` (`wifi.rs`) | 145 |
| Inbound firewall rule detect/add | `netsh advfirewall` (`firewall.rs`) | 82 |

Six primitives is the whole job — **but on Linux each may need two
implementations**, X11 and Wayland.

## 3. Mode × display server — where it stops being a port

| Mode | X11 | Wayland |
|---|---|---|
| **Clicker** (control-only) | XTEST — a direct `SendInput` equivalent | portal `RemoteDesktop`/libei, **or** `uinput` |
| **Slide preview snapshots** | `XGetImage`/XShm | ScreenCast portal → PipeWire |
| **Window picker** | EWMH `_NET_CLIENT_LIST` + `_NET_ACTIVE_WINDOW` | ❌ **no protocol exists** — by design |
| **Trackpad** | XTEST relative motion | libei / `uinput` |
| **Mirror** | XShm capture | ScreenCast portal (consent dialog each session) |
| **Remote control** | XTEST + capture | portal + libei |
| **Second screen** (extend) | `xrandr` VIRTUAL output | ❌ compositor-specific, no generic route |

Three consequences that change the product, not just the code:

- **Wayland has no window enumeration at all.** The clicker's window picker — a
  shipped ✅ feature — cannot be ported. It has to *degrade*, not port: on
  Wayland the clicker drives whatever is focused, and the UI has to say so.
- **Every Wayland capture opens with a user consent dialog** picking what to
  share. There is no "just grab the primary display". That is a first-run UX
  design question for Mirror and for the clicker's slide previews, not plumbing.
- ⚠️ **The input-injection fork is the decision worth making first.** The portal
  route is the sanctioned one but needs a compositor that implements it (GNOME
  45+, KDE Plasma 6; wlroots partial) and prompts for consent. **`uinput` is one
  implementation that covers X11 *and* Wayland, every compositor, no portal** —
  it collapses the 2× fan-out for the whole control half of the app. Its cost is
  write access to `/dev/uinput`: a udev rule at install, or a first-run
  instruction. For a clicker-first host, uinput is the cheaper and broader
  answer by a wide margin.

## 4. The two shell-outs degrade rather than port

- **Wi-Fi (`wifi.rs`).** `nmcli` gives the SSID, and the PSK via
  `nmcli -s -g 802-11-wireless-security.psk connection show <name>` — but reading
  a *secret* triggers a polkit prompt on most distros, and iwd /
  systemd-networkd systems have no equivalent at all. Expect the combined
  join-Wi-Fi QR to fall back to the existing "connect to the same network" note
  far more often than on Windows. Not a blocker; just don't build the UI
  assuming it works.
- **Firewall (`firewall.rs`).** No `netsh` equivalent, and no single target:
  ufw, firewalld and raw nftables all differ. Note that Ubuntu ships ufw
  *inactive*, so most desktop users need nothing — while Fedora's firewalld is
  active and **will** block 9000. Detect which is running, show the exact
  command, and don't attempt the pkexec elevation dance the Windows host does
  with UAC.

## 5. Packaging and CI

- **AppImage first.** It is the only format that matches what Universal Screens
  already does on Windows and macOS — per-user, no admin, unsigned,
  download-and-run — and it fits the standing ship-unsigned policy. A `.deb` can
  follow if anyone asks.
- ⚠️ **Not Flatpak, at least not first.** Flatpak is sandboxed and *forces* the
  portal path: no `uinput`, no XTEST outside the sandbox without punching holes.
  It removes exactly the capability this app exists to provide.
- **CI is the cheap part.** `ubuntu-latest` is the cheapest of the three runners,
  and `macos-release.yml` already carries a comment about a job that could be
  "folded into a Linux job".

## 6. ⚠️ Fix the GUI fork *before* adding a third host

`gui.rs` is 1,598 lines in `host-windows` and 1,451 in `host-macos`, already
diverged by ~1,000 lines once whitespace is normalised. A third copy means three
places to keep the navbar, the changelog popup and the profile disc in sync — and
the 2026-08-24 handover already records both navbar commits landing in
`host-windows/src/gui.rs` while that host **went uncompiled**. Extracting the
shared egui shell into a `host-ui` crate is the single biggest cost driver here,
and it is not a Linux problem; adding Linux just makes it three times as
expensive to keep ignoring.

## 7. Recommended staging

| Stage | Scope | Rough size |
|---|---|---|
| **0** | Decide: `uinput` vs portals; extract `host-ui` yes/no; swap web-bridge to rustls | ~1 hour, no feature code |
| **1** | **Clicker-only host.** uinput injection, no capture, no window picker. Works identically under X11 and Wayland. AppImage + udev rule + CI job. **This is what flips "Coming soon".** | ~1 session |
| **2** | X11 capture: XShm grab → reuse `stream.rs`/openh264 unchanged. Adds slide previews, Mirror, Remote control, EWMH window picker. | 2–3 sessions |
| **3** | Wayland capture: `ashpd` portal + PipeWire, plus the consent UX. | 4+, and needs real hardware |
| **—** | **Second screen: defer.** The Windows trick (stream the first non-primary monitor; an external driver makes it exist) transfers to X11 via a VIRTUAL output far more cheaply than macOS's `CGVirtualDisplay` did. Wayland has no generic answer. | not scheduled |

Stage 1 is deliberately the smallest shippable thing, and it is the mode the app
is actually used in at a lectern.

Hardware H.264 encode, if it is ever wanted here, is VAAPI — the Linux twin of
the Media Foundation work in progress on Windows. Software openh264 is the same
trade-off on all three platforms, so Stage 2 inherits the existing ≤1280px cap
rather than needing its own answer.

## 8. ⚠️ Testing reality

**There is no Linux machine in this workspace** — WSL here has only a stopped
`docker-desktop` distro. A container can prove Stages 0–2 compile and can run the
X11 path headlessly under Xvfb, but **Wayland cannot be validated in a container
or in headless CI**: portal consent dialogs need a real logged-in session, and
behaviour differs between GNOME and KDE.

The precedent is in this repo's own handover: `crates/host-macos` was edited
blind across several sessions, compiled clean first try — and had still never
been *run*. Stage 3 needs a real Linux desktop, tested on GNOME and KDE both.
