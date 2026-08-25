# Universal Screens

Use a phone or another computer as a **clicker, trackpad, remote control, mirror,
or second screen** for your PC — and (planned) stream the phone *to* the PC for
live demos. Part of the UNI·SIM open-source suite.

## How it works

Everything is **host ⇄ client** over one platform-agnostic protocol (length-
prefixed `postcard` frames; H.264 for video):

- **Host** = the machine that *gives up* a screen (captures + streams it, and/or
  receives input). `extender-host-windows` (Windows), `extender-host` (macOS) and
  `extender-host-linux` (Linux, input only).
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
| `crates/host-linux` | Linux host — clicker + trackpad only, via uinput. No capture: see [docs/LINUX-HOST.md](docs/LINUX-HOST.md). |
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

**Linux hosts the input modes only** — Clicker and Trackpad. There is no capture
backend, so Mirror, Remote control and Second screen are unavailable, and the
clicker's window picker is missing because Wayland has no way to enumerate
windows. [docs/LINUX-HOST.md](docs/LINUX-HOST.md) is the scope; the rest is
staged, not abandoned.

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
  [docs/LINUX-APP.md](docs/LINUX-APP.md), but **not** NASM — there's no encoder in
  this build. Package it with `./scripts/build-appimage.sh`. ⚠️ Input needs write
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
- **Windows installer** — done. `scripts/build-installer.ps1` (or a `v*` tag, via
  [`windows-release.yml`](.github/workflows/windows-release.yml)) produces a
  per-user, no-admin, statically-linked `UniversalScreens-Setup-*.exe`. Nothing is
  published yet, so the download page still says "coming soon" — cutting the first
  tag is what flips it.
- **macOS packaging** — done. Universal DMG, ad-hoc signed, built by
  [`macos-release.yml`](.github/workflows/macos-release.yml) and attached to
  **v0.1.0**; the download page offers Mac alongside Windows. See
  [docs/MACOS-APP.md](docs/MACOS-APP.md).
- **Linux packaging** — done for what exists. `scripts/build-appimage.sh` (or a
  `v*` tag, via [`linux-release.yml`](.github/workflows/linux-release.yml))
  produces an unsigned `UniversalScreens-*.AppImage`. Nothing is published yet,
  so the download page still says "coming soon" — cutting the first tag flips it.
  See [docs/LINUX-APP.md](docs/LINUX-APP.md).
- **Linux capture** — not started, and the reason the Linux host is input-only.
  Scoped in [docs/LINUX-HOST.md](docs/LINUX-HOST.md) as Stages 2 (X11) and 3
  (Wayland/PipeWire), which are different jobs rather than one.

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
