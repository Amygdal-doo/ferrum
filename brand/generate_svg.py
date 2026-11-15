#!/usr/bin/env python3
"""Generates the Ferrum logo SVGs. Usage: python3 brand/generate_svg.py brand/svg

Every logo is a flat tile: a dark rounded square with a thin border in the
app's accent, and "Fe" in near-white plus the app's capital in the accent,
centred on one line. Three masters per logo:

  svg/<name>.svg         the flat tile, full bleed: the website, in-app logos,
                         PNGs from 64px up, Windows .ico and Linux icons
  svg/small/<name>.svg   the same tile drawn for 16-32px: a heavier border and
                         bigger, bolder letters (`cargo brand` uses it there)
  svg/macos/<name>.svg   the macOS app icon: the tile on Apple's icon grid (an
                         824px rounded square centred in 1024, with a soft
                         shadow), used for the .icns from 64px up

The letters are stroked paths, so nothing depends on installed fonts.
"""
import os
import sys

OUT = sys.argv[1]
TILE = "#0d1318"
INK = "#eef2f5"  # the "Fe", near-white
WORD_DARK = "#0d1318"

APPS = {
    # name: (capital after "Fe", accent). The core mark is plain "Fe" with an
    # accent "e". Files keep the app ids (editor = FerrumIDE, env =
    # FerrumVariables, vibe = FerrumEngineer, dock = FerrumContainers).
    "ferrum":   (None, "#E8743B"),
    "api":      ("A", "#3FB8A0"),
    "git":      ("G", "#F0B43C"),
    "data":     ("D", "#A07CF0"),
    "term":     ("T", "#7CC85A"),
    "launcher": ("L", "#E85A7E"),
    "kit":      ("K", "#D46BE0"),
    "dock":     ("C", "#38BDF8"),
    "editor":   ("I", "#C5DC3A"),
    "ports":    ("P", "#5B8DEF"),
    "env":      ("V", "#F2665A"),
    "vibe":     ("N", "#4CD97B"),
    "system":   ("S", "#2DD4BF"),
}

# Glyphs as stroke centre lines on a cap height of 390..730 (x-height 480):
# (paths, left, right) where left/right are the outermost centre-line x.
GLYPHS = {
    "F": (["M0 730 V390 H190", "M0 552 H160"], 0, 190),
    "e": (["M0 605 H180 A90 125 0 1 0 160 690"], 0, 180),
    "r": (["M0 730 V480", "M0 580 Q0 480 100 480 H130"], 0, 130),
    "u": (["M0 480 V640 Q0 730 80 730 Q160 730 160 640", "M160 480 V730"], 0, 160),
    "m": (["M0 730 V480", "M0 570 Q0 480 75 480 Q140 480 140 560 V730",
           "M140 560 Q140 480 210 480 Q280 480 280 560 V730"], 0, 280),
    # A, K, N and V run their diagonals past the cap band, and the clip to
    # the band cuts them flat.
    "A": (["M-18.5 790 L105 390 L228.5 790", "M37 610 H173"], 0, 210),
    "G": (["M196 452 A108 170 0 1 0 216 562", ("M130 562 H216", "square")], -2, 216),
    "D": (["M0 390 V730", "M0 390 H60 A140 170 0 0 1 60 730 H0"], 0, 200),
    "T": (["M0 390 H210", "M105 390 V730"], 0, 210),
    "L": (["M0 390 V730 H170"], 0, 170),
    "K": (["M0 330 V790", "M241.8 330 L0 610", "M66 534 L241 790"], 0, 228),
    "C": (["M200 456 A110 170 0 1 0 200 664"], -10, 200),
    "I": (["M70 390 V730", "M0 390 H140", "M0 730 H140"], 0, 140),
    "P": (["M0 730 V390 H90 A95 88 0 0 1 90 566 H0"], 0, 185),
    "V": (["M-18.5 330 L105 730 L228.5 330"], 0, 210),
    "N": (["M0 330 V790", "M190 330 V790", "M-33.5 330 L223.5 790"], 0, 190),
    "S": (["M182 440 Q160 390 98 390 Q10 390 10 470 Q10 540 98 556 Q188 572 188 646 "
           "Q188 730 98 730 Q26 730 4 680"], 4, 188),
}
CAP_TOP, BASELINE = 390, 730
CAP_MID = (CAP_TOP + BASELINE) / 2


# Curved letters keep butt ends on their open strokes; straight ones get square
# ends so stems reach the same cap top and baseline as the bars.
BUTT = set("eCGSrumAKNV")
# Clipped at its sides as well as to the cap band (the diagonal ends inside
# the stems).
SIDES = set("N")


def clip_def(ch, sw):
    """Every glyph is clipped to the cap band (the centre lines plus half a
    stroke), which squares off the points of A, K, N and V."""
    paths, left, right = GLYPHS[ch]
    x, w = (left - sw / 2, right - left + sw) if ch in SIDES else (-1000, 3000)
    top = CAP_TOP - sw / 2
    return (f'<clipPath id="{clip_id(ch, sw)}" clipPathUnits="userSpaceOnUse">'
            f'<rect x="{x}" y="{top}" width="{w}" height="{BASELINE - CAP_TOP + sw}"/></clipPath>')


