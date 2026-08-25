#!/usr/bin/env bash
#
# Build the Universal Screens Linux host as an AppImage.
#
# Output lands in dist/UniversalScreens-<version>-x86_64.AppImage — a single
# executable file with no installer, no admin rights and no package manager,
# which is the closest Linux equivalent of what the Windows .exe and the macOS
# .dmg already do. Nothing is signed; that is standing UNI·SIM policy.
#
# Usage:
#   ./scripts/build-appimage.sh              # full build
#   ./scripts/build-appimage.sh --skip-build # repackage what's already compiled
#   ./scripts/build-appimage.sh --version X  # override the stamped version
#
# ⚠️ NOT Flatpak. A Flatpak is sandboxed, and the sandbox forces the XDG portal
# path for input — which is exactly the capability this host exists to provide,
# and which uinput deliberately bypasses so one implementation covers X11 and
# every Wayland compositor. See docs/LINUX-HOST.md §5.
#
# ⚠️ The AppImage does NOT install the udev rule. It cannot: an AppImage never
# runs an install step, and the rule needs root. The host detects the missing
# permission at startup and shows the three commands — see
# installer/99-universal-screens-uinput.rules.
#
# There is no Windows twin of this script and cannot be: it links against Linux
# system libraries and produces a Linux binary format. scripts/build-installer.ps1
# and scripts/build-app-macos.sh are the other two sides of the same job.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

APP_NAME="Universal Screens"
HOST_BIN="extender-host-linux"
ARCH="x86_64"

SKIP_BUILD=0
VERSION=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --version)    VERSION="${2:-}"; shift 2 ;;
    -h|--help)    sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$VERSION" ]]; then
  VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
fi
[[ -n "$VERSION" ]] || { echo "could not determine the version" >&2; exit 1; }

# --- build ------------------------------------------------------------------

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "==> building $HOST_BIN $VERSION (release)"
  cargo build --release -p extender-host-linux
fi

# Honour CARGO_TARGET_DIR: CI caches set it, and so does building in a container
# where the repo is a bind mount. Assuming ./target silently packages a stale
# binary, or none at all.
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
BIN="$TARGET_DIR/release/$HOST_BIN"
[[ -x "$BIN" ]] || { echo "missing $BIN — run without --skip-build" >&2; exit 1; }

# --- AppDir -----------------------------------------------------------------

APPDIR="$(mktemp -d)/UniversalScreens.AppDir"
trap 'rm -rf "$(dirname "$APPDIR")"' EXIT
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
         "$APPDIR/usr/share/doc/universal-screens"

cp "$BIN" "$APPDIR/usr/bin/$HOST_BIN"
cp crates/host-linux/assets/app-icon.png \
   "$APPDIR/usr/share/icons/hicolor/256x256/apps/universal-screens.png"
# Ship the udev rule inside the image so the GUI's instructions can point at a
# real file the user already has, rather than asking them to retype it.
cp installer/99-universal-screens-uinput.rules "$APPDIR/usr/share/doc/universal-screens/"

# AppImage looks for these two at the AppDir root, by these exact names.
cp "$APPDIR/usr/share/icons/hicolor/256x256/apps/universal-screens.png" \
   "$APPDIR/universal-screens.png"

cat > "$APPDIR/usr/share/applications/universal-screens.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$APP_NAME
Comment=Use your phone as a presentation clicker or trackpad
Exec=$HOST_BIN
Icon=universal-screens
Categories=Utility;RemoteAccess;
Terminal=false
DESKTOP
cp "$APPDIR/usr/share/applications/universal-screens.desktop" "$APPDIR/"

# AppRun is what the AppImage actually executes.
cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/extender-host-linux" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

# --- package ----------------------------------------------------------------

TOOL="$(command -v appimagetool || true)"
if [[ -z "$TOOL" ]]; then
  echo "==> fetching appimagetool"
  TOOL="$(dirname "$APPDIR")/appimagetool"
  curl -fsSL -o "$TOOL" \
    "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-${ARCH}.AppImage"
  chmod +x "$TOOL"
fi

mkdir -p dist
OUT="dist/UniversalScreens-${VERSION}-${ARCH}.AppImage"
echo "==> packaging $OUT"
# ⚠️ --appimage-extract-and-run: appimagetool is itself an AppImage, and mounting
# one needs FUSE. CI runners and most containers have no FUSE, where the plain
# invocation fails with a message about libfuse that has nothing to do with this
# build. Extracting instead works everywhere and costs a second.
ARCH="$ARCH" "$TOOL" --appimage-extract-and-run --no-appstream "$APPDIR" "$OUT"

sha256sum "$OUT" | tee "$OUT.sha256"
echo
echo "Built $OUT"
echo "Run it directly: chmod +x $OUT && ./$OUT"
echo "Input will not work until the udev rule is installed — the app says so on startup."
