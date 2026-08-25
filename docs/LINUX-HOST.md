# Linux host — feasibility and scope

**Status: Stages 1 and 2 are built** (`crates/host-linux`, 2026-08-25). The
backlog said a Linux host "should be scoped as one before anyone starts" — this
is that scope, kept as the plan for the rest and as the record of why the Linux
host looks different from the other two.

The clicker now has **slide previews, the deck scan and the window picker** on
X11. What is left of Stage 2 is the H.264 *mirror* ("2b" below); Wayland capture
is Stage 3 and is still a different job entirely.

**Verdict: feasible, and the shared spine is already portable — but it is not one
port.** Linux has two display stacks with different capabilities, and the app's
five modes do *not* all survive the crossing. The recommendation is a staged
Clicker-first host, not a "build the Linux host" project.

## 1. What is already portable (measured, not assumed)

`cargo check --target x86_64-unknown-linux-gnu` on the Windows dev box:

| Crate | Result |
|---|---|
| `extender-protocol` | ✅ clean |
| `extender-transport` (Noise) | ✅ clean |
| `extender-core` | ✅ clean |
| `extender-discovery` (beacon + mDNS) | ✅ clean |
| `extender-client` | ❌ toolchain only — see below |
| `extender-web-bridge` | ❌ toolchain only — see below |

There is **not one `cfg(target_os)` / `cfg(windows)` / `cfg(unix)`** in any of
`protocol`, `transport`, `discovery`, `core`, `web-bridge` or `client`. The wire
format, the PIN-keyed Noise handshake, LAN discovery, the cloud rendezvous and
the browser bridge all come to Linux for free.

Both failures are **cross-compiling from Windows**, not portability:

- `extender-client` — `openh264-sys2` wants `x86_64-linux-gnu-g++`. NASM
  assembled the x86 source fine; only the C++ compiler is missing. Natively on
  Linux this is `build-essential` + `nasm`, the same NASM requirement the Windows
  host already documents.
- `extender-web-bridge` — `openssl-sys` can't find a Linux libssl. ✅ **Fixed
  2026-08-25**, and the crate now builds and tests on Linux; see below.

✅ **`crates/web-bridge/Cargo.toml` was wrong about Linux — now corrected.** Its
comment used to read "macOS uses Security.framework, Windows uses SChannel —
**no OpenSSL to vendor**". On Linux, `native-tls` *is* OpenSSL: the build needed
`libssl-dev` and the binary then carried a distro-specific runtime dependency —
poison for a download-and-run AppImage. The `tungstenite` feature is now
`rustls-tls-native-roots`, and `native-tls`, `openssl`, `openssl-sys`,
`openssl-macros` and `vcpkg` are gone from `Cargo.lock` entirely. `ldd` on the
Linux binary shows `libgcc_s`, `libc` and the loader, nothing else.

⚠️ **The one-line feature swap would have shipped a panic.** tungstenite depends
on rustls with `default-features = false`, so **neither `ring` nor `aws-lc-rs` is
compiled in by its features** — and `ClientConfig::builder()`, deep inside
tungstenite's `wss://` path, *panics* when no crypto provider is installed. There
is no `Result` to handle and nothing fails until the first real dial at the cloud
rendezvous, on a user's machine. `web-bridge` therefore depends on `rustls`
directly to supply `ring`, and installs it as the process default explicitly
rather than relying on rustls' pick-from-crate-features fall-back, which panics
again the day a second provider enters the tree.
`tests/wss_crypto_provider.rs` guards it by dialling a plain TCP listener as
`wss://`, so the handshake reaches the panic site for real.

`-native-roots` and not `-webpki-roots`: bundled roots ignore the machine's trust
store, so a corporate network that re-signs TLS could not reach the rendezvous.
On Linux reading the platform store is `openssl-probe` looking up
`/etc/ssl/certs` — a path lookup in pure Rust, not a link against OpenSSL.

## 2. What genuinely has to be written

The Windows host is 3,429 lines. Subtract the parts that are already OS-agnostic
in substance — the 1,598-line egui GUI, QR rendering, the discovery adapter, and
the openh264 encode and framing in `stream.rs` — and the actual platform surface
is **about 880 lines across six primitives**:

