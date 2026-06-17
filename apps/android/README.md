# Screen Extender — Android app

A single app that connects to a `extender-host` and works in three modes:

- **Full control** — streams the host's screen (mirror mode), decodes with
  `MediaCodec`, renders to a `SurfaceView`, and forwards touch / keys as input.
- **Viewer** — same stream, read-only (no input sent).
- **Clicker** — a presentation remote: big ◀ ▶ buttons (plus first/last,
  start/end, blank) that send key events. Needs no video, so it's light on
  battery — see the clicker features below.

All three sit on the shared Rust core via the JNI bridge in
`crates/android-jni` (`com.universalsim.extender.ExtenderNative`).

> **Status: builds and runs.** Build the native lib with `cargo-ndk` (below),
> then `./gradlew assembleDebug` (the Gradle wrapper is included). The clicker
> path is exercised against the Windows host; the streaming (video) modes still
> want more on-device testing.

## Hosts

- **`extender-host-windows`** (Windows) — input-only clicker host: injects keys
  via `SendInput`, streams no video, and additionally pushes still **slide
  previews** and the **window list** for the clicker. Run:
  `cargo run -p extender-host-windows [-- 0.0.0.0:PORT]` (default port 9000).
- **`extender-host`** (macOS) — the streaming host for the mirror/viewer modes.

## Architecture

```
  Kotlin (Compose UI)          ExtenderNative (JNI)        Rust
  ┌─────────────────┐  external fun  ┌──────────────┐  extender-core
  │ mode switch      │ ─────────────► │ libextender_ │ ─► Session
  │ connect screen   │                │ mobile.so    │     • next_event → frames
  │ control surface  │ ◄───────────── │ (android-jni)│     • send Input
  │ clicker buttons  │   byte[] / int └──────────────┘
  └─────────────────┘
```

## Building the native library

The JNI bridge compiles to `libextender_mobile.so` per Android ABI via
[`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk):

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk

# From the repo root — outputs into the app's jniLibs (arm64 shown; add -t for more):
cargo ndk -t arm64-v8a \
  -o apps/android/app/src/main/jniLibs \
  build -p extender-android-jni --release
```

Then build the APK and install it:

```bash
cd apps/android
./gradlew assembleDebug        # or open in Android Studio and Run
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

In the app, enter the host's `ip:port` and pick a mode. For the Windows clicker
host, run `cargo run -p extender-host-windows`. To test over a USB cable (handy
when the LAN blocks peer-to-peer, e.g. a guest network), forward the port and
connect to localhost:

```bash
adb reverse tcp:9000 tcp:9000   # then connect the app to 127.0.0.1:9000
```

## Clicker features

- **Slide preview** — the current slide (a JPEG still from the host) is shown at
  the top, refreshed after each tap.
- **Next-slide look-ahead** — tap **Scan deck** (keep the document focused) and
  the host pages through it once, caching each page; the previous / next slides
  then appear above the ◀ / ▶ buttons.
- **Saved connections** — successful hosts are remembered with their OS icon and
  machine name for one-tap reconnect; hide or delete them.
- **Window picker** — **Focus window ▾** lists the host's open windows; pick one
  to bring it to the foreground (and start its slideshow).

## Layout

```
apps/android/
  gradlew, gradle/                 # Gradle wrapper (8.7)
  gradle.properties                # android.useAndroidX=true
  settings.gradle.kts
  build.gradle.kts
  app/
    build.gradle.kts
    src/main/
      AndroidManifest.xml
      java/com/universalsim/extender/
        ExtenderNative.kt    # JNI declarations (must match android-jni symbols)
        ExtenderSession.kt   # Kotlin-friendly wrapper + event-pump thread
        MainActivity.kt      # Compose UI: mode switch, connect, clicker, control
        VideoDecoder.kt      # MediaCodec Annex-B → Surface (control/viewer)
        HidKeys.kt           # HID usage ids for the clicker
        ConnectionStore.kt   # saved-connection persistence
      res/drawable/ic_screenextender.xml
      res/values/strings.xml
```

## Notes

- **Frame feed:** `ExtenderNative.nativeNextEvent` advances the stream and
  returns a kind (0 Start, 1 Frame, 2 Snapshot, 3 HostInfo, 4 WindowList, -1
  ended); read fields with the `nativeEvent*` accessors. On a `Start`, the
  `byte[]` is the Annex-B parameter sets (feed as `csd-0`); on a `Frame`, it's the
  Annex-B NAL units; on a `Snapshot` it's a JPEG.
- **Local network:** ensure the device and host are on the same network (or use
  `adb reverse` over USB). Android 13+ doesn't gate LAN like iOS.
- **Clicker keys:** see `HidKeys.kt` (Page Down = 0x4E = next slide, etc.). A tap
  is a key down then up.
```
