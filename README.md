# Universal Screens

Use a phone or another computer as a **clicker, trackpad, remote control, mirror,
or second screen** for your PC — and (planned) stream the phone *to* the PC for
live demos. Part of the UNI·SIM open-source suite.

## How it works

Everything is **host ⇄ client** over one platform-agnostic protocol (length-
prefixed `postcard` frames; H.264 for video):

- **Host** = the machine that *gives up* a screen (captures + streams it, and/or
  receives input). `extender-host-windows` (Windows) and `extender-host` (macOS).
- **Client** = the device that *shows / drives* it. `extender-client` (desktop,
  cross-platform) and the **Android app** (`apps/android`); **iOS** is a scaffold.

"Extend"/"mirror" extend the **host's** desktop; the client just displays it.
Which way you point it decides who's host vs client (e.g. *phone as a 2nd screen
for Windows* → Windows is the host, phone is the client).

## Crates / apps

| Path | What |
|---|---|
| `crates/protocol` | wire types (`ClientHello`, `Message`, `Input`, `CaptureMode`), framing, NAL helpers. Protocol **v9**. |
| `crates/core` | client `Session` (handshake, event stream, input). |
| `crates/host` | macOS host — ScreenCaptureKit + VideoToolbox, CGVirtualDisplay (extend). |
| `crates/host-windows` | Windows host — clicker, mirror, remote control, second screen, trackpad, GUI. |
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
- **Android:** rebuild the native lib with `cargo-ndk`, then
  `apps/android/gradlew assembleDebug` → `adb install -r` — see
  [apps/android/README.md](apps/android/README.md).
- **macOS host / desktop client:** see `scripts/preview.sh` /
  [docs/WINDOWS-CLIENT.md](docs/WINDOWS-CLIENT.md).

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

**Deploy-time / external (can't be done in-repo):**
- Host `web/.well-known/assetlinks.json` at the domain root; add the **Play
  release** signing fingerprint (file currently has the debug cert only); fill in
  the real store URLs.
- Install a virtual-display driver (IddCx) on Windows for **Second screen**.
- **iOS**: generate the Xcode project from the scaffold (incl. an AppIcon).

**Untested combos:**
- Desktop client on **macOS** (the Windows → Mac path) — cross-platform crate,
  not yet verified building on macOS.

## Security

Connections are **PIN-gated but unencrypted** (plaintext over the LAN). Use on
trusted networks; the in-app/host security note spells out what is and isn't
protected. The PIN is a gate, not encryption.
