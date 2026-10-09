#!/usr/bin/env bash
# Publish the browser client (apps/web) to the portal, where it is served at
#   https://opensource.unisim.co.uk/screens/app/
#
# The portal (backoffice/opensource-portal) is a static-assets Worker with no
# build step, so the client goes in as BUILT files: this script rebuilds the
# WASM shim in release mode, then copies exactly what the page loads into the
# portal's public/screens/app/. Commit and push the portal afterwards; its own
# workflow deploys on push to main.
#
# Why a copy rather than a build in CI: `apps/web/pkg/` is git-ignored, and the
# portal's deploy runner has no Rust/wasm-pack. A cross-repo CI job would need a
# token that can push to the portal; until one exists, this is the pipeline.
#
# Usage (from anywhere):
#   scripts/publish-web-client.sh [PORTAL_PUBLIC_SCREENS_APP_DIR]
# Default target: ../../backoffice/opensource-portal/public/screens/app
# (the umbrella checkout's layout). Needs: rustup target wasm32-unknown-unknown,
# wasm-pack.

set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-$REPO/../../backoffice/opensource-portal/public/screens/app}"

if [ ! -d "$(dirname "$DEST")" ]; then
  echo "error: $(dirname "$DEST") does not exist — pass the portal's public/screens/app path" >&2
  exit 1
fi

cd "$REPO"
# Release, not --dev: ~290 KB of WASM instead of ~1.3 MB.
wasm-pack build crates/protocol-wasm --release --target web \
  --out-dir ../../apps/web/pkg --out-name extender_protocol

# ⚠️ A stale pkg/ silently tests (and here, ships) the old bindings — the shim
# is checked against canonical Rust bytes before anything is copied.
node apps/web/verify-wasm.mjs

rm -rf "$DEST"
mkdir -p "$DEST/src" "$DEST/pkg" "$DEST/knowledge"
cp apps/web/index.html "$DEST/"
# The page's modules only — not the *.test.mjs files or the dev server.
cp apps/web/src/*.js "$DEST/src/"
cp apps/web/pkg/extender_protocol.js apps/web/pkg/extender_protocol_bg.wasm "$DEST/pkg/"
cp apps/web/knowledge/*.md "$DEST/knowledge/"

SHA="$(git rev-parse --short HEAD)"
DIRTY=""; git diff --quiet -- apps/web crates || DIRTY=" (+ uncommitted changes)"
cat > "$DEST/BUILD.txt" <<EOF
Universal Screens browser client, copied by Universal_Screens/scripts/publish-web-client.sh
Source: universal-simulation-ltd/Universal_Screens @ $SHA$DIRTY (apps/web + crates/protocol-wasm)
Built: $(date -u +%Y-%m-%dT%H:%MZ)
Do not edit these files here — change apps/web in Universal_Screens and re-run the script.
EOF

echo "published apps/web @ $SHA$DIRTY -> $DEST"
