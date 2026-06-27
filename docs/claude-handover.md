# Claude session handover

Newest entry first. Each dated `## Update` overrides anything older that conflicts.
A `SessionStart` hook injects the top ~150 lines into new sessions, so keep the
newest entry at the top.

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
