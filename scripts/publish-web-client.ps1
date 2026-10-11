# Publish the browser client (apps/web) to the portal, where it is served at
#   https://opensource.unisim.co.uk/screens/app/
#
# The Windows twin of publish-web-client.sh; keep the two in step. The portal
# (backoffice/opensource-portal) is a static-assets Worker with no build step,
# so the client goes in as BUILT files: this script rebuilds the WASM shim in
# release mode, then copies exactly what the page loads into the portal's
# public\screens\app\. Commit and push the portal afterwards; its own workflow
# deploys on push to main.
#
# Why a copy rather than a build in CI: apps/web/pkg/ is git-ignored, and the
# portal's deploy runner has no Rust/wasm-pack. A cross-repo CI job would need a
# token that can push to the portal; until one exists, this is the pipeline.
#
# Usage (from anywhere):
#   .\scripts\publish-web-client.ps1 [PORTAL_PUBLIC_SCREENS_APP_DIR]
# Default target: ..\..\backoffice\opensource-portal\public\screens\app
# (the umbrella checkout's layout, where this repo is
# Universal_Apps\Universal_Screens). Needs: rustup target wasm32-unknown-unknown, wasm-pack,
# node.
#
# Note - line endings: with core.autocrlf=true a Windows checkout has CRLF in
# apps/web, and this copies them as they are - the portal then shows a diff on
# every file that is only line endings. Publish from a checkout with
# core.autocrlf=false (or input), or from the Mac.

param([string]$Dest)

$ErrorActionPreference = 'Stop'
$Repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $Dest) {
    $Dest = Join-Path $Repo '..\..\backoffice\opensource-portal\public\screens\app'
}
$Dest = [System.IO.Path]::GetFullPath($Dest)
$Parent = Split-Path -Parent $Dest
if (-not (Test-Path -LiteralPath $Parent -PathType Container)) {
    Write-Error "$Parent does not exist - pass the portal's public\screens\app path"
    exit 1
}
foreach ($tool in 'wasm-pack', 'node', 'git') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        Write-Error "$tool not found on PATH"
        exit 1
    }
}

Push-Location $Repo
try {
    # Release, not --dev: ~290 KB of WASM instead of ~1.3 MB.
    # The remaps keep the builder's home folder out of the public WASM (panic
    # locations name source files), and make two machines' builds closer to equal.
    $CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME '.cargo' }
    $CargoSrc = Join-Path $CargoHome 'registry\src'
    $SavedFlags = $env:RUSTFLAGS
    $env:RUSTFLAGS = "$SavedFlags --remap-path-prefix=$CargoSrc=/cargo/registry/src --remap-path-prefix=$Repo=/Universal_Screens".Trim()
    try {
        wasm-pack build crates/protocol-wasm --release --target web `
            --out-dir ../../apps/web/pkg --out-name extender_protocol
        if ($LASTEXITCODE -ne 0) { throw "wasm-pack failed" }
    } finally {
        $env:RUSTFLAGS = $SavedFlags
    }

    # A stale pkg\ silently tests (and here, ships) the old bindings - the shim
    # is checked against canonical Rust bytes before anything is copied.
    node apps/web/verify-wasm.mjs
    if ($LASTEXITCODE -ne 0) { throw "verify-wasm failed - nothing was copied" }

    if (Test-Path -LiteralPath $Dest) { Remove-Item -LiteralPath $Dest -Recurse -Force }
    foreach ($sub in 'src', 'pkg', 'knowledge') {
        New-Item -ItemType Directory -Force -Path (Join-Path $Dest $sub) | Out-Null
    }
    Copy-Item apps/web/index.html $Dest
    # The page's modules only - not the *.test.mjs files or the dev server.
    Copy-Item apps/web/src/*.js (Join-Path $Dest 'src')
    Copy-Item apps/web/pkg/extender_protocol.js, apps/web/pkg/extender_protocol_bg.wasm (Join-Path $Dest 'pkg')
    Copy-Item apps/web/knowledge/*.md (Join-Path $Dest 'knowledge')

    $Sha = (git rev-parse --short HEAD).Trim()
    git diff --quiet -- apps/web crates
    $Dirty = if ($LASTEXITCODE -ne 0) { ' (+ uncommitted changes)' } else { '' }
    $Built = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm'Z'")
    # LF line endings and no BOM, as the .sh writes. The dash is built from its
    # code point: Windows PowerShell 5.1 reads a BOM-less script as ANSI.
    $Dash = [char]0x2014
    $Text = "Universal Screens browser client, copied by Universal_Screens/scripts/publish-web-client.ps1`n" +
            "Source: universal-simulation-ltd/Universal_Screens @ $Sha$Dirty (apps/web + crates/protocol-wasm)`n" +
            "Built: $Built`n" +
            "Do not edit these files here $Dash change apps/web in Universal_Screens and re-run the script.`n"
    [System.IO.File]::WriteAllText((Join-Path $Dest 'BUILD.txt'), $Text, (New-Object System.Text.UTF8Encoding $false))

    Write-Host "published apps/web @ $Sha$Dirty -> $Dest" -ForegroundColor Green
} finally {
    Pop-Location
}
