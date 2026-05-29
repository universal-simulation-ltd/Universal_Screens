# M2 — Input round-trip (design)

**Status:** proposed (awaiting approval). **Prereq:** M1 complete ✅ (capture → encode → TCP → decode → render, verified on-screen).

## Goal

Let the **client** drive the **host**: capture mouse + keyboard in the client window, send them upstream, and inject them into macOS on the host. This proves the *reverse* data path and synthetic-input injection in isolation — the last big unknown before M3.

This is the input analogue of M1: M1 de-risked streaming, M2 de-risks input, and **M3** (virtual display via `CGVirtualDisplay`) ties them together into a real second screen. Like M3-streaming, M2 input does **not** depend on the virtual display existing yet.

### In scope
- Mouse: absolute move, left/right/middle press & release, scroll wheel.
- Keyboard: key down / key up.
- A reverse channel on the existing TCP connection.
- Host-side injection on macOS via CoreGraphics events.

### Out of scope (deferred)
- Targeting a *virtual* display — M2 injects into the **main display** (which is what we currently capture, so input is self-visible; see Verification). M3 retargets.
- Windows/Linux host injection (Mac host first, per project direction).
- Pointer-lock / relative-mouse mode, multi-monitor coordinate routing, clipboard, drag-and-drop files.

## Architecture

The TCP socket is already bidirectional; today only host→client is used. M2 adds client→host using a second handle on the same socket via `TcpStream::try_clone()`.

```
        client                                   host
  ┌────────────────┐   video  (StreamStart/Frame) ┌────────────────┐
  │ winit window   │ ◄──────────────────────────── │ capture+encode │
  │  • render      │                                │  (writer)      │
  │  • capture in  │   input  (Input messages)      │                │
  │    events  ────┼──────────────────────────────►│ reader+inject  │
  └────────────────┘                                └────────────────┘
```

- **Client:** the winit event handler (main thread) already sees every input event. It forwards them over an `mpsc` channel to a small **input-writer thread** that owns a `try_clone()`d `TcpStream` and writes `Input` messages. (Keeps the render loop non-blocking.)
- **Host:** `serve()` spawns an **input-reader thread** on a `try_clone()`d `TcpStream` that reads `Input` messages and injects them, alongside the existing video writer. One socket, two directions, two threads per side.

## Protocol changes (`extender-protocol`)

Add an upstream message type. Generalise the existing length-prefixed postcard framing so both directions share it:

```rust
// Refactor write_message/read_message into generics (back-compat for the
// existing host→client Message), then add the upstream type:
pub fn write_framed<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()>;
pub fn read_framed<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<T>;

pub enum Input {
    MouseMove { x: f32, y: f32 },              // normalized [0,1] within the streamed frame
    MouseButton { button: Button, pressed: bool },
    Scroll { dx: f32, dy: f32 },               // wheel deltas (lines)
    Key { code: u32, pressed: bool },          // see "Key encoding" below
}

pub enum Button { Left, Right, Middle }
```

Normalized mouse coords make input resolution-independent: the client sends where the cursor is *within the displayed frame* (0..1), and the host maps that to its display pixels — correct regardless of window size vs. capture size.

## Client capture (`extender-client`)

In the existing `window_event` handler, match and translate:

| winit event | → `Input` |
|---|---|
| `CursorMoved { position }` | `MouseMove { x: position.x / win_w, y: position.y / win_h }` |
| `MouseInput { state, button }` | `MouseButton { button, pressed: state == Pressed }` |
| `MouseWheel { delta }` | `Scroll { dx, dy }` (normalize `LineDelta` vs `PixelDelta`) |
| `KeyboardInput { event: KeyEvent { physical_key, state, .. } }` | `Key { code, pressed }` |

Send each over the `mpsc` to the input-writer thread. (Window size comes from `WindowEvent::Resized` / `window.inner_size()`.)

## Host injection (`extender-host`)

Use the **`core-graphics`** crate (v0.23.2 — already in `Cargo.lock` transitively via winit/wgpu, so no new download; add it as a direct host dep):

```rust
use core_graphics::event::{CGEvent, CGEventType, CGMouseButton, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)?;
// move: map normalized (x,y) → display pixels
let p = CGPoint::new(x as f64 * display_w, y as f64 * display_h);
CGEvent::new_mouse_event(source.clone(), CGEventType::MouseMoved, p, CGMouseButton::Left)?
    .post(CGEventTapLocation::HID);
```

