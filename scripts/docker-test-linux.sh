#!/usr/bin/env bash
# Run the Linux host's tests from a Mac, in a container.
#
# ⚠️ There is no Linux machine in this workspace (docs/LINUX-HOST.md §8), and
# `crates/host-linux` cannot even be compiled anywhere else: it uses uinput and
# X11 unconditionally. So this is how the Linux half is checked before a push,
# rather than discovering it in CI.
#
# What a container CAN prove: the crate compiles and links on real Linux, and
# every test that needs a live X server runs against one - including the second
# screen, which needs a resizable framebuffer that Xvfb does not have (see
# scripts/xorg-dummy.conf).
#
# What it still cannot: uinput injection (no /dev/uinput), the GUI, and anything
# about a real desktop's compositor, a multi-monitor layout or a real phone.
#
# Docker Desktop must be running. The Windows twin is scripts/docker-test-linux.ps1.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="screens-linux-test"

docker info >/dev/null 2>&1 || {
    echo "!! Docker is not running. Start Docker Desktop and try again." >&2
    exit 1
}

echo "==> Building $image"
docker build -t "$image" -f - "$repo" >/dev/null <<'DOCKERFILE'
FROM rust:1-bookworm
# The same list as .github/workflows/tests.yml, plus the Xorg dummy driver the
# second-screen tests need.
RUN apt-get update -qq && apt-get install -y -qq \
      pkg-config libx11-dev libxcursor-dev libxrandr-dev libxi-dev \
      libgl1-mesa-dev libxkbcommon-dev libwayland-dev \
      build-essential nasm \
      xvfb xauth x11-xserver-utils x11-utils \
      xserver-xorg-core xserver-xorg-video-dummy \
    && rm -rf /var/lib/apt/lists/*
DOCKERFILE

# Named volumes for the registry and the build directory: without them every run
# recompiles ~400 crates, which takes long enough that nobody runs this twice.
# CARGO_TARGET_DIR keeps the container's artifacts out of the host's target/,
# where they would fight the Mac build over the same directory.
docker volume create screens-cargo-registry >/dev/null
docker volume create screens-target >/dev/null

exec docker run --rm \
    -v "$repo":/src -w /src \
    -v screens-cargo-registry:/usr/local/cargo/registry \
    -v screens-target:/target \
    -e CARGO_TARGET_DIR=/target \
    "$image" bash -c 'scripts/test-linux-x11.sh'
