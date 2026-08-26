# Universal Screens

Use a phone or another computer as a **clicker, trackpad, remote control, mirror,
or second screen** for your PC — and (planned) stream the phone *to* the PC for
live demos. Part of the UNI·SIM open-source suite.

## How it works

Everything is **host ⇄ client** over one platform-agnostic protocol (length-
prefixed `postcard` frames; H.264 for video):

- **Host** = the machine that *gives up* a screen (captures + streams it, and/or
  receives input). `extender-host-windows` (Windows), `extender-host` (macOS) and
  `extender-host-linux` (Linux; clicker everywhere, plus previews and mirror on
  X11).
- **Client** = the device that *shows / drives* it. `extender-client` (desktop,
  cross-platform) and the **Android app** (`apps/android`); **iOS** is a scaffold.

"Extend"/"mirror" extend the **host's** desktop; the client just displays it.
Which way you point it decides who's host vs client (e.g. *phone as a 2nd screen
for Windows* → Windows is the host, phone is the client).

## Crates / apps

| Path | What |
|---|---|
| `crates/protocol` | wire types (`ClientHello`, `Message`, `Input`, `CaptureMode`), framing, NAL helpers. Protocol **v10**. |
| `crates/core` | client `Session` (handshake, event stream, input). |
| `crates/host` | macOS host — ScreenCaptureKit + VideoToolbox, CGVirtualDisplay (extend). |
| `crates/host-windows` | Windows host — clicker, mirror, remote control, second screen, trackpad, GUI. |
| `crates/host-linux` | Linux host — clicker + trackpad (uinput), and on X11: slide previews, window picker, H.264 mirror. No second screen: see [docs/LINUX-HOST.md](docs/LINUX-HOST.md). |
| `crates/h264` | Annex-B → wire framing + encode sizing, shared by the mirroring hosts. |
| `crates/client` | desktop client — openh264 decode + wgpu display. |
| `crates/mobile-ffi` | C ABI for mobile clients (`extender_ffi.h`). |
| `crates/android-jni` | JNI bridge → `libextender_mobile.so`. |
| `apps/android` | Jetpack Compose app (the main mobile client). |
| `apps/ios` | SwiftUI scaffold (not built yet). |
| `web/` | `assetlinks.json` for the "get the app" App Link. |

## Modes (phone/desktop client → Windows host)

| Mode | What | Status |
|---|---|---|
| **Clicker** | slide remote: keys + live slide previews, deck pre-scan, window picker, PIN pairing | ✅ |
| **Trackpad** | relative mouse, tap/scroll/right-click, click-and-drag (tap-and-a-half + Drag-lock button), sensitivity slider, haptics | ✅ |
| **Mirror** | view the host screen (H.264) — letterboxed, pinch-zoom/pan, cursor shown | ✅ |
| **Remote control** | mirror + forward touch/keys; hold-handle to toggle the bar | ✅ |
| **Second screen** | host streams a *virtual* monitor (extend) | ✅ app+host; needs a virtual-display driver — see [docs/SECOND-SCREEN.md](docs/SECOND-SCREEN.md) |

macOS host streams to the desktop client for the same modes (the original path).

**Linux hosts the Clicker and Trackpad** everywhere, and on an **X11** session
also slide previews, the deck scan, the window picker and **Mirror / Remote
control** (H.264, software openh264, same 30 fps and ≤1280px cap as Windows).
**Second screen is still unavailable** — it needs a virtual display, which on
X11 means an `xrandr` VIRTUAL output and is deferred; a client that asks for one
is mirrored instead.

⚠️ **Capture on Linux is X11 only, and that is a permanent split rather than a
staging one.** Injection goes through uinput, which works identically under X11
and every Wayland compositor. Capture does not: Wayland needs the portal and
PipeWire, and it has **no window-enumeration protocol at all**, by design. So on
a Wayland session the previews are off and the window picker is empty — the host
says which, on startup and per session, rather than going quietly blank.
[docs/LINUX-HOST.md](docs/LINUX-HOST.md) is the scope; the rest is staged, not
abandoned.

## Connect flow

1. **Step 1 – Get the app:** host shows a QR to `opensource.unisim.co.uk/screens`
   (opens the app if installed via App Links, else the download page).
2. **Step 2 – Scan to connect:** a **combined QR** that joins the host's Wi-Fi
   *and* connects in one scan (the app uses `WifiNetworkSpecifier`); or type the
   address + 4-digit PIN. Over USB use `adb reverse tcp:9000 tcp:9000` →
   `127.0.0.1:9000`.

## Build / run (quickstart)

- **Windows host:** `cargo run -p extender-host-windows` (GUI) or
  `… -- 0.0.0.0:9000` (headless). Needs **NASM** (openh264 builds from source).
  To package it for other people: `.\scripts\build-installer.ps1` →
  `dist\UniversalScreens-Setup-*.exe` — see [docs/WINDOWS-INSTALLER.md](docs/WINDOWS-INSTALLER.md).
- **Android:** rebuild the native lib with `cargo-ndk`, then
  `apps/android/gradlew assembleDebug` → `adb install -r` — see
  [apps/android/README.md](apps/android/README.md).
- **macOS host / desktop client:** see `scripts/preview.sh` /
  [docs/WINDOWS-CLIENT.md](docs/WINDOWS-CLIENT.md).
