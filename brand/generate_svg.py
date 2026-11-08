#!/usr/bin/env python3
"""Generates brand/svg/*.svg. Usage: python3 brand/generate_svg.py brand/svg"""
import math, os, sys
OUT = sys.argv[1]
INK = "#f1e9df"
DARK = "#16130f"
APPS = {
    # name: (symbol second letter, accent, glyph)
    "ferrum":  ("e", "#E8743B", None),
    "ide":     ("i", "#5B8DEF", "brackets"),
    "api":     ("a", "#3FB8A0", "arrows"),
    "git":     ("g", "#F0B43C", "branch"),
    "data":    ("d", "#A07CF0", "cylinder"),
    "term":    ("t", "#7CC85A", "prompt"),
    "toolbox": ("x", "#E85A7E", "grid"),
}
SW = 62  # letter stroke width

def letter(ch, ox=0.0, oy=0.0, s=1.0):
    """Monoline glyph paths in a 1024 grid: cap 390..730, x-height 480..730, x 590..760."""
    def P(d): return d
    paths = []
    if ch == "F":
        paths = ["M330 730 V390 H530", "M330 555 H485"]
    elif ch == "e":
        paths = ["M584 605 H768 A92 126 0 1 0 748 692"]
    elif ch == "i":
        paths = ["M675 535 V730"]
    elif ch == "a":
        paths = ["M762 480 V730", "M762 605 A88 125 0 1 0 762 605.1"]
    elif ch == "d":
        paths = ["M762 390 V730", "M762 605 A88 125 0 1 0 762 605.1"]
    elif ch == "g":
        paths = ["M762 480 V742 Q762 832 668 832 Q622 832 594 816", "M762 588 A88 108 0 1 0 762 588.1"]
    elif ch == "t":
        paths = ["M665 410 V665 Q665 730 735 730 H752", "M598 500 H748"]
    elif ch == "x":
        paths = ["M596 482 L756 728", "M756 482 L596 728"]
    elif ch == "r":
        paths = ["M600 730 V480", "M600 580 Q600 480 700 480 H735"]
    elif ch == "u":
        paths = ["M600 480 V640 Q600 730 680 730 Q760 730 760 640", "M760 480 V730"]
    elif ch == "m":
        paths = ["M560 730 V480", "M560 570 Q560 480 640 480 Q700 480 700 560 V730", "M700 560 Q700 480 770 480 Q840 480 840 560 V730"]
    return paths

def dot_i():
    return '<circle cx="675" cy="428" r="44"/>'

def glyph(kind):
    # drawn in a 220x220 box centred at (0,0), dark strokes on accent badge
    w = 30
    common = f'fill="none" stroke="{DARK}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round"'
    if kind == "brackets":
        return f'<path {common} d="M-30 -55 L-75 0 L-30 55 M30 -55 L75 0 L30 55"/>'
    if kind == "arrows":
        return f'<path {common} d="M-70 -30 H60 M30 -60 L65 -30 L30 0 M70 35 H-60 M-30 5 L-65 35 L-30 65"/>'
    if kind == "branch":
        return (f'<path {common} d="M-40 -70 V70 M-40 30 Q-40 -10 0 -15 Q40 -20 40 -50"/>'
                f'<circle cx="-40" cy="70" r="20" fill="{DARK}"/><circle cx="-40" cy="-70" r="20" fill="{DARK}"/><circle cx="40" cy="-55" r="20" fill="{DARK}"/>')
    if kind == "cylinder":
        return f'<path {common} d="M-60 -45 A60 22 0 1 0 60 -45 A60 22 0 1 0 -60 -45 V45 A60 22 0 0 0 60 45 V-45 M-60 0 A60 22 0 0 0 60 0"/>'
    if kind == "prompt":
        return f'<path {common} d="M-70 -50 L-20 0 L-70 50 M5 55 H75"/>'
    if kind == "grid":
        r = 18
        return "".join(f'<rect x="{x-30}" y="{y-30}" width="60" height="60" rx="{r}" fill="{DARK}"/>' for x in (-38, 38) for y in (-38, 38))
    return ""

def hexagon(cx, cy, r):
    pts = [(cx + r * math.cos(math.radians(a)), cy + r * math.sin(math.radians(a))) for a in range(0, 360, 60)]
    return pts