def clip_id(ch, sw):
    return f"clip-{ch}{'-cap' if ch.isupper() else ''}-{sw}"


def path(d):
    if isinstance(d, tuple):
        return f'<path d="{d[0]}" stroke-linecap="{d[1]}"/>'
    return f'<path d="{d}"/>'


def word(chars, colours, sw, gap):
    """Lays the glyphs out left to right; returns (svg, total width incl. strokes)."""
    x, parts = 0.0, [clip_def(ch, sw) for ch in dict.fromkeys(chars)]
    for i, (ch, colour) in enumerate(zip(chars, colours)):
        paths, left, right = GLYPHS[ch]
        if i > 0:
            # "Fe": the e tucks under the F's arm.
            x += gap * (0.45 if chars[i - 1] == "F" and ch.islower() else 1)
        dx = x + sw / 2 - left
        body = "".join(path(d) for d in paths)
        join = "miter" if ch in "FELTIKNVAD" else "round"
        cap = "butt" if ch in BUTT else "square"
        parts.append(f'<g transform="translate({dx:.1f} 0)" clip-path="url(#{clip_id(ch, sw)})" fill="none" '
                     f'stroke="{colour}" stroke-width="{sw}" stroke-linecap="{cap}" '
                     f'stroke-linejoin="{join}" stroke-miterlimit="10">{body}</g>')
        x = dx + right + sw / 2
    return "".join(parts), x


def symbol(cap, accent, sw, gap):
    if cap is None:
        return word("Fe", [INK, accent], sw, gap)
    return word(["F", "e", cap], [INK, INK, accent], sw, gap)


def widest(sw, gap):
    return max(symbol(cap, a, sw, gap)[1] for cap, a in APPS.values())


def tile(cap, accent, small=False):
    """The flat tile's content in a 1024 box (no <svg> wrapper)."""
    if small:
        border, sw, gap, fill = 72, 84, 34, 0.80
    else:
        border, sw, gap, fill = 34, 64, 30, 0.64
    # One scale for every app, so the letters are the same size everywhere:
    # the widest symbol fills `fill` of the tile.
    scale = fill * 1024 / widest(sw, gap)
    body, width = symbol(cap, accent, sw, gap)
    tx = 512 - width * scale / 2
    ty = 512 - CAP_MID * scale
    half = border / 2
    return (f'<rect width="1024" height="1024" rx="245" fill="{TILE}"/>'
            f'<rect x="{half}" y="{half}" width="{1024 - border}" height="{1024 - border}" '
            f'rx="{245 - half}" fill="none" stroke="{accent}" stroke-width="{border}"/>'
            f'<g transform="translate({tx:.1f} {ty:.1f}) scale({scale:.4f})">{body}</g>')


def svg(title, content, w=1024, h=1024, defs=""):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">\n'
            f'  <title>{title}</title>\n{defs}  {content}\n</svg>\n')


def macos(name, cap, accent):
    """Apple's grid: an 824px body centred in 1024, with a soft drop shadow."""
    defs = (f'  <defs><filter id="{name}-shadow" x="-10%" y="-10%" width="120%" height="125%">'
            '<feDropShadow dx="0" dy="10" stdDeviation="14" flood-color="#000" flood-opacity="0.35"/>'
            '</filter></defs>\n')
    s = 824 / 1024
    return svg(title(name), f'<g filter="url(#{name}-shadow)" transform="translate(100 100) scale({s:.6f})">'
               f'{tile(cap, accent)}</g>', defs=defs)


def title(name):
    return "Ferrum" if name == "ferrum" else f"Ferrum {name}"


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)


for name, (cap, accent) in APPS.items():
    write(os.path.join(OUT, f"{name}.svg"), svg(title(name), tile(cap, accent)))
    write(os.path.join(OUT, "small", f"{name}.svg"), svg(title(name), tile(cap, accent, small=True)))
    write(os.path.join(OUT, "macos", f"{name}.svg"), macos(name, cap, accent))

# Wordmarks: the core tile + "Ferrum", for dark and light backgrounds.
cap, accent = APPS["ferrum"]
letters, width = word("Ferrum", [INK] * 6, 64, 16)
scale = 0.86
wm_w = int(1024 + 120 + width * scale + 40)
for suffix, ink in (("", INK), ("-light", WORD_DARK)):
    body = letters.replace(f'stroke="{INK}"', f'stroke="{ink}"')
    content = (f'{tile(cap, accent)}'
               f'<g transform="translate(1144 {512 - CAP_MID * scale:.1f}) scale({scale})">{body}</g>')
    write(os.path.join(OUT, f"ferrum-wordmark{suffix}.svg"), svg("Ferrum", content, w=wm_w))
print("ok")
