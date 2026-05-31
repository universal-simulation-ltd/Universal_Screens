# Running the client on Windows

Use a **Windows laptop as a second screen for your Mac**. The Mac runs the
**host** (creates a virtual display, captures + H.264-encodes it, streams over
the LAN); the Windows laptop runs the **client** (decodes, renders, and sends
your mouse/keyboard back to the Mac).

The client is fully cross-platform (Rust + winit + wgpu + the openh264 software
decoder). The **host is macOS-only** and stays on the Mac.

```
   Mac (host)                                  Windows laptop (client)
 ┌──────────────────┐   H.264 over TCP        ┌──────────────────────┐
 │ virtual display  │ ──────────────────────► │ openh264 decode      │
 │ → capture/encode │                          │ → wgpu render        │
 │ → inject input   │ ◄────────────────────── │ → capture input      │
 └──────────────────┘   input over TCP         └──────────────────────┘
```

---

## 1. Prerequisites (Windows, one-time)

| Tool | Why | How |
|---|---|---|
| **Rust** (MSVC toolchain) | builds the client | [rustup.rs](https://rustup.rs) — accept the default `x86_64-pc-windows-msvc` |
| **Visual Studio C++ Build Tools** | C/C++ linker + compiler for native deps | [VS Build Tools](https://visualstudio.microsoft.com/downloads/) → "Desktop development with C++" (rustup will also prompt for this) |
| **NASM** on `PATH` | assembles the bundled OpenH264 decoder | `winget install NASM.NASM`, or [nasm.us](https://www.nasm.us/) then add its folder to `PATH` |
| **Git** | clone the repo | [git-scm.com](https://git-scm.com/) or `winget install Git.Git` |

After installing, **open a fresh terminal** so `PATH` changes take effect. Verify:

```
rustc --version
nasm -v
```

---

## 2. Get the code

```
cd D:\Github\UNISIM\Universal_Apps
git clone https://github.com/universal-simulation-ltd/Universal_ScreenExtender.git
cd Universal_ScreenExtender
```

(If you already have it, `git pull` instead.)

> `cargo run -p extender-client` builds **only** the client and its portable
> dependencies — it does **not** try to build the macOS-only host, so the
> workspace builds fine on Windows.

---

## 3. Start the host (on the Mac)

```
cd /Users/jamesmarkey/Github/UNISIM/Universal_Apps/Universal_ScreenExtender
cargo run --release -p extender-host
```

- On first run, macOS prompts to **allow incoming network connections** — allow it. The host listens on `0.0.0.0:9000`.
- It also needs **Screen Recording** + **Accessibility** permissions (System Settings → Privacy & Security) — grant them if prompted.
- Find the Mac's LAN IP (you'll need it on Windows):
  ```
  ipconfig getifaddr en0     # Ethernet; try en1 for Wi-Fi
  ```
  (Or read it from System Settings → Network.)
- The host now **sizes the virtual display automatically** to match the client's
  panel — the client advertises its resolution when it connects, so you don't
  need to pass a size. To **force** a size instead (overriding the client), pass
  it as a 2nd arg: `cargo run --release -p extender-host -- 0.0.0.0:9000 2560x1440`.

The Mac and Windows machines must be on the **same LAN**.

---

## 4. Run the client (on Windows)

Use `--release` — the H.264 decode is software, and release builds are much smoother:

```
cargo run --release -p extender-client -- 192.168.1.42:9000
```

Replace **`192.168.1.42`** with your Mac's IP from step 3. (The first build
compiles OpenH264 from source, so it takes a minute or two.)

On startup the client lists your monitors and reports the chosen one's **native
(physical) resolution** to the host, which sizes the virtual display to match.
With **multiple monitors**, pass a monitor index (from that list) as a 2nd arg to
pick which one to mirror the size of:

```
cargo run --release -p extender-client -- 192.168.1.42:9000 1
```

A window titled **"ExtenderScreen client"** opens showing the Mac's virtual
screen (blank at first — it's an empty second desktop).

---

## 5. Use it

- On the **Mac**, open **System Settings → Displays** and drag a window onto the
  "ExtenderScreen Virtual Display" (or set its arrangement). It appears in the
  Windows window.
- In the **Windows** window:
  - **Click** to grab control (pointer locks). Move the mouse / type / scroll →
    it all drives the virtual screen on the Mac.
  - **Esc** releases control.
  - **F** (when not in control mode) toggles fullscreen; **Esc** exits fullscreen.

---

## 6. Troubleshooting

| Symptom | Fix |
|---|---|
| Build error mentioning `nasm` / `openh264-sys2` | NASM isn't on `PATH`. Install it, open a new terminal, rebuild. |
| Build error about a linker / `link.exe` / `cl.exe` | Install the Visual Studio "Desktop development with C++" workload. |
| `client error: ... Connection refused` / times out | Host isn't running, wrong IP, not on the same LAN, or the macOS firewall is blocking — re-check step 3. |
| Window is black | Nothing is on the virtual display yet — drag a window onto it on the Mac (step 5). |
| Choppy / high CPU | Software decode is CPU-bound, and the client now auto-selects your **native** resolution (e.g. 2880×1800 on a HiDPI laptop), which is ~2× the pixels of its scaled size. Make sure you used `--release`; if it's still heavy, force a smaller size on the host (step 3, e.g. `... 1920x1200`). |
| Colors look wrong | File an issue — the client decodes to RGBA; a swap would be a small fix. |

---

## Notes

- **Mouse "moves by itself"** only happens when the host *and* client run on the
  **same** Mac (the host's injected moves re-enter the client's raw input). With
  the Windows laptop as a separate client, this does not occur.
- The **host is macOS-only** (ScreenCaptureKit / VideoToolbox / the private
  `CGVirtualDisplay`). A Windows *host* is a separate, larger effort.
- The virtual display is currently **non-HiDPI** at the chosen resolution; true
  Retina is deferred (see [`M4-hidpi-deferred.md`](M4-hidpi-deferred.md)).
- Design docs for the streaming/input/virtual-display milestones are alongside
  this file in [`docs/`](.).
