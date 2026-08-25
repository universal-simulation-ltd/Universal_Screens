#!/usr/bin/env bash
# Launch the Universal Screens host — the macOS one on a Mac, the Linux one on
# Linux. This is a Rust workspace, not a web app; see docs/WINDOWS-CLIENT.md for
# the Windows-side runbook.
#
# Host = the machine that gives up a screen (or, on Linux, just its keyboard and
# mouse). It waits for a client to connect on TCP :9000.
#
# Usage:  ./scripts/preview.sh                   (listens on 0.0.0.0:9000)
#         ./scripts/preview.sh 0.0.0.0:9000 2560x1440   (macOS: force a size)
#
# macOS requires the Rust toolchain plus Screen Recording + Accessibility
# permissions (System Settings → Privacy & Security); cargo builds native
# frameworks (ScreenCaptureKit, VideoToolbox) and the first build takes minutes.
#
# Linux requires the Rust toolchain, the X11/Wayland dev packages listed in
# docs/LINUX-APP.md, and write access to /dev/uinput — the host warns on startup
# when it doesn't have it, because the symptom otherwise is a client that
# connects and a desktop that never moves.

set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OS="$(uname -s)"
case "$OS" in
  Darwin) HOST_PKG="extender-host" ;;
  Linux)  HOST_PKG="extender-host-linux" ;;
  *)
    echo "ERROR: no host crate for this OS ($OS)."
    echo "       On Windows run scripts/preview.ps1 — see docs/WINDOWS-CLIENT.md."
    exit 1
    ;;
esac

if ! command -v cargo >/dev/null 2>&1; then
  echo "ERROR: cargo not found on PATH. Install Rust via https://rustup.rs"
  exit 1
fi

echo "Universal Screens ($HOST_PKG) → listening on 0.0.0.0:9000"
if [[ "$OS" == "Darwin" ]]; then
  echo "Find this Mac's LAN IP with: ipconfig getifaddr en0   (or en1 for Wi-Fi)"
else
  echo "Find this machine's LAN IP with: ip -4 -o addr show scope global"
  echo "This host is input-only: clicker and trackpad, no screen mirroring."
fi
echo "Then on the client machine, run: cargo run --release -p extender-client -- <host-ip>:9000"
echo ""
exec cargo run --release -p "$HOST_PKG" -- "$@"
