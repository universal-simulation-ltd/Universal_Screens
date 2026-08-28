# Universal Screens on Linux

The Linux host is the **clicker and trackpad**: your phone drives the keyboard
and mouse on this machine. On an **X11** session it also does **slide previews,
the window picker, screen mirroring / remote control, and using the phone as a
**second screen** — extra desktop, not a copy of your monitor.

⚠️ **Which of those you get depends on your session, and the app tells you
which.** Everything except the clicker and trackpad needs X11: on Wayland there
is no way to read the screen without a per-session consent dialog that isn't
built yet, and no way to list windows at all. The host window says
"Screen mirroring and previews ready" or "Screen mirroring off" on startup, and
the log line says the same.
[`LINUX-HOST.md`](LINUX-HOST.md) is the scope that explains why capture forks
that way and injection doesn't.

## Install

Download `UniversalScreens-*.AppImage` from the
[releases page](https://github.com/universal-simulation-ltd/Universal_Screens/releases),
make it executable, run it. No installer, no admin, no package manager.

```bash
chmod +x UniversalScreens-*.AppImage
./UniversalScreens-*.AppImage
```

It is **not signed**. That is standing UNI·SIM policy, the same as the unsigned
Windows installer and the ad-hoc-signed Mac app. Check the download against the
`.sha256` published beside it:

```bash
sha256sum -c UniversalScreens-*.AppImage.sha256
```

## The one setup step

⚠️ **The app types on your behalf through `/dev/uinput`, which is root-owned by
default.** Until that is fixed the app runs, the phone connects, the window says
"Connected" — and nothing moves. The app checks the permission when it opens and
shows these commands itself, but here they are:

```bash
sudo usermod -aG input $USER
echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' \
  | sudo tee /etc/udev/rules.d/99-universal-screens-uinput.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

⚠️ **Then log out and back in.** Group membership is established at login, so
until you do, everything above looks like it did nothing.

The rule is also shipped inside the AppImage at
`usr/share/doc/universal-screens/99-universal-screens-uinput.rules`, and in this
repo at [`installer/`](../installer/99-universal-screens-uinput.rules).

## Connecting

1. **Get the app** on the phone — scan the first QR, or go to
   `opensource.unisim.co.uk/screens`.
2. **Scan to connect** — the second QR carries the address, the PIN and, when
   the host could read them, the Wi-Fi credentials.
3. Or type the address and 4-digit PIN shown under the QR.

Over USB, `adb reverse tcp:9000 tcp:9000` then connect to `127.0.0.1:9000`.

The connection is PIN-gated and encrypted (Noise, keyed by the PIN) exactly as
on the other two hosts — see the Security section of the main
[README](../README.md).

## If the phone can't reach the host

Most desktop Linux needs nothing: Ubuntu ships ufw *inactive*, and Debian and
Arch ship no inbound filtering at all. **Fedora is the exception** — firewalld is
on by default and will drop the connection silently. The host detects both and
shows the exact command; it never changes your firewall itself.

```bash
sudo firewall-cmd --add-port=9000/tcp --permanent && sudo firewall-cmd --reload   # Fedora
sudo ufw allow 9000/tcp                                                           # if ufw is active
```

## Known limitations

These are properties of the platform, not bugs:

- **The second screen has no hardware behind it.** It is a RandR monitor over
  extra framebuffer, which is enough for windows to maximise into it and for the
  pointer to cross — but it is not a display your GPU scans out, so a few things
  behave oddly: a compositor may not redirect windows there, and full-screen
  video may refuse to go full-screen on it. Your desktop widens while the phone
  is connected and goes back to normal the moment it disconnects.
  ⚠️ If the desktop *cannot* be widened — a headless or fixed-size X server, or a
  driver already at its maximum framebuffer — you get a **mirror** instead, and
  the host log says why in one line.
- **On Wayland, no mirroring, no second screen, no remote control, no slide
  previews and no window picker.** Capture forks into an X11 implementation and a Wayland/PipeWire one;
  window enumeration has no Wayland protocol *by design*. See
  [`LINUX-HOST.md`](LINUX-HOST.md) §3.
  ⚠️ **The phone's mode picker doesn't know which session you're on.** It offers
  every mode against every host, because nothing in the protocol lets a host
  advertise what it can't do. Pick Mirror against a Wayland Linux host and you
  get a **black screen** — your keys and trackpad still work, because the session
  is served as a clicker, but no video ever arrives. Telling the phone properly
  needs a protocol addition and a client release, so on Wayland: pick Clicker or
  Trackpad.
- **Tapping the mirrored picture doesn't move the pointer.** Remote control is
  driven by *relative* motion — the trackpad — on Linux and Windows alike;
  absolute taps are ignored by both hosts.
- ⚠️ **Typing from the phone's soft keyboard assumes a US QWERTY layout.**
  uinput sends key *positions*, and your compositor applies your layout
  afterwards — so on AZERTY, asking for `a` produces `q`. Non-ASCII characters
  can't be sent at all. The keys that matter for presenting — arrows, PageUp,
  PageDown, F5, Escape, `b`/`w`/`.` for blanking — are layout-independent and
  unaffected.
- **The clicker and trackpad are unaffected by all of the above.** uinput sits
  below the display server, so they work identically under X11, GNOME, KDE and
  wlroots, with no portal consent dialog. It is only the parts that need to
  *read* the screen that fork.

## Running headless

Pass a bind address to skip the window:

```bash
./UniversalScreens-*.AppImage 0.0.0.0:9000
```

It logs each connection to stdout and warns up front if `/dev/uinput` isn't
writable.

## Building it yourself

```bash
sudo apt-get install -y pkg-config libx11-dev libxcursor-dev libxrandr-dev \
  libxi-dev libgl1-mesa-dev libxkbcommon-dev libwayland-dev \
  build-essential nasm
cargo run -p extender-host-linux            # GUI
cargo run -p extender-host-linux -- 0.0.0.0:9000   # headless
./scripts/build-appimage.sh                 # dist/UniversalScreens-*.AppImage
```

⚠️ **`build-essential` and `nasm` are in that list for the mirror**, and this
document used to say NASM was *not* needed. Since Stage 2b the build carries
openh264, which `openh264-sys2` compiles from source, and its x86 assembly needs
an assembler. Without them the build fails inside a C build script, with an
error that names `nasm` and never mentions video.

✅ **The whole release sequence was rehearsed on a bare `ubuntu:22.04`**
(2026-08-26) — the workflow's exact apt list, then the tests under Xvfb, a full
release build, `build-appimage.sh`, and the packaged binary accepting a
connection. So the dependency list in
[`linux-release.yml`](../.github/workflows/linux-release.yml) is known
sufficient rather than assumed, and cutting the first `v*` tag should be a
formality. ⚠️ `ldd` on that build lists `libstdc++` as well as `libgcc_s`,
`libm`, `libc` and the loader — openh264 is C++ — which is one more reason the
build machine must not be newer than the oldest distro you support.

⚠️ **Build the AppImage on the oldest distro you intend to support**, or let
[`linux-release.yml`](../.github/workflows/linux-release.yml) do it (pinned to
`ubuntu-22.04` for exactly this reason). An AppImage links against the build
machine's glibc, which is forward- but not backward-compatible: built on a newer
distro it fails to start on older ones with a `GLIBC_2.xx not found` error that
names the symbol rather than the cause.
