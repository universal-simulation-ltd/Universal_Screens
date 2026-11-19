# Second screen (extend) — Windows setup

The **Second screen** mode uses the phone as an *extra* display (not a mirror): you
drag windows onto it like any monitor. Windows only treats something as a real
second monitor if a **virtual display driver** creates one, so that driver is a
one-time install on the PC. Everything else is built in:

- **App:** pick **Second screen** in the mode picker (sends the `VirtualDisplay`
  capture mode).
- **Host:** when a client connects in that mode, it streams the **first
  non-primary monitor** (the virtual one) with the same H.264 path as Mirror. If
  no second monitor exists it falls back to mirroring the primary.

## 1. Install a virtual display driver (one time)

Use any IddCx-based virtual display driver. A convenient open-source one:

- **Virtual Display Driver** (MIT) — https://github.com/itsmikethetech/Virtual-Display-Driver
  Follow its README to install (it ships a signed driver + installer, so no
  Windows test-mode needed). After install, a new monitor appears.

> Building Microsoft's IddCx *sample* yourself is the alternative, but it needs the
> WDK + test-signing + Windows test mode — heavier and more invasive.

## 2. Set it to extend

Windows **Settings → System → Display** → select the virtual monitor → **"Extend
these displays"** (not "Duplicate"). Set its **resolution** to match your phone's
screen for the best fit (e.g. a portrait or 16:9 mode). Arrange it where you like.

## 3. Use it

1. Run the host (`extender-host-windows`).
2. On the phone: connect → **Second screen**.
3. The phone shows the virtual monitor; drag windows onto it on the PC.

Pinch-zoom + pan work like Mirror. It's **view-only** (no input is forwarded) —
you arrange and interact with that screen from the PC. Encoding is software
(openh264), same trade-offs as Mirror.

## Notes / limitations

- The virtual monitor's **resolution is set by the driver**, not the app — pick a
  size close to the phone in Display Settings to avoid big letterbox bars.
- If you have a *real* second monitor plugged in, the host streams the **first
  non-primary** one — which may be that physical monitor rather than the virtual
  display. Unplug it or make the virtual display the one you want extended.
- True per-client display sizing (like the macOS `CGVirtualDisplay` host) isn't
  possible with a generic prebuilt driver; configure resolution in Windows.

## When the second screen disconnects (macOS host)

Until 2026-11-19 the Mac kept a session's virtual display after the device
left, to reuse it on reconnect. With the device gone, macOS still had the
display, so windows and the pointer could wander onto a screen nobody could
see, and the Mac could not be controlled properly (James, 2026-11-19).

Now `host::serve_session` removes the virtual display whenever its session
ends, however it ends: the device hung up, the stream failed, or **Disconnect**
in the window. Reconnecting creates a new one (and puts it back on the side
saved for that device).

Two things had to change for the host to notice a device leaving at all:

- **ScreenCaptureKit sends no frames while a display is idle**, so on a second
  screen nobody was using no write ever failed, and the session (and its
  display) outlived the client indefinitely. `serve_video` now ends the session
  as soon as the input thread's read ends (`gone`), which is immediate when the
  client closes the connection.
- A client that **vanishes** without closing (Wi-Fi off, lid shut) never ends
  that read. Video writes now time out after 10 s, and the socket is shut down
  before joining the input thread so the join can't hang. On an idle display
  nothing is written, so this case is still only noticed once something
  changes on it; TCP keepalive would close that gap.

The input loop reads and decodes separately (`protocol::decode_frame`), so an
`Input` it doesn't understand (from a newer client) is skipped rather than
ending the session.

⚠️ Built and reviewed on Windows only: `host-macos` cannot compile there and
`tests.yml` has no macOS job. Check it on the Mac.

## Connected now (Windows and macOS hosts)

Each host window lists its live sessions under **Connected now** (device
name, address, what it's doing, how long), each with **Disconnect**
(`crates/host-ui/src/sessions.rs`). Disconnect shuts the session's socket in
both directions; the host's normal end-of-session path then runs, as if the
device had hung up. ⚠️ On Windows a shutdown doesn't wake a `recv` blocked on
another thread until the peer closes too. Every Screens client closes when its
stream ends, so in practice it is immediate, but a peer that never reacts
holds the session until it does. The Linux host's lean window doesn't have
the list.

The **Show another computer here** panel has its own **Disconnect** for the
viewer it opened (it kills the client process).

## Which side the second screen sits on (macOS host, 0.4.8)

macOS places a new display wherever it remembers a display with the same
vendor / product / serial sitting, and every virtual display the shim creates
shares one serial (`0x0001`, `shim/virtual_display.m`). So a laptop to the
right of the Mac could come up on its left (2026-11-18). The host now moves
it with `CGConfigureDisplayOrigin` (`crates/host-macos/src/placement.rs`):
edge to edge with the main display, centred along that edge, for the login
session.

- **From the Mac:** Virtual displays → each display has *Sits: ◀ Left /
  Right ▶ / ▲ Above / ▼ Below*.
- **From the client:** `--side left|right|above|below`, the viewer panel's
  "this sits" choice, or an arrow key in the client window when not in
  control mode. These send `Input::PlaceDisplay { side }` (appended to
  `Input`; protocol stays v10).
- The side is remembered per connecting device name in
  `~/Library/Application Support/UniversalScreens/display-sides.txt`
  (`side<TAB>device`) and reapplied whenever that device's display is created.
- The input loop re-reads the display's bounds per event, so moving it
  mid-session never misplaces the pointer.

⚠️ A host older than 0.4.8 cannot decode `PlaceDisplay` and stops reading
that session's input, so clients send it only when the user asks for a side.
Windows and Linux hosts ignore it for now.

⚠️ The shared serial also means a second host process on the same Mac cannot
create a virtual display while another holds one ("CGVirtualDisplay rejected
the requested size"). A per-device serial would fix that, and would let macOS
remember each device's arrangement itself; not done.