- **Linux host:** `cargo run -p extender-host-linux` (GUI) or `… -- 0.0.0.0:9000`
  (headless). Needs the X11/Wayland dev packages listed in
  [docs/LINUX-APP.md](docs/LINUX-APP.md) — those are for the GUI; capture links
  nothing, since `x11rb` speaks the X protocol over the socket. **Not** NASM,
  though: there's no encoder in this build. Package it with `./scripts/build-appimage.sh`. ⚠️ Input needs write
  access to `/dev/uinput`; without the udev rule the app runs and silently injects
  nothing, which is why it checks on startup.

## Status — outstanding / needed

**In progress (working tree):**
- **Hardware H.264 encode** (DXGI Desktop Duplication + Media Foundation MFT) for
  the PC→client stream — `stream_hw.rs` + the MF Cargo features are scaffolded but
  incomplete (build is mid-edit until `stream_hw.rs` lands). Removes the current
  720p downscale workaround (the software encoder is CPU-bound, so the stream is
  capped at ≤1280px long-side to stay smooth).

**Queued (background tasks):**
- **Phone → PC streaming** — present the phone's screen on the projector for live
  app demos, with a "Present my phone" toggle (MediaProjection + upstream video).

**Shipping:**
- **Windows installer** — done and **published** (v0.2.0).
  `scripts/build-installer.ps1` (or a `v*` tag, via
  [`windows-release.yml`](.github/workflows/windows-release.yml)) produces a
  per-user, no-admin, statically-linked `UniversalScreens-Setup-*.exe`.
- **macOS packaging** — done. Universal DMG, ad-hoc signed, built by
  [`macos-release.yml`](.github/workflows/macos-release.yml) and attached to
  **v0.1.0**; the download page offers Mac alongside Windows. See
  [docs/MACOS-APP.md](docs/MACOS-APP.md).
- **Linux packaging** — done and **published** (v0.2.0, the first Linux
  release). `scripts/build-appimage.sh` (or a `v*` tag, via
  [`linux-release.yml`](.github/workflows/linux-release.yml)) produces an
  unsigned `UniversalScreens-*.AppImage`.
  See [docs/LINUX-APP.md](docs/LINUX-APP.md).
- **Linux capture** — X11 done, mirror included (previews, deck scan, window
  picker, and H.264 at 30 fps; MIT-SHM with a `GetImage` fallback, nothing
  linked). What's left is the **second screen** (an `xrandr` VIRTUAL output —
  a request for one is served as a mirror meanwhile) and then **Wayland**
  (portal + PipeWire), a different job again.
  See [docs/LINUX-HOST.md](docs/LINUX-HOST.md) §7.

**Tests:** [`tests.yml`](.github/workflows/tests.yml) runs on every push — the
shared crates and the Linux host (under Xvfb, where a missing display is a
failure rather than a skip) on Linux, the Windows host on Windows, plus the
browser client's Node tests including the encrypted leg end to end.
⚠️ `cargo test --workspace` **fails by design** — each host crate uses its own
platform's API unconditionally — so every job names its packages.

**Deploy-time / external (can't be done in-repo):**
- Host `web/.well-known/assetlinks.json` at the domain root; add the **Play
  release** signing fingerprint (file currently has the debug cert only); fill in
  the real store URLs.
- Install a virtual-display driver (IddCx) on Windows for **Second screen**.
- **iOS**: generate the Xcode project from the scaffold (incl. an AppIcon).

**Untested combos:**
- Desktop client on **macOS** (the Windows → Mac path) — cross-platform crate,
  not yet verified building on macOS.
- ⚠️ **The Linux host has never injected a keystroke.** It compiles, links, passes
  its tests and packages into an AppImage that starts and accepts a connection —
  all proven in a container, which by definition has no `/dev/uinput` and no
  desktop. The GUI has never been drawn either. The first job on a real Linux
  machine is to install the udev rule and check a phone actually moves a slide.
  ⚠️ **Capture is the exception**, and worth not lumping in with the above: it is
  genuinely exercised, because Xvfb is a real X server rather than a stand-in —
  tests paint a root window and read the pixels back (the mirror's too, decoding
  its H.264 again to check the colour survived), and CI fails rather than skips
  if no display is present. What is untested there is a *desktop*: a compositing
  window manager, a multi-monitor layout, and a GPU readback path instead of a
  software framebuffer — and, for the mirror, whether software openh264 holds 30
  fps on real hardware.

## Security

Native connections are **PIN-gated and transport-encrypted**. Right after the TCP
connect, the client and host run a **Noise** handshake
(`Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s`, via the `snow` crate) keyed by the
pairing PIN, and every `postcard` frame after it — the `ClientHello`, injected
keystrokes/text, and the mirror video — travels inside that tunnel. See
[`crates/transport`](crates/transport/src/lib.rs) and
[docs/M10-transport-encryption.md](docs/M10-transport-encryption.md).

- **Confidentiality + forward secrecy:** the ephemeral-ephemeral DH means a passive
  eavesdropper on the LAN learns nothing, even if the PIN later leaks.
- **PIN-bound MITM resistance:** the PIN is the Noise pre-shared key, so an on-path
  attacker can't complete (or silently relay) the handshake without it. The PIN is
  now *encryption*, not just a gate. The existing plaintext-`ClientHello` PIN check
  is kept unchanged inside the tunnel.

The host auto-detects the peer: an encrypting native client is required to speak
Noise, while the loopback WebSocket **browser bridge** (`crates/web-bridge`, which
can't speak Noise on a browser's behalf) is still accepted as plaintext and logged.
The **browser client** leg is therefore not yet end-to-end encrypted (it relies on
`wss://` to the cloud rendezvous); requiring encryption from every non-loopback peer
is a follow-up once every client has shipped this build.
