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
evening and v0.4.4 re-cut. When the icon changes, regenerate every surface at
once and commit what it writes:

```bash
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

### ⚠️ The running app must not set its own Dock icon

`AppIcon.icns` is drawn on Apple's grid: the art in a centred 824×824 rounded
square (radius 22.4%) on the 1024 canvas, transparent margins, soft shadow.
Measured the same as Calculator, Notes and App Store: tile 824×824 at (100,100).
Only the macOS surfaces are inset. Windows, Linux, Android and iOS keep their
full-bleed conventions, and `app-icon.png` stays full bleed because it's also
the QR badge.

That alone wasn't enough. Up to 0.4.7 Screens was **a size larger than every
other app in the Dock and Cmd-Tab** while it ran (James, 2026-11-18). The cause
was `gui.rs`: eframe's `ViewportBuilder::with_icon` isn't a window icon on macOS.
eframe 0.29 passes it to `NSApplication.setApplicationIconImage`, which replaces
the bundle's icon for the life of the process, and it was fed the full-bleed
`app-icon.png`. A replaced icon also skips macOS's tinted and dark icon styles.
Inside a `.app`, `run()` now passes `egui::IconData::default()`, which eframe
reads as "set nothing". **Don't just leave `with_icon` out:** eframe then
pushes egui's own logo instead. Only an unbundled `cargo run` gets the art.
Checked in the Dock, side by side: the bundled build sits the same size as
System Settings and takes the tint, and the unbundled one is oversized.

After an update the Dock may keep the old picture cached: `killall Dock`.

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

## Signing: Developer ID, notarised (since 0.4.8)

Some signature is not optional: macOS **refuses to execute an unsigned arm64
binary**, killing it at exec. Which signature depends on the environment
`build-app-macos.sh` runs in:

| Variable | Effect |
|---|---|
| `MACOS_SIGN_IDENTITY` | `Developer ID Application: Universal Simulation Ltd (ZH9C5TS86A)`. Signs `Resources/extender-client` (identifier `com.universalsim.screens.client`), then the app, with hardened runtime + secure timestamp, then the DMG. Unset: ad-hoc. |
| `NOTARY_KEY_PATH` / `NOTARY_KEY_ID` / `NOTARY_ISSUER` | An App Store Connect API key (`V366TJ4SJ5`). With an identity, the DMG goes to `notarytool submit --wait`, is stapled, and must pass `spctl --assess`. |
| `REQUIRE_SIGNING=1` | Fail instead of falling back. The release workflow sets it on tags, so **a tagged release can never ship ad-hoc or un-notarised**. |

No entitlements: the host is not sandboxed, and ScreenCaptureKit, Accessibility
and the private `CGVirtualDisplay` are gated by TCC, not entitlements.

### ⚠️ Why ad-hoc was dropped: it loses the privacy grants on every update

TCC keys an ad-hoc app's Screen Recording and Accessibility grants to that exact
build. After an update the switches in System Settings **still show on** while
macOS refuses: a connecting device gets the virtual display, then "stream ended"
and a black window (the host logs `The user declined TCCs for application,
window, display capture`), or a picture with no control. Found 2026-11-18 on
James's Mac running 0.4.3. A Developer ID build's designated requirement is the
team (`ZH9C5TS86A`) + bundle ID, so grants survive updates.

**Moving from an ad-hoc build (≤ 0.4.7) to a signed one counts as a new signer
once**: remove Universal Screens from Screen & System Audio Recording and from
Accessibility with −, add it back, restart. Never again after that.

### CI secrets (`universal-simulation-ltd/Universal_Screens`)

`macos-release.yml` imports the certificate into a throwaway keychain, writes the
API key to `$RUNNER_TEMP`, and deletes both at the end of the job.

| Secret | What |
|---|---|
| `MACOS_CERT_P12_BASE64` | base64 of a .p12 holding only the Developer ID Application certificate + key |
| `MACOS_CERT_PASSWORD` | that .p12's password |
| `MACOS_SIGN_IDENTITY` | the identity name above |
| `ASC_API_KEY_P8_BASE64` | base64 of `AuthKey_V366TJ4SJ5.p8` (`~/.unisim-keys/`) |
| `ASC_API_KEY_ID` / `ASC_API_ISSUER_ID` | `V366TJ4SJ5` / the team issuer ID |

A branch dispatch (`gh workflow run macos-release.yml --ref main`) signs and
notarises too when the secrets exist, but publishes nothing: use it to prove a
change to signing before tagging.

Proven 2026-11-18: run 44333503441 on main signed, notarised and stapled, and
its artifact passed `spctl` as "Notarized Developer ID". Two traps from setting
it up: the first export held the neighbouring **Apple Development** certificate
(the script now lists the identities it finds when the wanted one is missing),
and GitHub's arm64 macOS runners repeatedly failed to pick the job up at all
("The job was not acquired by Runner … capacity constraints") — that is not a
signing failure; dispatch again.

### ⚠️ The certificate expires 1 February 2027

Apps signed before then keep running (the secure timestamp proves when they were
signed), but **new releases fail to sign** after it. Renew in
developer.apple.com → Certificates (Developer ID Application), install it in
the login keychain, export it from Keychain Access as a .p12 and replace the two
`MACOS_CERT_*` secrets. Keychain export needs James: Claude's sandbox may not
read private keys out of the keychain.

### ⚠️ Notarisation stops when Apple's agreement lapses

`notarytool` returns **HTTP 403 "A required agreement is missing or has
expired"** whenever Apple publishes an updated Developer Program License
Agreement and nobody has accepted it yet (developer.apple.com/account banner, or
App Store Connect → Business). Only the account holder can accept it. With
`REQUIRE_SIGNING=1` the release fails rather than shipping un-notarised.

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
- **Gatekeeper on a machine that has never seen the app.** The notarised DMG passes `spctl` here; a first launch on a clean Mac has not been watched.
