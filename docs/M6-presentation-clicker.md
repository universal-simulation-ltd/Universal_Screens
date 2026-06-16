# M6 — Presentation clicker

**Status:** working end to end on Windows + Android — keymap + FFI ✅, Android app
built & running ✅, true no-stream `ControlOnly` host ✅ (Windows), slide preview ✅,
saved connections ✅. **Prereq:** M2 (input) ✅, M5a (protocol) ✅, M5c (core + C ABI) ✅.

## Goal

Use a phone (or the desktop client) as a presentation remote: next / previous
slide, jump to first / last, start and end the slideshow, blank the screen. The
nice version is a *presenter remote with live preview* — the screen is already
streamed to the client, so you see the current slide in your hand while you click.

## Why this is cheap

A clicker is **just input events**, and the whole input path already exists:
`Input::Key { code, pressed }` carries a USB-HID usage id that the host maps to a
macOS keycode (`hid_to_macos`) and injects via CoreGraphics. Physical clickers
just send keystrokes — so this is a keymap + a send-path, not new machinery.

What apps expect:

| Action | Keynote | PowerPoint | Google Slides | PDF |
|---|---|---|---|---|
| Next | →, PageDown | →, PageDown, N | →, PageDown | →, PageDown |
| Previous | ←, PageUp | ←, PageUp, P | ←, PageUp | ←, PageUp |
| First / last | Home / End | Home / End | Home / End | Home / End |
| Start | ⌘⌥P / Play | F5 | ⌘+Enter | — |
| End | Esc | Esc | Esc | Esc |
| Blank | — | B (black), W (white) | . (black) | — |

Arrows and Esc were already mapped; `→`/`←` alone already drive every app above.

## Sub-increments

- **M6a — keymap + FFI key send.** ✅ Done.
  - Added to both keymaps (`crates/client` `key_to_hid`, `crates/host`
    `hid_to_macos`): **PageUp/PageDown, Home, End, Insert, Delete, F1–F12**.
    (Arrows, Esc, letters/`.` for blanking were already present.)
  - Added `extender_send_key(session, hid_code, pressed)` to
    `extender-mobile-ffi`, with the common clicker keycodes `#define`d in
    `include/extender_ffi.h` (`EXTENDER_KEY_PAGE_DOWN`, …). A tap = a `down` then
    an `up`.
  - *Verified:* client builds; an FFI test confirms a Page-Down keypress reaches
    a loopback host as `Input::Key { code: 0x4E, pressed: true }`. (Host keymap is
    macOS-only — written against the standard HID→macOS table, needs a Mac to
    compile/smoke-test.)

- **M6b — clicker UI (mobile).** ✅ Built and running on Android (`apps/android/`):
  the `ClickerScreen` Compose view has ◀ Prev / Next ▶, First/Last, Blank (PPT) /
  Blank (.), and Start(F5)/End(Esc) buttons, each calling `tapKey` (a key down
  then up) over the JNI bridge. Build with `gradlew assembleDebug` (the repo now
  ships the Gradle wrapper + `gradle.properties`; see `apps/android/README.md`);
  the iOS equivalent follows with the iOS shell.

- **M6c — "control-only" (no-stream) mode.** ✅ Done via the **Windows clicker
  host** (`extender-host-windows`): it has no capture/encode at all — bind
  `0.0.0.0:9000`, read the `ClientHello`, then receive `Input` and inject via
  `SendInput` (`hid_to_windows_vk` mirrors `hid_to_macos`). So a phone clicks a
  Windows laptop with **no Mac involved**. `CaptureMode::ControlOnly` is in the
  protocol and the Android clicker requests it. (The macOS host still treats
  `ControlOnly` like `MirrorPrimary` and streams anyway — unchanged.)

- **M6d — slide preview (snapshots).** ✅ Slides are static, so instead of a video
  stream the Windows host pushes a JPEG still (`Message::Snapshot`, protocol v3):
  it captures the primary display (GDI `BitBlt`), downscales + encodes (~1000px,
  q70), and sends one ~350 ms after each slide-changing key plus one on connect,
  on a debounced thread. The Android clicker shows it atop the buttons. Far
  lighter than H.264 and keeps the no-video ethos.

- **M6e — saved connections.** ✅ The host sends `Message::HostInfo{os,name}`
  (protocol v4) right after the handshake; the app remembers each connected host
  (`ConnectionStore`, SharedPreferences) and shows a quick-connect list with a
  device icon (Windows/Mac/Linux), the machine name, and one-tap reconnect in the
  last-used mode, with hide / delete.

## Keycode reference (added in M6a)

USB-HID usage id → macOS virtual keycode, for the keys this milestone added:

| Key | HID | macOS | Key | HID | macOS |
|---|---|---|---|---|---|
| PageUp | 0x4B | 0x74 | F1 | 0x3A | 0x7A |
| PageDown | 0x4E | 0x79 | F2 | 0x3B | 0x78 |
| Home | 0x4A | 0x73 | F3 | 0x3C | 0x63 |
| End | 0x4D | 0x77 | F4 | 0x3D | 0x76 |
| Insert | 0x49 | 0x72 | F5 | 0x3E | 0x60 |
| Delete (fwd) | 0x4C | 0x75 | … | … | … |

(F6–F12 follow the standard table; see `hid_to_macos` / `key_to_hid`.)

## Open questions

1. **Blank-screen portability.** There's no universal "blank" key — PowerPoint
   uses `B`/`W`, Keynote/Slides use `.`. The clicker UI can expose a "blank"
   button that sends `.` (broadest) or be app-aware later.
2. **Start-slideshow portability.** `F5` is PowerPoint-specific; Keynote/Slides
   differ. Either expose per-app presets or leave "start" to the user and focus
   the clicker on navigation (the common, portable case).
3. **Latency.** Clicking tolerates latency far better than pointer control, so
   even an off-LAN/relayed link (M5f) is fine for this use.

## Surface

- `crates/client/src/main.rs` — `key_to_hid`: PageUp/Down, Home/End, Insert/Delete, F1–F12 (done).
- `crates/host/src/main.rs` — `hid_to_macos`: matching entries (done).
- `crates/host-windows/src/main.rs` — `hid_to_windows_vk` (mirrors `hid_to_macos`) + `SendInput` injection; `snapshot.rs` — GDI screen capture → JPEG (M6c/M6d).
- `crates/mobile-ffi/src/lib.rs` + `include/extender_ffi.h` — `extender_send_key` + clicker keycodes (done).
- `crates/protocol/src/lib.rs` — `CaptureMode::ControlOnly` (M6c), `Message::Snapshot` (M6d), `Message::HostInfo` (M6e).
- `apps/android/.../MainActivity.kt` — clicker control row, slide preview, saved-connection list (M6b/d/e); `ConnectionStore.kt` — persisted hosts.