| Primitive | Windows | Lines |
|---|---|---|
| Input injection + HID→keycode map | `SendInput` (`main.rs` 448–767) | ~320 |
| Screen grab → BGRA (+ cursor composite) | GDI `BitBlt`/`GetDIBits` (`snapshot.rs`) | 149 |
| Monitor enumeration (mirror vs extend region) | `EnumDisplayMonitors` (`stream.rs`) | ~120 |
| Window list + raise to foreground | `EnumWindows`/`SetForegroundWindow` (`winlist.rs`) | 68 |
| Current Wi-Fi SSID + PSK (the join-QR) | `netsh wlan` (`wifi.rs`) | 145 |
| Inbound firewall rule detect/add | `netsh advfirewall` (`firewall.rs`) | 82 |

Six primitives is the whole job — **but on Linux each may need two
implementations**, X11 and Wayland.

## 3. Mode × display server — where it stops being a port

| Mode | X11 | Wayland |
|---|---|---|
| **Clicker** (control-only) | XTEST — a direct `SendInput` equivalent | portal `RemoteDesktop`/libei, **or** `uinput` |
| **Slide preview snapshots** | `XGetImage`/XShm | ScreenCast portal → PipeWire |
| **Window picker** | EWMH `_NET_CLIENT_LIST` + `_NET_ACTIVE_WINDOW` | ❌ **no protocol exists** — by design |
| **Trackpad** | XTEST relative motion | libei / `uinput` |
| **Mirror** | XShm capture | ScreenCast portal (consent dialog each session) |
| **Remote control** | XTEST + capture | portal + libei |
| **Second screen** (extend) | `xrandr` VIRTUAL output | ❌ compositor-specific, no generic route |

**Everything in the X11 column above is now built**, other than Mirror (Stage 2b)
and Second screen (deferred). The Wayland column is unchanged and unstarted.

Three consequences that change the product, not just the code:

- **Wayland has no window enumeration at all.** The clicker's window picker — a
  shipped ✅ feature — cannot be ported. It has to *degrade*, not port: on
  Wayland the clicker drives whatever is focused, and the UI has to say so.
- **Every Wayland capture opens with a user consent dialog** picking what to
  share. There is no "just grab the primary display". That is a first-run UX
  design question for Mirror and for the clicker's slide previews, not plumbing.
- ⚠️ **The input-injection fork is the decision worth making first.** The portal
  route is the sanctioned one but needs a compositor that implements it (GNOME
  45+, KDE Plasma 6; wlroots partial) and prompts for consent. **`uinput` is one
  implementation that covers X11 *and* Wayland, every compositor, no portal** —
  it collapses the 2× fan-out for the whole control half of the app. Its cost is
  write access to `/dev/uinput`: a udev rule at install, or a first-run
  instruction. For a clicker-first host, uinput is the cheaper and broader
  answer by a wide margin.

### 3a. What Stage 2a settled, with numbers

⚠️ **`GetImage` vs MIT-SHM was decided by measuring, not by reputation.** On
Xvfb at 1920×1080: **`GetImage` 10.7 ms/frame, MIT-SHM 0.93 ms/frame** — 11×.
For a *slide preview* (one grab per page turn) either is far inside budget; for
the Stage 2b mirror at 30 fps, 10.7 ms is a third of the frame budget spent
before the encoder starts, and 0.93 ms is nothing.

⚠️ **The reason to expect SHM to cost something turned out to be wrong, and the
detail matters for any future X work here.** `x11rb`'s `allow-unsafe-code`
feature pulls in `as-raw-xcb-connection`, which puts `-lxcb` on the link line
and so needs `libxcb1-dev` to build. The **`shm` feature alone does not** — the
SysV `shmget`/`shmat` calls are a handful of `libc` lines. So the fast path is
available with nothing linked: `ldd` on the release binary lists `libgcc_s`,
`libm`, `libc` and the loader, exactly as it did before capture existed.

⚠️ **`GetImage` is a real fallback, not a formality.** Shared memory requires
the X server to be on this machine, so a remote display (`ssh -X`) either has no
SHM extension or fails to attach. `SCREENS_X11_NO_SHM=1` forces the slow path
for debugging.

⚠️ **Pixel layout is read from the server, never assumed.** The near-universal
desktop layout is little-endian 32-bpp BGRX, and that takes a copy-only fast
path — but the masks are a property of the *visual*, so `capture.rs` decodes
from `red_mask`/`green_mask`/`blue_mask` and `image_byte_order`, and *scales*
narrow channels rather than shifting them (5 bits of full red is 31, and 31 as
an 8-bit channel is nearly black).

## 4. The two shell-outs degrade rather than port

