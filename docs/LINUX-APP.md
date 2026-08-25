# Universal Screens on Linux

The Linux host is the **clicker and trackpad**: your phone drives the keyboard
and mouse on this machine. Mirror, remote control and second screen are Windows
and macOS for now — [`LINUX-HOST.md`](LINUX-HOST.md) is the scope that explains
why capture is a separate project and injection isn't.

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

- **No screen mirroring, remote control or second screen.** Capture forks into
  an X11 implementation and a Wayland/PipeWire one; see
  [`LINUX-HOST.md`](LINUX-HOST.md) §3.
  ⚠️ **The phone's mode picker doesn't know that.** It offers every mode against
  every host, because nothing in the protocol lets a host advertise what it can't
  do. Pick Mirror against a Linux host and you get a **black screen** — your keys
  and trackpad still work, because the session is served as a clicker, but no
  video ever arrives. Telling the phone properly needs a protocol addition and a
  client release, so for now: pick Clicker or Trackpad.
- **No window picker.** The Windows clicker can list your open windows and bring
  one to the front. Wayland has no window-enumeration protocol *by design*, so
  the host answers the phone with an empty list and your keystrokes go to
  whatever is focused.
- ⚠️ **Typing from the phone's soft keyboard assumes a US QWERTY layout.**
  uinput sends key *positions*, and your compositor applies your layout
  afterwards — so on AZERTY, asking for `a` produces `q`. Non-ASCII characters
  can't be sent at all. The keys that matter for presenting — arrows, PageUp,
  PageDown, F5, Escape, `b`/`w`/`.` for blanking — are layout-independent and
  unaffected.
- **Wayland is unaffected by all of the above.** uinput sits below the display
  server, so the clicker works identically under X11, GNOME, KDE and wlroots,
  with no portal consent dialog.

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
  libxi-dev libgl1-mesa-dev libxkbcommon-dev libwayland-dev
cargo run -p extender-host-linux            # GUI
cargo run -p extender-host-linux -- 0.0.0.0:9000   # headless
./scripts/build-appimage.sh                 # dist/UniversalScreens-*.AppImage
```

Unlike the Windows host, **NASM is not needed** — there is no openh264 in this
build, because there is no video to encode.

⚠️ **Build the AppImage on the oldest distro you intend to support**, or let
[`linux-release.yml`](../.github/workflows/linux-release.yml) do it (pinned to
`ubuntu-22.04` for exactly this reason). An AppImage links against the build
machine's glibc, which is forward- but not backward-compatible: built on a newer
distro it fails to start on older ones with a `GLIBC_2.xx not found` error that
names the symbol rather than the cause.
