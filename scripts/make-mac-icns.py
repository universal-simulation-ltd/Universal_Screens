#!/usr/bin/env python3
"""Generate the macOS .icns from the shared app-icon artwork.

Reuses `render_icon()` from make-app-icon.py (single source of truth) and writes:
  crates/host-macos/assets/AppIcon.icns

Every size is *rendered* rather than downsampled from one large PNG, for the same
reason make-win-ico.py does it: the small entries are the ones the system actually
shows most (16/32 px in the Finder list view and the menu bar), and a LANCZOS
downsample of a 1024 px drawing turns their hairlines to mush.

⚠️ Rounded corners are OURS to draw, not the system's. macOS does NOT mask app
icons the way iOS does — whatever is in the .icns is what appears in the Dock. So
this uses `opaque=False`, keeping render_icon's rounded-square on a transparent
canvas. Passing opaque=True (the iOS setting) would put a hard-cornered square in
the Dock next to every rounded one.

`iconutil` is the only supported way to build an .icns and ships with macOS, so
this script is macOS-only by necessity — there is no Windows twin.

Run after changing the artwork:  python3 scripts/make-mac-icns.py
"""
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT_DIR = os.path.join(HERE, "..", "crates", "host-macos", "assets")
OUT = os.path.join(OUT_DIR, "AppIcon.icns")

# Load make-app-icon.py (hyphens → can't `import` directly) for render_icon().
_spec = importlib.util.spec_from_file_location(
    "make_app_icon", os.path.join(HERE, "make-app-icon.py")
)
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)

# The .iconset names macOS expects. Each (base, scale) renders at base*scale px.
# 16 through 512@2x = 1024 is the full set `iconutil` accepts; omitting any of
# them makes the icon fall back to a scaled neighbour in that context.
SIZES = [
    (16, 1), (16, 2),
    (32, 1), (32, 2),
    (128, 1), (128, 2),
    (256, 1), (256, 2),
    (512, 1), (512, 2),
]


def main() -> None:
    if sys.platform != "darwin":
        sys.exit("make-mac-icns.py needs macOS (iconutil). Nothing else builds .icns.")
    if not shutil.which("iconutil"):
        sys.exit("iconutil not found — it ships with macOS; is this a full install?")

    os.makedirs(OUT_DIR, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        iconset = os.path.join(tmp, "AppIcon.iconset")
        os.makedirs(iconset)
        for base, scale in SIZES:
            px = base * scale
            suffix = "" if scale == 1 else "@2x"
            name = f"icon_{base}x{base}{suffix}.png"
            _mod.render_icon(px, opaque=False).save(os.path.join(iconset, name))
            print(f"  rendered {name} ({px}px)")
        subprocess.run(
            ["iconutil", "-c", "icns", iconset, "-o", os.path.abspath(OUT)],
            check=True,
        )
    print("wrote", os.path.normpath(OUT))


if __name__ == "__main__":
    main()
