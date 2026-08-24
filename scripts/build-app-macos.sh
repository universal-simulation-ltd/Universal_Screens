#!/usr/bin/env bash
#
# Build the Universal Screens macOS app + DMG.
#
# Compiles the host and client for BOTH architectures, lipos them into universal
# binaries, wraps the host in Universal Screens.app, and packages that into a
# UniversalScreens-<version>.dmg with an /Applications symlink to drag it into.
#
# Output lands in dist/. Nothing is notarised and the signature is ad-hoc --
# see docs/MACOS-APP.md.
#
# Usage:
#   ./scripts/build-app-macos.sh              # full build
#   ./scripts/build-app-macos.sh --skip-build # repackage what's already compiled
#   ./scripts/build-app-macos.sh --version X  # override the stamped version
#
# There is no Windows twin of this script and cannot be: codesign, hdiutil,
# iconutil and lipo are all macOS-only. scripts/build-installer.ps1 is the
# Windows side of the same job.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

# ⚠️ LOAD-BEARING, not tidiness. The host calls ScreenCaptureKit, which is macOS
# 12.3+. Without this, rustc stamps the binaries with its own defaults -- 11.0 on
# arm64 and 10.12 on x86_64 -- so the app installs happily on macOS 11 and then
# fails to capture anything, which is the worst place for someone to find out.
# The guard below refuses to package a build that lost this.
export MACOSX_DEPLOYMENT_TARGET=12.3
MIN_OS="$MACOSX_DEPLOYMENT_TARGET"

APP_NAME="Universal Screens"
BUNDLE_ID="com.universalsim.screens.host"
HOST_BIN="extender-host-macos"
CLIENT_BIN="extender-client"
ARCHES=(aarch64-apple-darwin x86_64-apple-darwin)

SKIP_BUILD=0
VERSION=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --version)    VERSION="${2:-}"; shift 2 ;;
    -h|--help)    sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# ── Version: single source of truth is [workspace.package] in Cargo.toml ──────
if [[ -z "$VERSION" ]]; then
  # Match build-installer.ps1's rule: the version is the first `version = "..."`
  # inside [workspace.package]. Extracted with sed rather than a greedy awk gsub,
  # which happily matches the whole line and yields an empty string.
  VERSION="$(sed -n '/^\[workspace\.package\]/,/^\[/ s/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -1)"
  [[ -n "$VERSION" ]] || { echo "Couldn't read version from [workspace.package] in Cargo.toml" >&2; exit 1; }
fi
echo "==> Universal Screens $VERSION (min macOS $MIN_OS)"

for tool in cargo lipo codesign hdiutil sips; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }
done

# ── Compile both architectures ────────────────────────────────────────────────
if [[ "$SKIP_BUILD" -eq 0 ]]; then
  for arch in "${ARCHES[@]}"; do
    rustup target add "$arch" >/dev/null 2>&1 || true
    echo "==> cargo build --target $arch"
    cargo build --release --target "$arch" -p extender-host-macos -p extender-client
  done
fi

# ── Universal binaries ────────────────────────────────────────────────────────
BUILD="$REPO/target/macos-universal"
rm -rf "$BUILD"; mkdir -p "$BUILD"
for bin in "$HOST_BIN" "$CLIENT_BIN"; do
  slices=()
  for arch in "${ARCHES[@]}"; do
    path="target/$arch/release/$bin"
    [[ -f "$path" ]] || { echo "Missing $path -- build it before packaging (drop --skip-build)" >&2; exit 1; }
    slices+=("$path")
  done
  lipo -create "${slices[@]}" -output "$BUILD/$bin"
done

