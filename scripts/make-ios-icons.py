#!/usr/bin/env python3
"""⚠️ SUPERSEDED — DO NOT RUN WITHOUT READING THIS.

Superseded twice. On 2026-08-27 Universal Screens' icons moved to the suite's
canonical mark; on 2026-11-05 the INSTALLED apps got an icon of their own,
"one picture, split" — a desk screen and a phone with a single orange disc
drawn across both. It is drawn by

    node scripts/app-icon.mjs

which writes every installed surface at once (Windows .png/.ico, macOS
.png/.icns, Linux .png, the Android launcher set, the iOS asset catalogue).
Two things are still the suite's, and come from

    node backoffice/universal-platform/scripts/app-marks/desktop-icons.mjs Universal_Screens

namely the sealed installer icon and the phone clients' in-app masthead logo.

This script still draws the 2026-08 laptop-and-phone artwork, and running it
will OVERWRITE the current icons with it. Kept because the drawing is the only
record of that artwork.
"""

"""Generate the iOS asset-catalog images from the shared app-icon artwork.

Reuses `render_icon()` from make-app-icon.py (single source of truth) and writes:
  apps/ios/ScreenExtender/Assets.xcassets/AppIcon.appiconset/icon-1024.png  (opaque)
  apps/ios/ScreenExtender/Assets.xcassets/AppLogo.imageset/logo.png         (RGBA, in-app)

The .xcassets Contents.json files are committed alongside; only the PNGs are
regenerated here. Run after changing the artwork:  python scripts/make-ios-icons.py
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

ASSETS = os.path.abspath(
    os.path.join(HERE, "..", "apps", "ios", "ScreenExtender", "Assets.xcassets")
)


def main() -> None:
    # App icon: a single 1024 universal icon (Xcode 14+ single-size slot). iOS app
    # icons must be opaque (no alpha) — render_icon(opaque=True) handles that.
    appicon = os.path.join(ASSETS, "AppIcon.appiconset", "icon-1024.png")
    _mod.render_icon(1024, opaque=True).save(appicon)
    print("wrote", appicon)

    # In-app logo (shown on the home screen, like Android's app_icon). Keep the
    # rounded-square on a transparent canvas so it reads as a tile at any size.
    logo = os.path.join(ASSETS, "AppLogo.imageset", "logo.png")
    _mod.render_icon(512, opaque=False).save(logo)
    print("wrote", logo)


if __name__ == "__main__":
    main()
