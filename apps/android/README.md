# ExtenderScreen — Android app (scaffold)

A single app that connects to a macOS `extender-host` and works in three modes:

- **Full control** — streams the host's screen (mirror mode), decodes with
  `MediaCodec`, renders to a `SurfaceView`, and forwards touch / keys as input.
- **Viewer** — same stream, read-only (no input sent).
- **Clicker** — a presentation remote: big ◀ ▶ buttons (plus first/last,
  start/end, blank) that send key events. Needs no video, so it's light on
  battery.

All three sit on the shared Rust core via the JNI bridge in
`crates/android-jni` (`com.universalsim.extender.ExtenderNative`).

> **Status: scaffold.** This was authored without an Android toolchain, so it
> has **not** been compiled or run. Treat the Kotlin/Gradle here as a starting
> point to open in Android Studio and finish — the `MediaCodec` decode path in
> particular wants on-device testing. The Rust JNI bridge *does* compile.

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

# From the repo root — outputs into the app's jniLibs:
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o apps/android/app/src/main/jniLibs \
  build -p extender-android-jni --release
```

Then open `apps/android/` in Android Studio and Run. (Point the connect screen
at your Mac's `ip:9000`; the Mac runs `cargo run --release -p extender-host`.)

## Layout

```
apps/android/
  settings.gradle.kts
  build.gradle.kts
  app/
    build.gradle.kts
    src/main/
      AndroidManifest.xml
      java/com/universalsim/extender/
        ExtenderNative.kt   # JNI declarations (must match android-jni symbols)
        ExtenderSession.kt  # Kotlin-friendly wrapper + event-pump thread
        MainActivity.kt     # Compose UI: mode switch, connect, clicker, control
        VideoDecoder.kt     # MediaCodec Annex-B → Surface (control/viewer)
        HidKeys.kt          # HID usage ids for the clicker
      res/values/strings.xml
```

## Notes / TODO

- **Frame feed:** `ExtenderNative.nativeNextEvent` advances the stream and
  returns a kind (0 Start, 1 Frame, -1 ended); read fields with the
  `nativeEvent*` accessors. On a `Start`, the `byte[]` is the Annex-B parameter
  sets (feed as `csd-0`); on a `Frame`, it's the Annex-B NAL units. Prepend the
  stored parameter sets on keyframes if your decoder needs it.
- **Local network permission:** Android 13+ doesn't gate LAN like iOS, but ensure
  the device is on the same Wi-Fi as the Mac.
- **Clicker keys:** see `HidKeys.kt` (Page Down = 0x4E = next slide, etc.). Send a
  down then an up for a tap.
- **No-stream clicker:** until the host's `CaptureMode::ControlOnly` (M6c) lands,
  clicker mode still connects with mirror mode and simply ignores the frames.
