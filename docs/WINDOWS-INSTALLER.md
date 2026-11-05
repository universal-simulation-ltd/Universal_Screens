# Windows installer

`UniversalScreens-Setup-<version>.exe` — one double-click, no admin prompt, no
prerequisites. It installs the **host** (the GUI you run on the PC whose screen
you're sharing) and, optionally, the **desktop client**.

Build it:

```powershell
.\scripts\build-installer.ps1          # -> dist\UniversalScreens-Setup-0.4.3.exe
.\scripts\build-installer.ps1 -SkipBuild   # repackage without recompiling
```

or push a `v*` tag and let [`windows-release.yml`](../.github/workflows/windows-release.yml)
do it on a clean runner and attach it to a GitHub release.

## What the pieces are

| File | What |
|---|---|
| [`installer/universal-screens.iss`](../installer/universal-screens.iss) | the Inno Setup script — everything about the install lives here |
| [`installer/README-installed.txt`](../installer/README-installed.txt) | dropped next to the binaries; the "Read me" Start Menu entry |
| [`scripts/build-installer.ps1`](../scripts/build-installer.ps1) | compiles, runs ISCC, writes a `.sha256` |
| [`scripts/make-win-ico.py`](../scripts/make-win-ico.py) | **superseded** — draws the pre-rebrand artwork; do not run it |
| [`crates/host-windows/build.rs`](../crates/host-windows/build.rs) | embeds that icon + version metadata into the `.exe` |

Prerequisites on the build machine: Rust (MSVC), **NASM** (openh264 assembles
from source) and **Inno Setup 6** (`winget install JRSoftware.InnoSetup`). The
build script checks all three up front and tells you the fix rather than failing
halfway through.

The app icon is the committed `crates/host-windows/assets/app-icon.ico` and the
build never regenerates it — it used to, from artwork superseded on 2026-08-27,
which quietly reverted the mark on every build. When the icon changes, regenerate
every surface at once and commit what it writes:

```powershell
node scripts/app-icon.mjs
```

The icon itself is **Screens' own**, not the suite mark on the orange tile: "one
picture, split" — a desk screen and a phone on teal with a single orange disc
drawn across both, so the arc leaving the monitor and the arc arriving on the
phone are the same curve. `scripts/app-icon.mjs` draws it and writes every
installed surface at once (Windows `.png`/`.ico`, macOS `.png`/`.icns`, Linux
`.png`, the Android launcher set, the iOS asset catalogue). Universal PDF set
the precedent; the flags that make the suite generators stand aside are
`ownNativeIcon` / `ownDesktopIcon` on the `Universal_Screens` entry in
`backoffice/universal-platform/scripts/app-marks/marks.mjs`.

Two surfaces are still the **suite's** and must not be drawn here: the sealed
installer icon (`crates/host-windows/assets/installer-icon.ico` — an installer
is not the app) and the phone clients' in-app masthead logo. Both come from
`node backoffice/universal-platform/scripts/app-marks/desktop-icons.mjs Universal_Screens`.

The **web** surfaces are this icon too — favicon, header logo, app switcher,
portal and marketing site — because Screens is published beyond the web (James,
2026-11-03: the orange marks are kept for browser-only apps, so the two kinds
can be told apart). `SVG_OUT=… node scripts/app-icon.mjs` writes the drawing to
`backoffice/universal-platform/scripts/app-marks/own-icons/screens.svg`, which
`webIcon` on the `Universal_Screens` entry in `marks.mjs` points at; the suite's
own `build.mjs` / `rasterise.mjs` run is what pushes it out to those surfaces.

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
UniversalScreens-Setup-0.4.4.exe /ALLUSERS /VERYSILENT /NORESTART
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

**Upgrades are covered now** (2026-11-04): 0.4.2 installed silently over that
0.1.0 install, then 0.4.3 over 0.4.2 — both per-user, both with no UAC prompt.
Add/Remove Programs showed **0.4.3**, publisher Universal Simulation Ltd;
`extender-host-windows.exe` reported FileVersion 0.4.3; the installed
`app-icon.ico` was byte-identical to the committed brand file and the icon
embedded in the `.exe` was the orange brand mark (the check that caught the
icon-regeneration bug above); the host ran headless on `127.0.0.1:9101`
(protocol v10, accepting a TCP connection) and the GUI opened its "Universal
Screens" window.

**And the released installer is covered now** (2026-11-04): v0.4.4's
`UniversalScreens-Setup-0.4.4.exe`, downloaded from the GitHub release rather
than built locally, installed silently over 0.4.3 on the same box. Add/Remove
Programs showed **0.4.4**, the exe reported FileVersion 0.4.4, the host ran
headless on `127.0.0.1:9103` (protocol v10) and the GUI window opened. All
three of that release's published `.sha256` files matched their assets, and the
copy on `downloads.unisim.co.uk/universal-screens/` is the same artefact.

**Still not covered:** a machine-wide `/ALLUSERS` install, what SmartScreen
actually does on a machine that has never seen this binary (this one has, so it
can't be tested here), and a first install onto a PC with no earlier version
present. Sign-in on the installed host has not been driven either.
