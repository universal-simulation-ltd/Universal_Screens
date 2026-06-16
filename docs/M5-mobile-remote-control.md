# M5 — Mobile clients & "control my actual screen"

**Status:** design / not started. **Prereq:** M1 (streaming) ✅, M2 (input) ✅,
M3 (virtual display) ✅, M4 (configurable resolution) ✅.

## Goal

Two related asks, treated as one roadmap:

1. **Drive the host from a phone** — an iPhone / Android app that connects, shows
   the stream, and sends touch + keyboard back. Today the only client is the
   desktop `extender-client` (Rust + `winit` + `wgpu` + software `openh264`).
2. **Control the *real* desktop, not just a virtual second screen** — a host
   capture mode that mirrors the **primary** (or any physical) display and routes
   input *to it*, i.e. VNC/RDP-style remote control rather than "extend".

These are independent levers (each useful alone) but together they're the
"control my Mac/PC from my phone" product. **Controlling a Windows machine is a
separate, larger track** — there is no Windows *host* yet (see
[Non-goals](#non-goals)); everything below targets the existing macOS host.

## Where the current design already helps

The wire protocol is the asset here, and it's already most of the way there:

- **Transport is portable.** `crates/protocol` is plain TCP + length-prefixed
  `postcard`, no platform code. A phone app needs only a socket, the same
  `ClientHello` handshake, and the same `Message` / `Input` framing.
- **Absolute pointer input already works end-to-end.** The host's `inject()`
  handles `Input::MouseMove { x, y }` (normalized `[0,1]`) by mapping into the
  captured display's global bounds — even though the *desktop* client only ever
  sends `MouseMoveRelative` in pointer-lock. So a **tap on a phone maps straight
  onto the existing `MouseMove` + `MouseButton` events with no protocol change.**
  This is the single biggest reason a basic touch client is cheap.
- **Codec is hardware-friendly.** The host emits AVCC H.264 with SPS/PPS in
  `StreamStart`; the protocol already carries an HEVC codec tag. Mobile SoCs
  decode both in hardware.

What does *not* carry over for free:

- **Software decode.** The client uses `openh264` (CPU). On a phone that's
  battery- and thermal-hostile — mobile wants the platform hardware decoder.
- **Keyboard model.** `Input::Key { code, pressed }` is a **physical USB-HID
  usage id** (client maps `winit` `KeyCode`→HID; host maps HID→macOS keycode).
  Soft keyboards emit *characters / IME text*, not scancodes — there's no clean
  physical key for "é" typed on an iOS keyboard. This needs a new text path
  (see [Protocol additions](#protocol-additions-m5a)).
- **Capture target.** The host captures the **virtual** display it created
  (`display_id() == virtual_id`). "Control the real screen" means selecting a
  physical display and *not* creating a virtual one.
- **Security / reachability.** Streaming is **plaintext TCP on the LAN** with no
  auth. Phone-from-anywhere needs encryption, pairing, and off-LAN reachability.

## Architecture changes

```
            ┌─────────────────────────── host (macOS) ───────────────────────────┐
  phone ───►│ ClientHello ─► capture mode select:                                 │
 (client)   │                 • virtual display  (M3, "extend")  ── existing      │
            │                 • mirror primary    (M5b, "control") ── new         │
            │  H.264 ◄──────  ScreenCaptureKit + VideoToolbox (unchanged)         │
            │  Input ───────► CoreGraphics inject, bounds = captured display       │
            └─────────────────────────────────────────────────────────────────────┘
```

- **Protocol** (`crates/protocol`): add touch/gesture/text `Input` variants and a
  capture-mode hint in `ClientHello`; bump `PROTOCOL_VERSION`. Additive only.
- **Host** (`crates/host`): a "mirror primary" capture path that picks a physical
  `SCDisplay` and sets input bounds to that display, gated by the client's
  requested mode. The encode/stream loop is unchanged.
- **Clients**: the desktop client stays. Add **one or two mobile app shells**
  (iOS, Android) that reuse a shared Rust core for protocol + decode-feed, with
  platform-native decode, render, and a touch/keyboard UI.

## Sub-increments

- **M5a — protocol: touch, gestures, text.** Add the `Input` variants below and a
  `CaptureMode` to `ClientHello`; bump `PROTOCOL_VERSION` to 2; keep the host
  tolerant of older clients (it already warns, not rejects, on version skew).
  *Verify:* desktop client still works against the new host (no behaviour change);
  round-trip tests for the new variants (mirror the existing
  `input_messages_round_trip` test).

- **M5b — host: "mirror primary display" mode.** When the client requests
  `CaptureMode::MirrorPrimary`, skip `extender_vdisplay_create`, select the main
  display from `SCShareableContent`, capture it at its native size, and set the
  injection `bounds` to that display. Everything downstream (encoder, stream
  loop, `inject`) is unchanged.
  *Verify:* run the existing desktop client with the new mode flag against a Mac;
  the client shows (and can drive) the **real** desktop. Confirm no
  same-machine feedback loop when host and client are different machines (the
  [WINDOWS-CLIENT](WINDOWS-CLIENT.md) note already documents this is loop-free
  across machines).

- **M5c — shared mobile core crate.** Factor the protocol-drive + frame-feed
  logic (currently inline in `crates/client/src/main.rs`) into a reusable piece
  (extend the near-empty `crates/core`) with a C/FFI or UniFFI surface: connect,
  pump frames out as decoded/encoded buffers, push `Input` in. No UI, no winit.
  *Verify:* a tiny host-side harness drives a connect → receive-N-frames →
  send-input cycle through the FFI boundary.

- **M5d — iOS app.** Thin SwiftUI/Metal shell over the M5c core: `VTDecompression`
  (VideoToolbox) hardware H.264 decode, `MTKView` render, gesture recognizers →
  `Input`, and a text field driving the new text-input variant. Discovery via a
  typed-in `host:port` first; Bonjour later.
  *Verify:* on-device, drive a Mac on the same Wi-Fi.

- **M5e — Android app.** Same shape: `MediaCodec` hardware decode to a
  `SurfaceView`, `GestureDetector` → `Input`, soft-keyboard text → text variant.
  Kotlin shell over the same M5c core via JNI/UniFFI.
  *Verify:* on-device against a Mac on the same Wi-Fi.

- **M5f — security & off-LAN (optional, larger).** TLS for the stream, a pairing
  step (PIN/QR) so any LAN peer can't connect, and an off-LAN path (relay or
  NAT traversal, or "bring your own VPN/Tailscale" as the cheap first answer).
  *Verify:* connect over TLS with pairing on the LAN; document the VPN path for
  remote use.

M5a→M5b are small and unlock the desktop client immediately. M5c→M5e are the real
mobile lift. M5f is its own project; ship LAN-only + a VPN recommendation first.

## Protocol additions (M5a)

Concrete sketch — additive, so existing variants keep their `postcard`
discriminants and old messages still decode. New variants go at the **end** of
the enum; bump `PROTOCOL_VERSION` so the host can branch on capability.

```rust
/// How the client wants the host to source the stream. Defaults to the existing
/// "extend" behaviour so older clients (which never send this) are unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CaptureMode {
    /// Create + capture a virtual display sized to the client (M3 — "extend").
    #[default]
    VirtualDisplay,
    /// Capture the host's existing primary display (M5b — "control").
    MirrorPrimary,
}

// Added to ClientHello (kept backward-compatible: a v1 host ignores the field
// because v1 decodes the struct it knows; introduce as a v2 hello, see note).
pub struct ClientHello {
    pub protocol_version: u32,
    pub width: u32,
    pub height: u32,
    pub capture_mode: CaptureMode, // NEW in v2
}

// New Input variants (appended after Key so existing indices are stable):
pub enum Input {
    // ... existing: MouseMove, MouseMoveRelative, MouseButton, Scroll, Key ...

    /// A touch/pen contact changed. `id` distinguishes simultaneous fingers;
    /// `x`/`y` are normalized to the frame like MouseMove. The host can treat a
    /// single-finger Down/Up as a left click at (x, y) — reusing existing inject.
    Touch { id: u32, phase: TouchPhase, x: f32, y: f32 },

    /// A recognized high-level gesture, pre-classified on the client where the
    /// touch history lives (cheaper and more reliable than re-deriving on the
    /// host). Pinch maps to zoom; two-finger pan can also be sent as Scroll.
    Gesture(Gesture),

    /// Committed Unicode text from a soft keyboard / IME. Solves the "no physical
    /// scancode for soft-keyboard characters" gap; the host synthesizes the
    /// keystrokes (CGEvent supports posting a Unicode string for a key event).
    Text { text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchPhase { Began, Moved, Ended, Cancelled }

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Gesture {
    /// Pinch scale factor relative to gesture start (1.0 = no change).
    Pinch { scale: f32 },
    /// Secondary-click request at a normalized point (e.g. long-press).
    SecondaryClick { x: f32, y: f32 },
}
```

Notes on accuracy / compatibility:

- **`ClientHello` is a `struct`, not an enum**, so adding a field is *not* a free
  `postcard`-compatible change — a v1 host reading a v2 hello would mis-decode the
  trailing bytes. Cleanest: gate on `protocol_version` and read either the v1 or
  v2 layout, or wrap the hello in an enum going forward. The host already *warns*
  (doesn't reject) on version mismatch, which gives room to negotiate.
- **`Input` is an enum**; appending variants is `postcard`-safe (discriminants are
  assigned by declaration order). A v1 host simply never receives the new ones.
- **Text vs. Key:** keep `Input::Key` for physical keys (desktop client,
  hardware keyboards, modifiers, arrows, shortcuts) and add `Input::Text` for
  soft-keyboard/IME committed strings. They coexist; don't try to force soft
  keyboards through the HID map.

## Input mapping detail (host side, M5b)

`inject()` needs no change for touch-as-mouse: `Touch{Began}`→`MouseMove` +
`MouseButton{Left,true}`, `Touch{Moved}`→`MouseMove`, `Touch{Ended}`→
`MouseButton{Left,false}`, all at the touch's normalized `(x, y)` — the host
already maps normalized→`bounds`. For `MirrorPrimary`, `bounds` becomes the
chosen physical display's `CGDisplay::bounds()` instead of the virtual display's.
`Input::Text` is the one genuinely new injection path (post a Unicode keyboard
event via `CGEventKeyboardSetUnicodeString`).

## Permissions

- **Host:** unchanged from M2/M3 — **Screen Recording** (capture, incl. the real
  display) + **Accessibility** (injection). Capturing the *primary* display needs
  the same Screen Recording grant already required for the virtual one.
- **iOS/Android client:** standard app sandbox + local-network permission (iOS 14+
  prompts for local network access on first LAN connection).

## Non-goals (separate tracks)

- **Windows host / "control my Windows PC".** There is no Windows host today; it's
  a from-scratch effort (DXGI Desktop Duplication capture, Media Foundation /
  NVENC encode, `SendInput` injection) and is independent of everything above.
  The current Windows story is *Windows-as-client* only (see
  [WINDOWS-CLIENT.md](WINDOWS-CLIENT.md)).
- **True HiDPI/Retina virtual display** — still deferred ([M4-hidpi-deferred.md](M4-hidpi-deferred.md)).
- **Audio** — not in scope.

## Open questions / risks

1. **Mobile decode path.** VideoToolbox (iOS) / MediaCodec (Android) want
   Annex-B or AVCC with explicit parameter sets; the host already provides
   SPS/PPS in `StreamStart`, so this should map cleanly — confirm each platform's
   expected NAL framing during M5d/M5e.
2. **Reusing Rust on mobile vs. native shells.** `winit` 0.30 + `wgpu` 29 *do*
   target iOS/Android, so a near-pure-Rust app is possible; but hardware decode +
   IME/soft-keyboard + gesture recognizers are far smoother through native APIs.
   Recommendation: **shared Rust core (M5c) + thin native UI**, not winit on phone.
3. **Latency under touch.** Pointer-lock relative motion (desktop) hides round-trip
   latency; absolute touch makes it visible (finger vs. cursor lag). May want a
   client-side predicted cursor.
4. **Off-LAN security (M5f).** Plaintext TCP is fine on a trusted LAN; exposing it
   to the internet without TLS + pairing would be unsafe. Ship LAN-only first and
   document a VPN as the interim remote path.

## Surface

- `crates/protocol/src/lib.rs` — new `Input` variants, `CaptureMode`, versioned
  `ClientHello`, round-trip tests (M5a).
- `crates/host/src/main.rs` — capture-mode branch in `serve()`/`main()`; primary
  display selection; `Input::Text` injection (M5b).
- `crates/core/src/lib.rs` — promote shared connect/decode-feed logic for FFI
  reuse (M5c).
- `apps/ios/`, `apps/android/` (new) — native shells over the core (M5d/M5e).
- `crates/client/src/main.rs` — add a `--mirror` flag to exercise M5b from the
  desktop before the mobile apps exist.