Verified APIs (read from the crate source): `CGEventSource::new(state_id) -> Result`, `CGEvent::new_mouse_event(source, CGEventType, CGPoint, CGMouseButton) -> Result`, `new_keyboard_event(source, keycode, keydown)`, `new_scroll_event(...)`, `.post(CGEventTapLocation)`.

**Coordinate mapping:** keep the last-known cursor position on the host so button/scroll events fire at the right spot; or post a `MouseMoved` immediately before clicks. Map normalized → the captured display's pixel bounds (we already know them from `serve()`).

### Permission: Accessibility
Posting synthetic events requires the host process to be trusted for **Accessibility** (System Settings → Privacy & Security → **Accessibility**) — the input-side analogue of the Screen Recording grant capture needed. When running `cargo run -p extender-host`, the controlling app (your terminal) must be added. Untrusted processes have their posted events silently dropped, so the injection probe (M2b) will check/announce this.

## Key encoding (open decision)

CoreGraphics keyboard events need a macOS **virtual keycode** (`CGKeyCode`, u16). winit gives a platform-neutral `PhysicalKey::Code(KeyCode)`. We must choose what crosses the wire:

- **(A, recommended) Neutral code + host-side map.** Client sends a platform-neutral `code` (winit `KeyCode` mapped to a stable u32, e.g. its USB-HID usage). Host maps → macOS virtual keycode via a lookup table. Keeps the protocol portable (a Windows/Linux client later just sends the same neutral codes). Cost: one mapping table on the host.
- **(B) Native scancode passthrough.** Client sends the OS scancode; host uses it directly. Simplest for a mac↔mac dev loop, but couples the wire to the sender's OS — re-work when clients diversify.

Recommendation: **A**, but build it in the keyboard phase (M2d) so the mouse loop lands first. The table is mechanical (~60 common keys to start).

## Crate decision

**Use `core-graphics` for injection** (already in the tree, maintained, safe ergonomic `CGEvent` API). Alternative considered: hand-write `extern "C"` bindings to `CGEventCreateMouseEvent`/`CGEventPost` to stay within the apple-cf "doom-fish" family (apple-cf does **not** expose CGEvent). Rejected for M2: more unsafe FFI for a security-sensitive path, with no upside since core-graphics is already a dependency. Noted that this adds the `core-foundation-rs` family alongside apple-cf — acceptable; both are battle-tested.

## Phased increments (each self-verifiable)

Mirrors the M1 discipline — small, independently verifiable steps; pause at the permission boundary.

- **M2a — protocol.** Add `Input`/`Button` + generic `write_framed`/`read_framed`; keep `Message` working. Pure Rust, unit-tested (round-trip), no macOS. *Verify: `cargo test`.*
- **M2b — injection probe.** `crates/host/examples/inject_probe.rs`: with no networking, post a square of mouse moves + a click via `CGEvent`, and a couple of key taps. Announces whether Accessibility is granted. *Verify: cursor visibly moves / a keystroke lands; self-evident.* This de-risks injection + permission before any wiring.
- **M2c — mouse round-trip.** Reverse channel (`try_clone` both sides) + client mouse capture → host mouse injection. *Verify: move/click in the client window → the host's real cursor moves correspondingly (and, since we capture the main display, you see it move in the client too — a built-in feedback check).*
- **M2d — keyboard + scroll.** Key mapping table (decision A) + scroll. *Verify: type into a host app via the client window; scroll a host window.*

## Verification & a heads-up

Because M2 injects into the **main display** (what we capture), testing M2c/M2d **moves your real cursor and types into your real apps** from the client window — expected, and a convenient self-check, but mildly disorienting. Keep a hand on the physical keyboard/trackpad to regain control. **M3's virtual display removes this** — input will go to the separate virtual screen instead of your main one.

## Risks / open questions

1. **Accessibility trust** for the dev process (terminal). Probe announces it; document the grant steps.
2. **Aspect-ratio skew** if the client window aspect ≠ host display aspect (the frame is stretched to the window, so normalized coords stay consistent with what's shown — acceptable for M2; revisit with letterboxing in M4).
3. **Key mapping coverage** — start with a common-key table; expand as needed.
4. **Event coalescing / rate** — high-frequency `MouseMove` could flood the socket; throttle to frame cadence if needed (likely fine on LAN/localhost for M2).
5. **`extender-core`** is the natural home for the shared session/input-mapping logic that's currently empty — M2 is where it gets its first real content.

## Estimated surface

`extender-protocol` (+`Input`, generic framing, tests) · `extender-client` (capture + input-writer thread) · `extender-host` (+`core-graphics` dep, input-reader thread, injection + key map) · one new example (`inject_probe`). No changes to the M1 video path.