- **Wi-Fi (`wifi.rs`).** `nmcli` gives the SSID, and the PSK via
  `nmcli -s -g 802-11-wireless-security.psk connection show <name>` — but reading
  a *secret* triggers a polkit prompt on most distros, and iwd /
  systemd-networkd systems have no equivalent at all. Expect the combined
  join-Wi-Fi QR to fall back to the existing "connect to the same network" note
  far more often than on Windows. Not a blocker; just don't build the UI
  assuming it works.
- **Firewall (`firewall.rs`).** No `netsh` equivalent, and no single target:
  ufw, firewalld and raw nftables all differ. Note that Ubuntu ships ufw
  *inactive*, so most desktop users need nothing — while Fedora's firewalld is
  active and **will** block 9000. Detect which is running, show the exact
  command, and don't attempt the pkexec elevation dance the Windows host does
  with UAC.

## 5. Packaging and CI

- **AppImage first.** It is the only format that matches what Universal Screens
  already does on Windows and macOS — per-user, no admin, unsigned,
  download-and-run — and it fits the standing ship-unsigned policy. A `.deb` can
  follow if anyone asks.
- ⚠️ **Not Flatpak, at least not first.** Flatpak is sandboxed and *forces* the
  portal path: no `uinput`, no XTEST outside the sandbox without punching holes.
  It removes exactly the capability this app exists to provide.
- **CI is the cheap part.** `ubuntu-latest` is the cheapest of the three runners,
  and `macos-release.yml` already carries a comment about a job that could be
  "folded into a Linux job".

## 6. ⚠️ Fix the GUI fork *before* adding a third host

`gui.rs` is 1,598 lines in `host-windows` and 1,451 in `host-macos`, already
diverged by ~1,000 lines once whitespace is normalised. A third copy means three
places to keep the navbar, the changelog popup and the profile disc in sync — and
the 2026-08-24 handover already records both navbar commits landing in
`host-windows/src/gui.rs` while that host **went uncompiled**. Extracting the
shared egui shell into a `host-ui` crate is the single biggest cost driver here,
and it is not a Linux problem; adding Linux just makes it three times as
expensive to keep ignoring.

## 7. Recommended staging

| Stage | Scope | Rough size |
|---|---|---|
| **0** | ✅ Decided: **uinput**, for the reason in §3. `host-ui` NOT extracted — see below. ✅ web-bridge/rustls done (§1). | done |
| **1** | ✅ **Built.** `crates/host-linux`: uinput injection, no capture, no window picker, identical under X11 and Wayland. AppImage (`scripts/build-appimage.sh`), udev rule, `linux-release.yml`. Cutting a tag is what flips "Coming soon". | done |
| **2a** | ✅ **Built.** X11 capture (`capture.rs`) + EWMH window picker (`winlist.rs`): slide previews, the deck scan and the window picker. MIT-SHM preferred, `GetImage` fallback, both pure-Rust. Six tests run against a live X server. | done |
| **2b** | The H.264 **mirror**: feed `capture::grab_primary_bgra` into a Linux `stream.rs` (openh264 needs `build-essential` + `nasm`, per §1). The capture half is done and the signature already matches the Windows host's. | 1–2 sessions |
| **3** | Wayland capture: `ashpd` portal + PipeWire, plus the consent UX. Unaffected by 2a — X11 and Wayland capture share nothing but the `capture::` entry points. | 4+, and needs real hardware |
| **1b** | Fold the `host-ui` extraction in — see §6. Deferred, not dropped: doing it now meant editing `host-macos/gui.rs` with no Mac to compile it on, which is the exact failure §8 warns about. Stage 1 avoided the problem by writing a lean window instead of a third fork. | next |
| **—** | **Second screen: defer.** The Windows trick (stream the first non-primary monitor; an external driver makes it exist) transfers to X11 via a VIRTUAL output far more cheaply than macOS's `CGVirtualDisplay` did. Wayland has no generic answer. | not scheduled |

Stage 1 is deliberately the smallest shippable thing, and it is the mode the app
is actually used in at a lectern.

Hardware H.264 encode, if it is ever wanted here, is VAAPI — the Linux twin of
the Media Foundation work in progress on Windows. Software openh264 is the same
trade-off on all three platforms, so Stage 2 inherits the existing ≤1280px cap
rather than needing its own answer.

## 8. ⚠️ Testing reality