# ── Guards ────────────────────────────────────────────────────────────────────
# Both properties below are ones the DMG quietly depends on and that a broken
# build satisfies right up until it reaches somebody else's Mac.
for bin in "$HOST_BIN" "$CLIENT_BIN"; do
  archs="$(lipo -archs "$BUILD/$bin")"
  for want in arm64 x86_64; do
    grep -qw "$want" <<<"$archs" || {
      echo "$bin is missing the $want slice (has: $archs) -- an Intel Mac could not run this." >&2
      exit 1; }
  done
  # Every slice must declare the ScreenCaptureKit-era minimum. vtool reports one
  # block per architecture, so check them all rather than the first.
  # Match `minos` EXACTLY: the same block also carries the linker's own
  # `version 1267.0`, and a looser pattern reads that as the deployment target
  # and fails a perfectly good build.
  # NB: plain while-read, not `mapfile` -- macOS ships bash 3.2, where mapfile
  # does not exist. CI runners have bash 5 and would never have caught it.
  n_minos=0
  while read -r got; do
    n_minos=$((n_minos + 1))
    [[ "$got" == "$MIN_OS" ]] || {
      echo "$bin declares minos $got, expected $MIN_OS -- MACOSX_DEPLOYMENT_TARGET was lost." >&2
      exit 1; }
  done < <(vtool -show-build "$BUILD/$bin" | awk '$1 == "minos" {print $2}')
  [[ "$n_minos" -eq "${#ARCHES[@]}" ]] || {
    echo "$bin: expected ${#ARCHES[@]} minos records, found $n_minos -- lipo produced something unexpected." >&2
    exit 1; }
done
echo "==> guards passed: universal (arm64 + x86_64), minos $MIN_OS"

# ── Icon ──────────────────────────────────────────────────────────────────────
ICNS="$REPO/crates/host-macos/assets/AppIcon.icns"
[[ -f "$ICNS" ]] || { echo "Missing $ICNS -- run: python3 scripts/make-mac-icns.py" >&2; exit 1; }

# ── Assemble the .app ─────────────────────────────────────────────────────────
DIST="$REPO/dist"; mkdir -p "$DIST"
STAGE="$REPO/target/macos-stage"
APP="$STAGE/$APP_NAME.app"
rm -rf "$STAGE"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BUILD/$HOST_BIN" "$APP/Contents/MacOS/$HOST_BIN"
# The client rides inside the bundle rather than getting its own .app, matching
# what installer/universal-screens.iss does on Windows and for the same stated
# reason: it takes a host address on the command line, so a bare double-click
# would only ever fail to reach 127.0.0.1. README.txt says how to run it.
cp "$BUILD/$CLIENT_BIN" "$APP/Contents/Resources/$CLIENT_BIN"
cp "$ICNS" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>$HOST_BIN</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>$MIN_OS</string>
  <key>NSHighResolutionCapable</key><true/>
  <!-- Not an agent: the host has a real window and belongs in the Dock. -->
  <key>LSUIElement</key><false/>
  <!-- macOS 15+ gates local networking behind this prompt. Without the usage
       string the system denies it silently and LAN discovery just finds nothing.
       The service types match extender-discovery's MDNS_SERVICE_TYPE and the
       legacy name the iOS client still browses for. -->
  <key>NSLocalNetworkUsageDescription</key>
  <string>Universal Screens needs local network access so phones and laptops on the same Wi-Fi can find and connect to this host.</string>
  <key>NSBonjourServices</key>
  <array>
    <string>_usscreens._tcp</string>
    <string>_extender._tcp</string>
  </array>
</dict>
</plist>
PLIST

cp "$REPO/LICENSE" "$STAGE/LICENSE.txt"
sed -e "s/{VERSION}/$VERSION/g" "$REPO/installer/README-macos.txt" > "$STAGE/README.txt"

# ⚠️ Ad-hoc signature, not decoration. macOS REFUSES to execute an unsigned
# arm64 binary outright -- it is killed at exec, not merely warned about. This is
# what makes the app launch at all on Apple Silicon. It is not notarisation and
# Gatekeeper will still challenge a downloaded copy; README.txt covers that.
codesign --force --deep --sign - --timestamp=none "$APP"
codesign --verify --strict "$APP" || { echo "codesign verification failed" >&2; exit 1; }
echo "==> ad-hoc signed and verified"

# ── DMG ───────────────────────────────────────────────────────────────────────
ln -s /Applications "$STAGE/Applications"
DMG="$DIST/UniversalScreens-$VERSION.dmg"
rm -f "$DMG"
hdiutil create -volname "$APP_NAME $VERSION" -srcfolder "$STAGE" \
               -ov -format UDZO -quiet "$DMG"

shasum -a 256 "$DMG" | awk -v n="$(basename "$DMG")" '{print $1"  "n}' > "$DMG.sha256"

echo
echo "App: $APP"
echo "DMG: $DMG"
echo "Size: $(du -h "$DMG" | cut -f1)"
echo "SHA256: $(awk '{print $1}' "$DMG.sha256")"
