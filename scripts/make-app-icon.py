#!/usr/bin/env python3
"""Generate the Universal Screens app icon (laptop + phone, dark with orange accent).

Renders at 4x and downsamples with LANCZOS for smooth edges. Output:
  crates/host-windows/assets/app-icon.png  (256x256 RGBA)

Run: python scripts/make-app-icon.py
"""
import os
from PIL import Image, ImageDraw

K = 4                      # supersample factor
S = 256 * K
OUT = os.path.join(
    os.path.dirname(__file__),
    "..", "crates", "host-windows", "assets", "app-icon.png",
)

# UNI·SIM-style palette: dark slate with the brand orange accent.
SLATE900 = (15, 23, 42, 255)
SLATE800 = (30, 41, 59, 255)
SLATE600 = (51, 65, 85, 255)
SLATE500 = (71, 85, 105, 255)
SLATE400 = (100, 116, 139, 255)
SCREEN   = (11, 18, 32, 255)
ORANGE   = (224, 85, 4, 255)   # #e05504
# Light slates for the laptop, so it reads clearly against the dark background.
LAPTOP   = (203, 213, 225, 255)  # slate-300 — screen bezel
DECK     = (148, 163, 184, 255)  # slate-400 — keyboard deck (a touch darker for depth)
EDGE     = (226, 232, 240, 255)  # slate-200 — hinge highlight

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)


def rr(b, r, f):
    d.rounded_rectangle([v * K for v in b], radius=r * K, fill=f)


def el(b, f):
    d.ellipse([v * K for v in b], fill=f)


def poly(pts, f):
    d.polygon([(x * K, y * K) for x, y in pts], fill=f)


def ln(p, f, w):
    d.line([(x * K, y * K) for x, y in p], fill=f, width=w * K)


# Rounded-square dark background.
rr([8, 8, 248, 248], 52, SLATE900)

# Laptop — screen lid (bezel + display), then the keyboard deck below.
rr([62, 50, 194, 144], 12, LAPTOP)
rr([72, 60, 184, 134], 7, SCREEN)
rr([84, 72, 148, 80], 4, ORANGE)        # orange accent line on the display
rr([84, 90, 168, 96], 3, SLATE500)
rr([84, 104, 134, 110], 3, SLATE500)
poly([(46, 144), (210, 144), (232, 176), (24, 176)], DECK)  # deck (perspective)
ln([(62, 144), (194, 144)], EDGE, 2)      # hinge highlight
rr([116, 148, 140, 154], 2, SLATE600)     # trackpad notch

# Phone — centred in front of the laptop, with a same-as-background "gap ring"
# so it reads as a separate object sitting on top.
rr([100, 112, 156, 216], 20, SLATE900)    # gap ring
rr([104, 116, 152, 212], 14, SLATE800)    # body
rr([110, 124, 146, 202], 9, ORANGE)       # orange screen
el([126, 118, 130, 122], SLATE500)        # camera dot
rr([120, 205, 136, 208], 1, SLATE600)     # home indicator

img = img.resize((256, 256), Image.LANCZOS)
img.save(os.path.abspath(OUT))
print("wrote", os.path.abspath(OUT))
