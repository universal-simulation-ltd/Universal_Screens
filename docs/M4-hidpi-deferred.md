# M4 — HiDPI/Retina: deferred (notes for a future attempt)

**Status:** deferred. The virtual display is a standard **non-HiDPI** display at the requested resolution. It works reliably; it's just not 2×-crisp when blown up fullscreen on a Retina client.

## What we want

A virtual display that renders at **2× (Retina) density** so captured text is crisp — i.e. logical 1920×1080 backed by a 3840×2160 framebuffer, captured at 3840×2160.

## What we tried (all on macOS 26.2, single machine)

1. `settings.hiDPI = 1` + a logical-size (1920) mode → display came up **@1×**.
2. `settings.hiDPI = 2` + a logical-size mode (KhaosT example shape) → **@1×** (current mode defaulted to 1×; probe showed `scale 1.0`).
3. Mode created at the native backing (3840) + `hiDPI = 1` (force-hidpi shape) → display still defaulted its **current mode to 1×** (`capturing 1920x1080`).
4. Explicit mode selection after creation — `CGDisplayCopyAllDisplayModes` (with `kCGDisplayShowDuplicateLowResolutionModes`) + `CGDisplaySetDisplayMode` to the HiDPI mode: **inconsistent** — one run jumped to an 8K (7680×4320) mode, the next stayed at 1920×1080. Selecting by "closest logical width to requested" still didn't stick.

## Why it's hard

A **standalone** `CGVirtualDisplay` doesn't reliably adopt a HiDPI mode as its *current* mode. The projects that do this dependably (e.g. **force-hidpi**, BetterDisplay) **mirror the virtual display onto a physical display** (`CGConfigureDisplayMirrorOfDisplay`); the mirror is what forces the matching HiDPI mode. Our workflow **captures** the virtual display rather than mirroring it onto a physical one, so that lever doesn't apply.

## Paths forward (if revisited)

- **Mirror trick:** create the HiDPI virtual display, briefly mirror it to a physical display to lock the HiDPI mode, then capture — fiddly and may have side effects.
- **Deeper mode investigation:** log the full `CGDisplayCopyAllDisplayModes` list (point + pixel per mode) to see exactly which HiDPI modes the OS exposes for a standalone VD and whether any `CGDisplaySetDisplayMode` sticks; try the `CGBeginDisplayConfiguration`/`CGConfigureDisplayWithDisplayMode`/`CGCompleteDisplayConfiguration(.permanently)` path instead of `CGDisplaySetDisplayMode`.
- **Client-side super-sampling:** request a larger logical resolution (the `WxH` arg already works) and let the client downscale — more detail without true HiDPI, at higher encode cost.

## What ships instead

Reliable non-HiDPI virtual display at a **configurable resolution** (host arg `WIDTHxHEIGHT`, default 1920×1080). Pick a larger size for more desktop space (e.g. `2560x1440`) — not 2×-crisp, but fully usable. The HiDPI shim/mode-selection code was removed to avoid the 8K-overshoot footgun; this doc + git history are the reference for a future attempt.
