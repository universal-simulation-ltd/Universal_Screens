# Screen Extender — iOS app (scaffold)

A SwiftUI client that connects to an `extender-host` as a **presentation
clicker**, mirroring the Android app. It drives the shared Rust core through the C
ABI in `crates/mobile-ffi` (`extender_ffi.h`).

> **Status: scaffold — not built.** This was authored on Windows without Xcode, so
> it has **not** been compiled or run. The Swift sources + bridging header are a
> starting point to drop into an Xcode project (steps below). The Rust C ABI it
> links against *does* build and is unit-tested (`cargo test -p extender-mobile-ffi`).

## What's here

```
apps/ios/
  ScreenExtender/
    ScreenExtenderApp.swift          # @main App
    ContentView.swift                # connect → clicker
    ConnectView.swift                # host ip:port entry
    ClickerView.swift                # Prev/Next, slide preview, Scan deck, window picker, More options
    ExtenderSession.swift            # Swift wrapper over the C FFI
    HidKeys.swift                    # HID usage ids for the clicker
    ScreenExtender-Bridging-Header.h # imports extender_ffi.h
```

The clicker connects in **control-only** mode (input only, no video) and is at
feature parity with the Android clicker: slide preview (current + previous/next),
**Scan deck** look-ahead, a **window picker**, and a **Start-show-on-focus (F5)**
toggle. Viewer / full-control (video) modes are still stubbed — they need a
`VideoToolbox` decode path.

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

- **Viewer / full-control (video)** — decode the `Start`/`Frame` Annex-B events
  with `VideoToolbox` and render to a layer; `ExtenderSession.startPump` already
  surfaces the other events, and the video events flow through the same path.
- **Saved connections** — the C ABI now delivers `HostInfo`; a connection store
  (like Android's) could remember hosts with their OS icon.

The C ABI (`extender-mobile-ffi`) is at parity with `crates/android-jni`: the
`Snapshot` / `HostInfo` / `WindowList` events and the `ScanDeck` / `ListWindows` /
`FocusWindow` sends are all exposed.
