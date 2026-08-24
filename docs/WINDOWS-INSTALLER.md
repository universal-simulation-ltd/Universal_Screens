# Windows installer

`UniversalScreens-Setup-<version>.exe` — one double-click, no admin prompt, no
prerequisites. It installs the **host** (the GUI you run on the PC whose screen
you're sharing) and, optionally, the **desktop client**.

Build it:

```powershell
.\scripts\build-installer.ps1          # -> dist\UniversalScreens-Setup-0.1.0.exe
.\scripts\build-installer.ps1 -SkipBuild   # repackage without recompiling
```

or push a `v*` tag and let [`windows-release.yml`](../.github/workflows/windows-release.yml)
do it on a clean runner and attach it to a GitHub release.

## What the pieces are

| File | What |
|---|---|
| [`installer/universal-screens.iss`](../installer/universal-screens.iss) | the Inno Setup script — everything about the install lives here |
| [`installer/README-installed.txt`](../installer/README-installed.txt) | dropped next to the binaries; the "Read me" Start Menu entry |
| [`scripts/build-installer.ps1`](../scripts/build-installer.ps1) | compiles, regenerates the icon, runs ISCC, writes a `.sha256` |
| [`scripts/make-win-ico.py`](../scripts/make-win-ico.py) | multi-resolution `.ico` from the shared `render_icon()` artwork |
| [`crates/host-windows/build.rs`](../crates/host-windows/build.rs) | embeds that icon + version metadata into the `.exe` |

Prerequisites on the build machine: Rust (MSVC), **NASM** (openh264 assembles
from source), **Inno Setup 6** (`winget install JRSoftware.InnoSetup`), and
Python with Pillow for the icon. The build script checks all four up front and
tells you the fix rather than failing halfway through.

## The three decisions worth knowing about

**Static CRT — this is load-bearing.** The binaries are built with
`-C target-feature=+crt-static`. Without it they import `VCRUNTIME140.dll`, and
the install then depends on a Visual C++ redistributable that plenty of clean
Windows machines have never had. With it, the whole app is two self-contained
`.exe` files and the installer ships no DLLs at all. The build script *verifies*
this — it scans each packaged binary for a `VCRUNTIME*.dll` import and refuses
to package one that has it, because a dynamically-linked build packages happily
and only fails on someone else's machine.

**Per-user by default.** `PrivilegesRequired=lowest` installs into
`%LOCALAPPDATA%\Programs\Universal Screens` and never shows a UAC prompt, which
matters because a good number of the people this is for are on a managed work
laptop. IT can still deploy it machine-wide:

```powershell
UniversalScreens-Setup-0.1.0.exe /ALLUSERS /VERYSILENT /NORESTART
```

**No firewall rule at install time.** The host adds one from its own UI, at the
moment a phone actually needs to reach it — one UAC prompt, in a context where
the user can see why. Doing it during setup would mean asking for elevation from
an install that otherwise needs none, to open a port that a loopback- or
USB-only user never uses. See [`firewall.rs`](../crates/host-windows/src/firewall.rs).

## Unsigned, and what that looks like

There is no code-signing certificate — [standing UNI·SIM policy](https://opensource.unisim.co.uk/usb)
for the free tools. On first run Windows shows **"Windows protected your PC"**;
*More info* → *Run anyway* gets past it. The SmartScreen reputation warning fades
as a given installer accumulates downloads, and resets with every new version.

On a locked-down corporate machine an unsigned installer is sometimes blocked
outright with no override. Building from source is the way in there — see the
[quickstart](../README.md#build--run-quickstart).

## Verified

Round-tripped on Windows 11 (2026-08-24, v0.1.0): silent install → files and
both Start Menu shortcuts land in the per-user location with no UAC prompt →
Add/Remove Programs shows the app with its publisher and icon → the installed
host launches its GUI *and* runs headless (`extender-host-windows.exe
127.0.0.1:9000`, accepting a TCP connection) → silent uninstall removes the
directory, the shortcuts and the registry entry.

**Not covered by that:** an upgrade over an existing install, a machine-wide
`/ALLUSERS` install, and what SmartScreen actually does on a machine that has
never seen this binary (this one has, so it can't be tested here).
