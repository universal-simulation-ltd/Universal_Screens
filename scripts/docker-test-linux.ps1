# Run the Linux host's tests from a Windows box, in a container.
#
# The twin of scripts/docker-test-linux.sh - see that file for what a container
# does and does not prove. Docker Desktop must be running.
$ErrorActionPreference = 'Stop'

$repo  = Split-Path -Parent $PSScriptRoot
$image = 'screens-linux-test'

docker info *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Error 'Docker is not running. Start Docker Desktop and try again.'
    exit 1
}

# ⚠️ Written to a temp file rather than piped on stdin: PowerShell's here-string
# reaches `docker build -f -` with CRLF line endings, and a Dockerfile whose
# RUN lines end in CR fails inside the container with a shell error that names
# a command nobody typed.
$dockerfile = Join-Path ([System.IO.Path]::GetTempPath()) 'screens-linux-test.Dockerfile'
$content = @'
FROM rust:1-bookworm
RUN apt-get update -qq && apt-get install -y -qq \
      pkg-config libx11-dev libxcursor-dev libxrandr-dev libxi-dev \
      libgl1-mesa-dev libxkbcommon-dev libwayland-dev \
      build-essential nasm \
      xvfb xauth x11-xserver-utils x11-utils \
      xserver-xorg-core xserver-xorg-video-dummy \
    && rm -rf /var/lib/apt/lists/*
'@
[System.IO.File]::WriteAllText($dockerfile, ($content -replace "`r`n", "`n"))

Write-Host "==> Building $image"
docker build -t $image -f $dockerfile $repo | Out-Null
if ($LASTEXITCODE -ne 0) { Write-Error 'docker build failed'; exit 1 }

docker volume create screens-cargo-registry | Out-Null
docker volume create screens-target | Out-Null

docker run --rm -it `
    -v "${repo}:/src" -w /src `
    -v screens-cargo-registry:/usr/local/cargo/registry `
    -v screens-target:/target `
    -e CARGO_TARGET_DIR=/target `
    $image bash -c 'scripts/test-linux-x11.sh'
exit $LASTEXITCODE
