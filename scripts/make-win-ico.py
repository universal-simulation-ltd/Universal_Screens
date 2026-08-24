#!/usr/bin/env python3
"""Generate the Windows .ico from the shared app-icon artwork.

Reuses `render_icon()` from make-app-icon.py (single source of truth) and writes:
  crates/host-windows/assets/app-icon.ico   (multi-resolution, 16-256 px)

Every size is *rendered* rather than downsampled from the 256 px PNG, so the
16/24/32 px entries Explorer and the taskbar actually use stay legible.

The .ico is what the installer stamps on Start Menu / desktop shortcuts and on
Add/Remove Programs, and what `build.rs` embeds into the host executable.

Run after changing the artwork:  python scripts/make-win-ico.py
"""
import importlib.util
import os

HERE = os.path.dirname(__file__)

# Load make-app-icon.py (hyphens → can't `import` directly) for render_icon().
_spec = importlib.util.spec_from_file_location(
    "make_app_icon", os.path.join(HERE, "make-app-icon.py")
)
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)

OUT = os.path.abspath(
    os.path.join(HERE, "..", "crates", "host-windows", "assets", "app-icon.ico")
)

# Windows picks the nearest entry for the context it's drawing (16 = title bar and
# Explorer's detail view, 32 = taskbar at 100%, 48 = medium icons, 256 = the big
# tiles and the Alt-Tab switcher). 24 and 64 cover the 150%/200% DPI scalings.
SIZES = [16, 24, 32, 48, 64, 128, 256]


def main() -> None:
    frames = [_mod.render_icon(px) for px in SIZES]
    # Pillow writes every `sizes` entry from the base image; passing the rendered
    # frames via append_images keeps each entry's own artwork instead.
    frames[-1].save(
        OUT, format="ICO", sizes=[(px, px) for px in SIZES], append_images=frames[:-1]
    )
    print("wrote", OUT, "-", ", ".join(f"{p}x{p}" for p in SIZES))


if __name__ == "__main__":
    main()
