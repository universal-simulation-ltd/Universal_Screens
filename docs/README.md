# Universal Screens — docs

## What this repo is

Universal Screens turns a phone or another computer into a **clicker,
trackpad, remote control, mirror, or second screen** for a PC. It is a **Rust
workspace** — not a Pages web app like the other Universal Apps — with hosts
(`crates/host-windows` for Windows, `crates/host` for macOS), a cross-platform
desktop client (`crates/client`, OpenH264 decode + wgpu render), a Jetpack
Compose Android app (`apps/android` via `crates/mobile-ffi` +
`crates/android-jni`), and a SwiftUI iOS scaffold (`apps/ios`). Everything
speaks one host ⇄ client protocol (length-prefixed `postcard` frames, H.264
video) defined in `crates/protocol`.

Its cloud touchpoints live in the `opensource-portal` Worker: the `/screens`
marketing/download page, and a **browser receiver** at `/screens/receive` — a
rendezvous Durable Object (+ `/screens/turn` for WebRTC ICE) that lets an app
or another browser pair by code, with a WebRTC peer-to-peer data channel
proven. Native LAN connections are PIN-gated **and** transport-encrypted with a
Noise tunnel keyed by the PIN (see the root `README.md` Security note and
`M10-transport-encryption.md`); the browser-bridge leg is the remaining plaintext
path.

## What's here

| File(s) | What they cover |
|---|---|
| `DEV-PREVIEW.md` | Quick-start for testers running the current build without compiling from source. |
| `WINDOWS-CLIENT.md` | Runbook: Windows laptop as a second screen for a Mac (Mac host → Windows client). |
| `SECOND-SCREEN.md` | Windows "Second screen" (extend) setup — the one-time virtual-display driver install. |
| `M2-…` – `M6-…` | Milestone design docs for the core pipeline: input round-trip (M2), virtual display (M3), HiDPI deferral notes (M4), mobile clients / remote control (M5), presentation clicker (M6). |
| `M7-…` – `M9-…` | Milestone docs for the browser era: browser client (M7), Windows second screen plan (M7), browser receiver + rendezvous (M8), WebRTC media (M8e), phone self-capture design (M8f), LAN discovery (M9). |
| `M10-transport-encryption.md` | Noise (snow) transport encryption over the LAN TCP protocol, keyed by the pairing PIN — the `extender-transport` crate. |
| `claude-handover.md` | Dated session-handover log for AI-assisted development — newest entry first. |

## Suite context

This repo is one part of the **Universal Simulation suite** (the open-source
Universal Apps family). For cross-repo context — how the `@unisim/sdk`, edge
routing, and the suite changelog wire together — see the suite docs repo:
[`universal-simulation-ltd/docs`](https://github.com/universal-simulation-ltd/docs)
(private; checked out at the umbrella root as `Docs_UNI_SIM/` for suite
contributors). Start with `ARCHITECTURE.md` (the cross-repo map).
