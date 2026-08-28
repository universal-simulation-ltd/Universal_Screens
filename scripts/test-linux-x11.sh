#!/usr/bin/env bash
# Run the Linux host's live-X-server tests against BOTH servers they need.
#
# Runs *on* Linux — in CI, on a Linux desktop, or inside the container that
# `docker-test-linux.sh` / `.ps1` start on a Mac or Windows box.
#
# ⚠️ Two servers, not one, and the split is not arbitrary:
#
#   - **Xvfb** for capture, the window picker and the mirror. It is a real X
#     server, so those tests genuinely run rather than being simulated.
#   - **Xorg + the `dummy` driver** for the second screen, because Xvfb's
#     framebuffer CANNOT GROW: its RandR size range reports maximum == current,
#     so `RRSetScreenSize` is refused. `vdisplay.rs` does nothing else.
#
# ⚠️ Both phases set a `SCREENS_REQUIRE_*` variable. Without them a missing
# server makes the tests SKIP and the run still reports success — which is
# indistinguishable from them passing, and is how a suite quietly stops
# exercising the thing it exists to check.
# Usage: scripts/test-linux-x11.sh [all|xvfb|xorg]   (default: all)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$here"

cleanup() { [ -n "${xorg_pid:-}" ] && kill "$xorg_pid" 2>/dev/null || true; }
trap cleanup EXIT

phase="${1:-all}"

if [ "$phase" = all ] || [ "$phase" = xvfb ]; then
    echo "==> Xvfb: capture, window picker, mirror"
    SCREENS_REQUIRE_X11=1 xvfb-run -a --server-args="-screen 0 1280x800x24" \
        cargo test -p extender-host-linux
    echo
fi

[ "$phase" = xvfb ] && exit 0

echo "==> Xorg (dummy driver): the second screen"
if ! command -v Xorg >/dev/null 2>&1; then
    echo "!! Xorg is not installed - install xserver-xorg-core and" >&2
    echo "!! xserver-xorg-video-dummy, or the second screen goes untested." >&2
    exit 1
fi

# ⚠️ Only the SERVER needs root, and only off a console. Ubuntu wraps `Xorg` in
# `Xorg.wrap`, which refuses to start for a user with no console session - a CI
# runner's. `cargo` deliberately stays as the invoking user: running the tests as
# root instead would put their build artifacts and registry cache somewhere the
# next step cannot use, which on a cached CI job is a silent full rebuild.
xorg=(Xorg)
[ "$(id -u)" -ne 0 ] && xorg=(sudo Xorg)

# :77 rather than :99, so the Xvfb phase's display and any nested server a
# developer is already running are left alone.
"${xorg[@]}" -config "$here/scripts/xorg-dummy.conf" \
    -logfile /tmp/xorg-dummy.log -noreset :77 &
xorg_pid=$!
for _ in $(seq 1 20); do
    DISPLAY=:77 xdpyinfo >/dev/null 2>&1 && break
    sleep 0.5
done
if ! DISPLAY=:77 xdpyinfo >/dev/null 2>&1; then
    echo "!! Xorg did not start; its log follows." >&2
    tail -30 /tmp/xorg-dummy.log >&2 || true
    exit 1
fi
DISPLAY=:77 xrandr --query | head -3

DISPLAY=:77 SCREENS_REQUIRE_X11=1 SCREENS_REQUIRE_RANDR=1 \
    cargo test -p extender-host-linux

echo
echo "All Linux host tests ran against a live X server."
