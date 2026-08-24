<#
.SYNOPSIS
  Build the Universal Screens Windows installer.

.DESCRIPTION
  Compiles the release binaries, regenerates the app icon, and packages both
  into a single UniversalScreens-Setup-<version>.exe with Inno Setup.

  The binaries are built with a STATIC CRT. That is what makes the install two
  self-contained files with no Visual C++ redistributable to chase -- without
  it the .exe imports VCRUNTIME140.dll and refuses to start on a machine that
  has never had a VC++ redist on it. If you drop the flag, you have to start
  shipping the redist.

  Output lands in dist/. Nothing is signed -- see docs/WINDOWS-INSTALLER.md.

.PARAMETER SkipBuild
  Package whatever is already in target/x86_64-pc-windows-msvc/release. Fast
  loop when you are only editing the .iss.

.PARAMETER Version
  Override the version stamped on the installer. Defaults to the workspace
  version in Cargo.toml.

.EXAMPLE
  .\scripts\build-installer.ps1
  .\scripts\build-installer.ps1 -SkipBuild
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $repo
try {
    $target = 'x86_64-pc-windows-msvc'

    # ── Version: single source of truth is [workspace.package] in Cargo.toml ──
    if (-not $Version) {
        $toml = Get-Content (Join-Path $repo 'Cargo.toml') -Raw
        if ($toml -notmatch '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') {
            throw "Couldn't read version from [workspace.package] in Cargo.toml"
        }
        $Version = $Matches[1]
    }
    Write-Host "Universal Screens $Version" -ForegroundColor Green

    # ── Toolchain checks, up front, with the fix in the message ──────────────
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo not found on PATH. Install Rust via https://rustup.rs"
    }
    if (-not (Get-Command nasm -ErrorAction SilentlyContinue)) {
        # openh264-sys2 assembles from source; without NASM the build dies deep
        # in a C build script with a much less obvious error.
        throw "nasm not found on PATH -- the openh264 build needs it. Install with: winget install NASM.NASM (then open a new terminal)"
    }

    $iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($iscc) {
        $iscc = $iscc.Source
    } else {
        $iscc = @(
            "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
            "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
            "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
        ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    if (-not $iscc) {
        throw "Inno Setup's ISCC.exe not found. Install with: winget install JRSoftware.InnoSetup"
    }

    # ── Icon: regenerated from the shared artwork so it can't drift ──────────
    if (Get-Command python -ErrorAction SilentlyContinue) {
        Write-Host "==> app icon" -ForegroundColor Cyan
        python (Join-Path $repo 'scripts\make-win-ico.py')
    } else {
        Write-Warning "python not found -- reusing the committed app-icon.ico"
    }
    $ico = Join-Path $repo 'crates\host-windows\assets\app-icon.ico'
    if (-not (Test-Path $ico)) { throw "Missing $ico -- run: python scripts/make-win-ico.py" }

    # ── Binaries ─────────────────────────────────────────────────────────────
    $outDir = Join-Path $repo "target\$target\release"
    if (-not $SkipBuild) {
        Write-Host "==> cargo build --release (static CRT)" -ForegroundColor Cyan
        $env:RUSTFLAGS = '-C target-feature=+crt-static'
        try {
            cargo build --release --target $target -p extender-host-windows -p extender-client
            if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
        } finally {
            Remove-Item Env:\RUSTFLAGS -ErrorAction SilentlyContinue
        }
    }

    foreach ($exe in 'extender-host-windows.exe', 'extender-client.exe') {
        $path = Join-Path $outDir $exe
        if (-not (Test-Path $path)) { throw "Missing $path -- build it before packaging (drop -SkipBuild)" }
        # Guard the property the installer quietly depends on. A dynamically
        # linked build packages perfectly happily and then fails to launch on
        # someone else's machine, which is the worst place to find out.
        $bytes = [System.IO.File]::ReadAllBytes($path)
        $ascii = [System.Text.Encoding]::ASCII.GetString($bytes)
        if ($ascii -match 'VCRUNTIME\d+\.dll') {
            throw "$exe imports the VC++ runtime -- it was not built with a static CRT. Rebuild without -SkipBuild."
        }
    }

    # ── Package ──────────────────────────────────────────────────────────────
    $dist = Join-Path $repo 'dist'
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    Write-Host "==> ISCC" -ForegroundColor Cyan
    & $iscc `
        "/DAppVersion=$Version" `
        "/DSourceDir=$repo" `
        "/DOutputDir=$dist" `
        (Join-Path $repo 'installer\universal-screens.iss')
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed ($LASTEXITCODE)" }

    $setup = Join-Path $dist "UniversalScreens-Setup-$Version.exe"
    $hash = (Get-FileHash $setup -Algorithm SHA256).Hash
    "$hash  UniversalScreens-Setup-$Version.exe" |
        Set-Content -Path (Join-Path $dist "UniversalScreens-Setup-$Version.exe.sha256") -Encoding ascii

    Write-Host ""
    Write-Host "Installer: $setup" -ForegroundColor Green
    Write-Host ("Size:      {0:N1} MB" -f ((Get-Item $setup).Length / 1MB))
    Write-Host "SHA256:    $hash"
    Write-Host ""
    Write-Host "Unsigned -- SmartScreen will warn on first run. That is expected." -ForegroundColor Yellow
} finally {
    Pop-Location
}
