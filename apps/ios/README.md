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
    ClickerView.swift                # Prev/Next + More options (First/Last/Blank/Start/End)
    ExtenderSession.swift            # Swift wrapper over the C FFI
    HidKeys.swift                    # HID usage ids for the clicker
    ScreenExtender-Bridging-Header.h # imports extender_ffi.h
```

The clicker connects in **control-only** mode (input only, no video). Viewer /
full-control (video) modes and the slide-preview / window-picker features are
**not** in this shell — the latter need downstream events the C ABI doesn't expose
yet (see "FFI parity" below).

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

## FFI parity (follow-up)

The Android app gained features that the C ABI (`extender-mobile-ffi`) doesn't yet
surface, so this iOS shell can't show them:

- **Slide preview** + **next-slide look-ahead** — needs the `Snapshot` event and a
  `ScanDeck` send.
- **Saved-connection OS icons** — needs the `HostInfo` event.
- **Window picker** — needs the `WindowList` event and `ListWindows` / `FocusWindow`
  sends.

Bringing the C ABI to parity with `crates/android-jni` would let the iOS app match
the Android feature set.