def tile(name, ch, accent, kind, defs_id):
    hx = hexagon(512, 560, 318)
    poly = " ".join(f"{x:.1f},{y:.1f}" for x, y in hx)
    nodes = "".join(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="20"/>' for x, y in hx)
    inner = hexagon(512, 560, 200)
    inner_poly = " ".join(f"{x:.1f},{y:.1f}" for x, y in inner)
    spokes = "".join(f'M{x0:.1f} {y0:.1f} L{x1:.1f} {y1:.1f} ' for (x0, y0), (x1, y1) in zip(inner, hx))
    letters = "".join(f'<path d="{d}"/>' for d in letter("F"))
    second = "".join(f'<path d="{d}"/>' for d in letter(ch))
    idot = f'<g fill="{accent}">{dot_i()}</g>' if ch == "i" else ""
    badge = ""
    if kind:
        badge = (f'<circle cx="800" cy="224" r="128" fill="{DARK}"/>'
                 f'<circle cx="800" cy="224" r="106" fill="{accent}"/>'
                 f'<g transform="translate(800 224)">{glyph(kind)}</g>')
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <title>Ferrum {name}</title>
  <defs>
    <linearGradient id="{defs_id}-bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#2b241c"/>
      <stop offset="1" stop-color="#14110d"/>
    </linearGradient>
    <radialGradient id="{defs_id}-glow" cx="0.3" cy="0.25" r="0.9">
      <stop offset="0" stop-color="{accent}" stop-opacity="0.22"/>
      <stop offset="0.6" stop-color="{accent}" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect x="48" y="48" width="928" height="928" rx="208" fill="url(#{defs_id}-bg)"/>
  <rect x="48" y="48" width="928" height="928" rx="208" fill="url(#{defs_id}-glow)"/>
  <rect x="96" y="96" width="832" height="832" rx="164" fill="none" stroke="{accent}" stroke-width="28"/>
  <g transform="translate(0 -34)">
  <g fill="none" stroke="{accent}" stroke-opacity="0.18" stroke-width="12" stroke-linejoin="round">
    <polygon points="{poly}"/>
    <polygon points="{inner_poly}"/>
  </g>
  <g fill="{accent}" fill-opacity="0.45">{nodes}</g>
  <g fill="none" stroke="{INK}" stroke-width="{SW}" stroke-linecap="butt" stroke-linejoin="miter">{letters}</g>
  <g fill="none" stroke="{accent}" stroke-width="{SW}" stroke-linecap="butt" stroke-linejoin="round">{second}</g>
  {idot}
  </g>
  {badge}
</svg>
'''

os.makedirs(OUT, exist_ok=True)
for name, (ch, accent, kind) in APPS.items():
    with open(os.path.join(OUT, f"{name}.svg"), "w") as f:
        f.write(tile(name, ch, accent, kind, name))

# Wordmark: core tile + "Ferrum" monoline, 3400x1024
def shift(paths, dx):
    return [f'<path transform="translate({dx} 0)" d="{d}"/>' for d in paths]
core = tile("ferrum", "e", "#E8743B", None, "wm")
inner = core.split("\n", 1)[1].rsplit("</svg>", 1)[0]
word = []
# (glyph, left edge of its drawing in the 1024 grid, slot x in the word)
for glyph_char, left, slot in [("F", 330, 0), ("e", 584, 270), ("r", 600, 560), ("r", 600, 755), ("u", 600, 950), ("m", 560, 1215)]:
    word += shift(letter(glyph_char), slot - left)
wm = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2500 1024" width="2500" height="1024">
  <title>Ferrum</title>
  {inner.replace('<title>Ferrum ferrum</title>', '')}
  <g transform="translate(1110 64) scale(0.82)" fill="none" stroke="{INK}" stroke-width="{SW}" stroke-linecap="butt" stroke-linejoin="round">
    {"".join(word)}
  </g>
</svg>
'''
with open(os.path.join(OUT, "ferrum-wordmark.svg"), "w") as f:
    f.write(wm)
# Same mark with dark lettering for light backgrounds.
with open(os.path.join(OUT, "ferrum-wordmark-light.svg"), "w") as f:
    f.write(wm.replace(f'stroke="{INK}" stroke-width="{SW}" stroke-linecap="butt" stroke-linejoin="round">\n    ', f'stroke="#1f1913" stroke-width="{SW}" stroke-linecap="butt" stroke-linejoin="round">\n    ', 1))
print("ok")
