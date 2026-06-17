# Launch the Universal Screens client on Windows.
# This is a Rust workspace, not a web app — see docs/WINDOWS-CLIENT.md.
#
# Client = the Windows laptop that acts as a second screen for a Mac. The Mac
# runs the host (preview.sh on macOS) and listens on TCP :9000. This script
# decodes the host's H.264 stream and renders it locally with wgpu.
#
# Usage:  .\scripts\preview.ps1 <mac-ip>[:9000] [--monitor N] [--res N]
#         .\scripts\preview.ps1 192.168.1.42:9000
#         .\scripts\preview.ps1 192.168.1.42:9000 --res 2
#
# Requires: Rust (MSVC toolchain), Visual Studio C++ Build Tools, and NASM on
# PATH (assembles the bundled OpenH264 decoder). See docs/WINDOWS-CLIENT.md §1
# for one-time setup. First build compiles OpenH264 from source — expect a few
# minutes; subsequent runs are instant.

$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Error "cargo not found on PATH. Install Rust via https://rustup.rs"
        exit 1
    }
    if (-not (Get-Command nasm -ErrorAction SilentlyContinue)) {
        Write-Warning "nasm not found on PATH — the openh264-sys2 build will fail."
        Write-Warning "Install with: winget install NASM.NASM   (then open a new terminal)"
    }

    if ($args.Count -lt 1) {
        Write-Host "Usage: .\scripts\preview.ps1 <mac-ip>[:9000] [--monitor N] [--res N]" -ForegroundColor Yellow
        Write-Host ""
        Write-Host "Find the Mac's LAN IP by running on the Mac:" -ForegroundColor Cyan
        Write-Host "  ipconfig getifaddr en0     # Ethernet (try en1 for Wi-Fi)"
        exit 2
    }

    Write-Host "Universal Screens (client) -> connecting to $($args[0])" -ForegroundColor Green
    cargo run --release -p extender-client -- @args
} finally {
    Pop-Location
}