**There is no Linux machine in this workspace** — WSL here has only a
`docker-desktop` distro. Docker turned out to be enough for more than expected,
and the line between what it proves and what it can't is the important part.

**What a container did prove for Stage 1:**

- The crate **compiles and links** on real Linux, eframe and all — much stronger
  than the `cargo check` in §1, which never invokes the linker.
- All 27 unit tests pass: the HID→evdev key map, the `nmcli` and `ufw` parsing,
  and the connect URL.
- `scripts/build-appimage.sh` produces an AppImage, and the **packaged binary
  starts and accepts a TCP connection** — the same shape of check the macOS job
  runs against its mounted DMG.

**And for the rustls swap (2026-08-25), a container proved more than usual.** The
`rust:slim` image ships **no `libssl-dev` at all**, so it simply could not have
compiled the old tree — the build succeeding there *is* the result. `ldd` on the
linked binary then showed the absence directly. The one Linux test failure,
`peers_endpoint`, is a container with no mDNS multicast; it passes on Windows.

⚠️ **Still unrun for that change: the macOS build.** `cargo tree` resolves for
both `*-apple-darwin` targets and the code is platform-independent, but there is
no Mac here to compile it. `security-framework` also moved 3.7.0 → 2.11.1
(`rustls-native-certs` 0.7 pins 2.x); nothing else in the workspace depends on
it, so the downgrade is confined to that one crate.

**And Stage 2a moved the line — a container CAN prove X11 capture.** This is
worth being precise about, because Stage 1's "verified in a container" was much
weaker. Xvfb is not a *stand-in* for an X server; it **is** an X server, running
the same protocol a desktop one does over the same socket. So the capture tests
paint a root window and read the pixels back for real:

- Six tests in `src/x11_tests.rs` run against a live server. They paint a colour
  whose three channels **differ**, so a B↔R swap, a big-endian misread or a
  stride error cannot pass unnoticed.
- Proven by mutation, not by passing: swapping the channels in the fast path
  fails three of them with the pixel values named, and shifting the SHM read by
  one pixel is caught by the SHM-vs-`GetImage` cross-check.
- ⚠️ That cross-check exists because the first cut of this work picked SHM on a
  **speed** measurement and never compared its output to `GetImage`'s. A fast
  wrong frame looks exactly like a fast right one.
- ⚠️ `SCREENS_REQUIRE_X11=1` turns a skipped X11 test into a **failure**, and
  `linux-release.yml` sets it under `xvfb-run`. Without it the job goes green
  while quietly not exercising capture at all — the same false clean pass this
  section exists to warn about.

⚠️ **Three traps specific to testing against a live X server**, all of which
first appeared as convincing "the capture code is broken" failures:

- **`xsetroot -solid` exits 0 and leaves the Xvfb framebuffer black.** X frees a
  client's resources when it disconnects, so a short-lived helper's paint is not
  reliably still there. The tests now paint from their own connection and hold
  it open for the test's lifetime.
- **One display is global mutable state.** `cargo test` runs tests as threads in
  one process, so the window-picker test's window was being photographed by
  capture tests running concurrently. They take a mutex.
- **A test that mutates `DISPLAY` breaks every other test**, for the same
  single-process reason. Pass the display *in* instead — `Ewmh::open_display`
  exists only for that.

⚠️ **What no container can prove, and what is therefore still unrun:**

- **Injection itself.** A container has no `/dev/uinput` and no desktop to
  receive the events. Every keystroke path in `inject.rs` is tested at the
  mapping layer and unexercised at the kernel layer. Unchanged by Stage 2a:
  capture is a socket protocol, injection is a kernel device, and only the
  first of those a container has.
- **The GUI has never been drawn.** It compiles; nobody has seen it.
- **A real client against the packaged AppImage.** The smoke test proves it
  starts and listens; nothing has completed a handshake and received a
  preview.
- **Capture on a real desktop**, as opposed to Xvfb: a compositing WM, a
  multi-monitor `xrandr` layout, and a GPU whose readback path is not a
  software framebuffer. The correctness tests should hold; the timings
  above are Xvfb's and may not.
- **Wayland**, for the same reason as before: portal behaviour and compositor
  differences need a real logged-in session, and GNOME and KDE differ.

The precedent is in this repo's own handover: `crates/host-macos` was edited
blind across several sessions, compiled clean first try — and had still never
been *run*. Stage 1 is in exactly that state for the half a container can't
reach. **The first thing to do on a real Linux desktop is install the udev rule
and check that a phone actually moves a slide.**
