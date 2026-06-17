#!/usr/bin/env bash
# Launch the Universal Screens host on macOS.
# This is a Rust workspace, not a web app — see docs/WINDOWS-CLIENT.md for the
# Windows-side runbook.
#
# Host = the Mac that gives up its virtual display. It captures + H.264-encodes
# the second screen and waits for a client to connect on TCP :9000.
#
# Usage:  ./scripts/preview.sh                   (listens on 0.0.0.0:9000)
#         ./scripts/preview.sh 0.0.0.0:9000 2560x1440   (force a fixed size)
#
# Requires: Rust toolchain, Screen Recording + Accessibility permissions
# (System Settings → Privacy & Security). Cargo will download deps and build
# native macOS frameworks (ScreenCaptureKit, VideoToolbox, screencapturekit).
# First build takes a few minutes.

set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OS="$(uname -s)"
if [[ "$OS" != "Darwin" ]]; then
  echo "ERROR: extender-host is macOS-only (uses ScreenCaptureKit + VideoToolbox)."
  echo "       Detected OS: $OS"
  echo "       On Windows/Linux, run scripts/preview.ps1 (or invoke cargo directly)"
  echo "       to start the CLIENT instead — see docs/WINDOWS-CLIENT.md."
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "ERROR: cargo not found on PATH. Install Rust via https://rustup.rs"
  exit 1
fi

echo "Universal Screens (host) → listening on 0.0.0.0:9000"
echo "Find this Mac's LAN IP with: ipconfig getifaddr en0   (or en1 for Wi-Fi)"
echo "Then on the client machine, run: cargo run --release -p extender-client -- <mac-ip>:9000"
echo ""
exec cargo run --release -p extender-host -- "$@"
