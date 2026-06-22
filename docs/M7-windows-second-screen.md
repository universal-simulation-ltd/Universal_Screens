# M7 — Phone as a second screen for a Windows PC (plan)

**Status:** planning / not started. **Goal:** let the phone act as a real
*extended* display for a Windows laptop — e.g. watch a webinar on the phone while
working on the laptop. This is the "extend" use case (different content on the
phone than on the laptop), not mirroring.

## Where we are today

- The **macOS host** (`crates/host`) already does extend: it creates a
  `CGVirtualDisplay`, captures it with ScreenCaptureKit, encodes with
  VideoToolbox, and streams. (M3.)
- The **Windows host** (`extender-host-windows`) is **input-only** — a clicker.
  It has no virtual display, no capture, no encoder, and ignores `CaptureMode`.
- The **phone** has a receiving path (`StreamScreen` + `VideoDecoder`/MediaCodec
  on Android; a VideoToolbox scaffold on iOS) and the **Viewer** mode (display,
  no input) — which is exactly the mode a second screen needs. It's wired to the
  protocol but untested against a real stream.
- The **protocol** already models everything downstream: `Message::StreamStart`
  (geometry + codec + H.264 parameter sets) and `Message::Frame` (Annex-B NALs).
  No protocol changes are needed.

So the missing piece is entirely **host-side on Windows**: make a virtual
display, capture it, encode it, and stream it through the existing protocol.

## The three host-side pieces

### 1. A virtual display (the hard part)

Windows has no public API to add a fake monitor. The supported route is an
**Indirect Display Driver (IDD)** built on Microsoft's **IddCx** framework — a
*user-mode* display driver that registers a virtual monitor the OS can extend to.

Options, cheapest → dearest:
- **Bundle an existing open-source IDD.** Microsoft ships an `IddSampleDriver`,
  and there are maintained MIT-licensed forks (e.g. community "Virtual Display
  Driver" projects) that add selectable resolutions. The host app would install/
  enable it (admin, `pnputil`), then capture the monitor it creates. *Verify:
  exact project, licence, and whether it exposes a phone-matching resolution.*
- **Fork/build a minimal IDD ourselves** (WDK + IddCx) to advertise exactly the
  phone's resolution and a clean device name. More control, more work.

**The real cost here is driver signing & install, not the C++:**
- Dev / personal use: enable **test-signing** (and the user accepts an unsigned/
  test-signed driver). Workable on your own machine.
- Distribution: needs an **EV code-signing cert + Microsoft attestation
  signing** of the driver package, plus a proper installer. This is the single
  biggest external dependency of the whole milestone.

(There's no driver-free way to truly *extend* on Windows. "Projecting to this PC"
/ Miracast is the reverse direction and can't target an Android phone reliably.)

### 2. Capture

- **Windows.Graphics.Capture (WGC)** — modern WinRT API; can capture a specific
  `HMONITOR` (so we capture the *virtual* monitor, not the real one). Clean,
  hardware-friendly. Available through the `windows` crate.
- Alternative: **DXGI Desktop Duplication** (per-output). Older, a bit more
  manual. WGC is the better fit.

### 3. Encode + stream

- **Media Foundation H.264 encoder** (the Windows analogue of the macOS host's
  VideoToolbox): feed captured frames, get H.264, extract SPS/PPS for
  `StreamStart`, emit `Frame`s. Hardware-accelerated where the GPU supports it.
- Reuse the **existing protocol + network loop** (mirror `crates/host`'s
  `serve()` structure). The phone already decodes Annex-B via MediaCodec.

All of §2–§3 are normal Rust-on-Windows work (via the `windows` crate),
self-contained and testable on this laptop without a phone (capture+encode to a
file first, then stream).

## Phone side

Minimal. The **Viewer** mode already exists; it needs on-device testing and
likely small decoder fixes (MediaCodec pacing, keyframe handling). For watching a
webinar — one-way, latency-tolerant — H.264 streaming quality is fine.

## Proposed increments

1. **M7a — Streaming host, mirror first (no driver).** Capture the *primary*
   display (WGC) → Media Foundation H.264 → stream via the existing protocol.
   Prove the whole video pipeline on Windows + fix the Android Viewer. *Mirrors
   the laptop (same content) — not the end goal, but de-risks §2–§3 entirely and
   is fully testable here.*
2. **M7b — Virtual display via a bundled IDD.** Integrate an open-source IDD,
   add install/enable + teardown, and point the capture at the virtual monitor
   instead of the primary. Now it truly *extends* → drag the webinar to the phone.
3. **M7c — Sizing + polish.** Match the virtual display to the phone's
   resolution/orientation, handle reconnect, hide the virtual monitor when no
   client is connected.
4. **M7d — Distribution (optional).** Driver signing + installer, if this ships
   beyond your own machine.

## Effort & risk (rough)

| Increment | Effort | Risk |
|---|---|---|
| M7a streaming host (mirror) + Viewer fixes | medium (days) | low — standard `windows`-crate work, testable here |
| M7b virtual display (bundled IDD) | medium–high | **high** — driver build/sign/install is the crux |
| M7c sizing/polish | low–medium | low |
| M7d signing for distribution | medium | high (EV cert + attestation, external) |

## Recommendation

Do **M7a first** — it's the testable, low-risk 80% (capture → encode → stream →
phone shows it), and it immediately proves the pipeline on Windows even though it
only mirrors. Then decide on **M7b** (the IDD) with eyes open about the driver
signing/install friction. If the goal is just *you*, on *your* laptop, test-signed
driver + M7a–M7c is achievable; shipping it widely (M7d) is a real project.

## Decisions needed before building

1. **Scope:** personal/dev (test-signed driver OK) or distributable (signing)?
2. **IDD:** bundle an existing open-source driver, or build a minimal one?
3. **Order:** start with M7a (mirror, testable now), or go straight for the
   virtual display?
