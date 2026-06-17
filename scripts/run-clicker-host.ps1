#!/usr/bin/env pwsh
# Run the Windows clicker host on the LAN for untethered (Wi-Fi) use: open the
# Windows firewall for its port, print the laptop's IP, then launch the host.
#
# Over a USB cable you don't need this — use `adb reverse tcp:9000 tcp:9000` and
# connect the phone to 127.0.0.1:9000 instead.
#
# The phone and laptop must be on the SAME, PRIVATE network (a phone hotspot or a
# home router). Guest / public Wi-Fi usually isolates clients, so this won't work
# there regardless of the firewall.
#
# Usage:  ./scripts/run-clicker-host.ps1 [-Port 9000]

param([int]$Port = 9000)

$ErrorActionPreference = 'Stop'
$ruleName = "Universal Screens clicker (TCP $Port)"
$addRule = "New-NetFirewallRule -DisplayName '$ruleName' -Direction Inbound -Action Allow -Protocol TCP -LocalPort $Port -Profile Private,Domain | Out-Null"

# Add the inbound firewall rule once (needs admin; relaunches elevated if needed).
if (-not (Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue)) {
    Write-Host "Adding firewall rule '$ruleName' (may prompt for admin)..."
    try {
        Invoke-Expression $addRule
    } catch {
        Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile', '-Command', $addRule
    }
}

# Show the LAN IPv4 addresses so you know what to type on the phone.
Write-Host "Connect the phone to <ip>:$Port — LAN addresses:"
Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -notlike '127.*' -and $_.IPAddress -notlike '169.254.*' } |
    Select-Object IPAddress, InterfaceAlias | Format-Table -AutoSize

# Launch the host (release build for lower latency).
cargo run --release -p extender-host-windows -- "0.0.0.0:$Port"
