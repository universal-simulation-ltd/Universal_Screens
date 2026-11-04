# The macOS app and DMG

The Mac counterpart of [`WINDOWS-INSTALLER.md`](WINDOWS-INSTALLER.md).
`scripts/build-app-macos.sh` compiles the host and client, wraps the host in
`Universal Screens.app`, and packages it into `UniversalScreens-<version>.dmg`.

```bash
cd /Users/jamesmarkey/Github/UNISIM/Universal_Apps/Universal_Screens
./scripts/build-app-macos.sh
```

`--skip-build` repackages what is already compiled; `--version X` overrides the
version stamped on the bundle. Output lands in `dist/`.

CI does the same thing on a clean runner via `.github/workflows/macos-release.yml`.

---

## ⚠️ The icon is committed, and the build must not redraw it

The app icon is the committed `crates/host-macos/assets/AppIcon.icns`.
`build-app-macos.sh` copies it into the bundle and refuses to package without
it — **and nothing in the release path regenerates it any more.**

It used to. `.github/workflows/macos-release.yml` installed Pillow and ran
`python3 scripts/make-mac-icns.py` before every build, and that script redraws
the laptop-and-phone artwork superseded on 2026-08-27 — the same bug
`build-installer.ps1` had on Windows. **v0.4.3's DMG was released (2026-11-04)
with the pre-rebrand icon because of it**; both steps were removed the same
evening and v0.4.4 re-cut. When the mark changes, regenerate every surface at
once with the suite generator and commit what it writes:

```bash
node backoffice/universal-platform/scripts/app-marks/desktop-icons.mjs Universal_Screens
```

Any generated asset that is also committed has two writers, and the build
script wins silently. Do not re-add an icon step here.
(`scripts/make-mac-icns.py` carries the same warning at the top of the file.)

## ⚠️ The deployment target is load-bearing

The script exports `MACOSX_DEPLOYMENT_TARGET=12.3` before building, and refuses
to package a binary that does not carry it.

That is not tidiness. The host calls **ScreenCaptureKit, which is macOS 12.3+**.
Left to itself, rustc stamps its own defaults — **11.0 on arm64 and 10.12 on
x86_64** — so the app installs perfectly happily on macOS 11 and then fails to
capture anything. `LSMinimumSystemVersion` in `Info.plist` matches, so the
Finder refuses the launch outright with a clear message instead.

Both facts were measured on the real binaries with `vtool -show-build`, not
assumed.

## ⚠️ The ad-hoc signature is what makes it run at all

`codesign --sign -` is not decoration and not a substitute for notarisation.
macOS **refuses to execute an unsigned arm64 binary** — it is killed at exec
rather than warned about. Without this step the app does not launch on any Apple
Silicon Mac.

It is still not notarised, so a *downloaded* copy is challenged by Gatekeeper
("cannot check it for malicious software"). The user has to right-click → Open
once. `installer/README-macos.txt` explains that, and so does the download page.

## ⚠️ Universal, or Intel Macs get nothing

Both slices are cross-compiled and `lipo`d together, and the script asserts both
are present before packaging. A single-architecture build packages perfectly
happily — which is exactly why it needs a guard rather than a good intention.

## Why the client has no icon

The DMG ships one app. `extender-client` rides inside the bundle at
`Contents/Resources/extender-client` and deliberately gets no launcher, mirroring
`installer/universal-screens.iss` on Windows and for the same reason recorded
there: the client takes a host address on the command line, so a bare
double-click would only ever fail to reach `127.0.0.1`. `README.txt` in the DMG
gives the command.

## Permissions

Three system prompts, and the app is not much use without the first two:

| Permission | Why | Without it |
|---|---|---|
| Screen Recording | capture the display | nothing to send; needs an app restart after granting |
| Accessibility | inject pointer/keyboard from a connected phone | picture but no control |
| Local Network | `_usscreens._tcp` discovery on macOS 15+ | works if you type the address by hand; discovery finds nothing |

`NSLocalNetworkUsageDescription` and `NSBonjourServices` are in the generated
`Info.plist`. Without the usage string macOS **denies local networking silently**
— discovery just returns empty, with no prompt and no error.

## Bash 3.2

The script avoids `mapfile` and other bash 4 builtins. macOS still ships **bash
3.2**; CI runners have bash 5, so a bash-4-ism passes CI and fails on the machine
of anyone who runs the script locally.

## Not covered

- **The x86_64 slice has never been run.** It is built and its architecture and
  deployment target are asserted, but development is on Apple Silicon — no Intel
  Mac has executed it.
- **Gatekeeper on a machine that has never seen the app.** This one has.
- **Notarisation**, deliberately — a recurring Developer ID cost, not paid for a
  free tool.
