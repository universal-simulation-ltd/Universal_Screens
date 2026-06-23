# Universal Screens — Dev Preview Guide

A quick-start for testers who want to try the current build without building from source.

---

## What you need

| | |
|---|---|
| **Windows PC** | The host — shares its screen, injects input |
| **Android phone** | The client — scans QR, controls the PC |
| **Same local network** | Home Wi-Fi, a phone hotspot the PC joins, or the PC's Mobile Hotspot |

---

## 1. Start the Windows host

Run the pre-built binary directly — no install needed:

```
Universal_ScreenExtender\target\release\extender-host-windows.exe
```

The window opens and hosting starts automatically. You'll see:

- A **QR code** — scan this from the Android app to connect
- The host's **local IP address** and **4-digit PIN** (under "More details")
- **Network** and Wi-Fi password if applicable
- A **"Nearby"** section (appears within ~2s once the Android app is open on the same network)

> **Firewall prompt:** Windows may ask whether to allow the app through the firewall the first time. Click **Allow** — this is needed for phones to reach the host.

---

## 2. Install the Android app

With the phone connected via USB and USB debugging enabled:

```powershell
adb install -r apps\android\app\build\outputs\apk\debug\app-debug.apk
```

Or push and launch in one step:

```powershell
adb install -r apps\android\app\build\outputs\apk\debug\app-debug.apk
adb shell am start -n "com.universalsim.extender/com.universalsim.extender.MainActivity"
```

---

## 3. Connect

There are three ways:

### A — Scan the QR (recommended)
Tap **Scan to connect** in the Android app and point the camera at the QR on the Windows host. The app joins the host's Wi-Fi (if shown) and connects in one step.

### B — LAN discovery (same network, no camera needed)
If both devices are on the same network, the Android app shows a **"Nearby"** section automatically within ~2s. Enter the 4-digit PIN shown on the Windows host and tap **Connect**.

### C — Manual entry
Tap **Advanced** in the Android app, type `ip:port` (shown under "More details" on the host) and the PIN, then tap **Connect**.

---

## 4. Pick a mode

After connecting you'll be asked what you want to do:

| Mode | What it does |
|---|---|
| **Clicker** | Presentation remote — next/back, blank, slide previews |
| **Trackpad** | Wireless trackpad and mouse buttons |
| **Mirror** | See the PC screen on the phone (read-only) |
| **Remote control** | Mirror + touch controls the PC |
| **Second screen** | Phone becomes a second display (needs a virtual display driver — see [SECOND-SCREEN.md](SECOND-SCREEN.md)) |

---

## LAN discovery notes

Discovery uses **UDP multicast** (`224.0.0.251:9001`). This works on:

- Home Wi-Fi ✅
- PC's Windows Mobile Hotspot ✅ (Settings → Network → Mobile Hotspot)
- Phone hotspot — usually ✅ but some Android models filter multicast between clients

If **Nearby** doesn't appear within ~6s, use **Scan QR** or **Manual entry** instead.

---

## Known limitations (dev preview)

- Traffic is **unencrypted** — use on trusted networks only. The PIN gates access but doesn't encrypt the connection.
- **Second screen** requires a virtual display driver (IddCx) — not included in this preview.
- **iOS** client is not yet built.
- Discovery is currently **one-way**: Windows beacons, Android listens. Two Windows hosts will also see each other.

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| "Connection failed" | Check both devices are on the same network; check the PIN matches |
| QR goes to a white page | Wait a few seconds — Cloudflare propagation delay after a recent deploy |
| No "Nearby" on phone | Hotspot may filter multicast — use QR scan or manual entry |
| Windows Firewall blocks connections | In the Windows app, expand "More details" → "Allow through firewall" |
| App not updating after reinstall | Check the version in the footer (e.g. `v0.1.0`) matches the build |
