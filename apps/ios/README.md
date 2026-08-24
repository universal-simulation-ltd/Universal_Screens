# Universal Screens — iOS app

A SwiftUI client that connects to an `extender-host` as a **presentation
clicker**, mirroring the Android app. It drives the shared Rust core through the C
ABI in `crates/mobile-ffi` (`extender_ffi.h`).

> **Status: builds, installs and runs on a real iPhone.** Last confirmed
> **2026-08-24** on an iPhone 15 Pro — built, signed (team ZH9C5TS86A, automatic
> provisioning), installed with `devicectl`, launched, and observed completing the
> handshake against `extender-host-macos`:
>
> ```
> client 192.168.4.39 hello: 1920x1080, mode ControlOnly, platform Ios, device "iPhone"
> ```
>
> This block used to read *"scaffold — not built … authored on Windows without
> Xcode"*. That stopped being true no later than June 2026, and a stale status
> line is worse than none: it invites the next person to treat working code as
> unproven, or to rebuild scaffolding that already exists.

> ⚠️ **Rebuild the xcframework whenever the Rust side moves.** It is gitignored,
> so a fresh clone has none, and a stale one is worse than a missing one — it
> links and installs happily and then fails at the protocol. The copy on this Mac
> predated the Noise transport encryption (`081c168`) by six weeks and had to be
> rebuilt before the app would talk to a current host. If a connection fails right
> after a `crates/` change, suspect this first.

## What's here

```
apps/ios/
  ScreenExtender/
    ScreenExtenderApp.swift          # @main App
    ContentView.swift                # connect → clicker
    ConnectView.swift                # host ip:port entry
    ClickerView.swift                # Prev/Next, slide preview, Scan deck, window picker, More options
    StreamView.swift                 # viewer / full-control: VideoToolbox + touch forwarding
    VideoDecoder.swift               # Annex-B H.264/HEVC -> AVSampleBufferDisplayLayer
    ExtenderSession.swift            # Swift wrapper over the C FFI (+ event pump)
    ConnectionStore.swift            # saved-connection persistence (UserDefaults)
    HidKeys.swift                    # HID usage ids for the clicker
    ScreenExtender-Bridging-Header.h # imports extender_ffi.h
```

The clicker connects in **control-only** mode (input only, no video) and is at
feature parity with the Android clicker: slide preview (current + previous/next),
**Scan deck** look-ahead, a **window picker**, and a **Start-show-on-focus (F5)**
toggle. The connect screen remembers hosts (saved connections with an OS icon;
swipe to hide / delete). Viewer and full-control modes decode the stream with
`VideoToolbox` into an `AVSampleBufferDisplayLayer` (full-control also forwards
touches) — **drafted but unverified**; the decode path wants on-device testing.

## Building the Rust static library

Build `extender-mobile-ffi` (a `staticlib`) for the iOS targets and bundle the
slices into an `.xcframework`:

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
cargo install cargo-lipo   # optional helper; or build each target with cargo

# Device + simulator slices:
cargo build -p extender-mobile-ffi --release --target aarch64-apple-ios
cargo build -p extender-mobile-ffi --release --target aarch64-apple-ios-sim

# Wrap into an xcframework the Xcode project can link:
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libextender_mobile.a \
      -headers crates/mobile-ffi/include \
  -library target/aarch64-apple-ios-sim/release/libextender_mobile.a \
      -headers crates/mobile-ffi/include \
  -output apps/ios/libs/ExtenderMobile.xcframework
```

(Must be done on a Mac — the iOS targets need the Apple SDKs.)

⚠️ The library is `libextender_mobile_ffi.a`, not `libextender_mobile.a` as the
commands above once said — `-create-xcframework` fails on the wrong name.

## Build and run on a connected iPhone

```bash
cd /Users/jamesmarkey/Github/UNISIM/Universal_Apps/Universal_Screens/apps/ios
xcrun devicectl list devices          # find your device's identifier
xcodegen generate
xcodebuild -project ScreenExtender.xcodeproj -scheme ScreenExtender \
  -configuration Debug -destination 'id=YOUR_DEVICE_ID' \
  -allowProvisioningUpdates -derivedDataPath build/dd build
xcrun devicectl device install app --device YOUR_DEVICE_ID \
  build/dd/Build/Products/Debug-iphoneos/ScreenExtender.app
xcrun devicectl device process launch --device YOUR_DEVICE_ID \
  com.universalsim.screenextender
```

⚠️ **Device builds only.** The **simulator** build fails to link: the xcframework
carries `ios-arm64` and `ios-arm64-simulator`, with no x86_64 slice.

## Assembling the Xcode project

1. **New → Project → iOS App** named `ScreenExtender` (SwiftUI, Swift). Put it in
   `apps/ios/` (or point it at these sources).
2. **Add the Swift files** in `ScreenExtender/` to the target (delete Xcode's
   generated `ContentView.swift` / `App.swift` first to avoid duplicates).
3. **Bridging header:** Build Settings → *Objective-C Bridging Header* →
   `apps/ios/ScreenExtender/ScreenExtender-Bridging-Header.h`. Add
   `crates/mobile-ffi/include` to *Header Search Paths*.
4. **Link the library:** add `ExtenderMobile.xcframework` (from the step above) to
   *Frameworks, Libraries, and Embedded Content*.
5. **Local network:** add `NSLocalNetworkUsageDescription` to Info.plist (iOS gates
   LAN access); the user is prompted on first connect.
6. **Run** on a device or simulator, enter the host's `ip:port`, and Connect. Tap
   ◀ / ▶ to drive slides. (For the Windows host, run
   `cargo run -p extender-host-windows`.)

## Remaining work

- **Compile + on-device test** — the whole Swift app is an unbuilt scaffold (no
  Xcode/Mac here). The VideoToolbox path (`VideoDecoder` Annex-B → AVCC, format
  description from parameter sets, sample-buffer enqueue) especially needs a real
  build + a live stream to validate and tune (frame pacing, error recovery).

The C ABI (`extender-mobile-ffi`) is at parity with `crates/android-jni`: the
`Snapshot` / `HostInfo` / `WindowList` events and the `ScanDeck` / `ListWindows` /
`FocusWindow` sends are all exposed.
