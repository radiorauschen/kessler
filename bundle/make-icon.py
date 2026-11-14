#!/usr/bin/env python3
"""Build the application icon.

Run this only when the icon changes; everything it writes is committed, so a
normal build needs neither Python nor any of its libraries.

    pip install pillow cairosvg fonttools
    python3 bundle/make-icon.py

**The picture is a k on a tile**, white on black: Muto Bold's k where the
copy has Muto, else Asap's at weight 700 and width 85 — where the window draws
it too, so the k does not jump when the app opens or closes. This is the icon
the system shows before kessler runs. Once the window is up it draws the icon
again itself, in the typeface and the colours it is set in —
`src/gui/appicon.rs` — so the black and white here is only ever seen before
that, and is the one picture of the program that does not depend on a scheme.

A licensed face meant for one Mac never goes into the icon.

**Three shapes**, after the desktop, with the same numbers the window uses:

- the Mac's, Apple's grid — below, and for every desktop without its own;
- GNOME's, Adwaita's rounded square on a 128-unit grid, standing on a lip a
  shade darker than its face (on black the lip is black too);
- KDE's, Breeze's: nearly the whole 48-unit grid, the corners only just
  turned.

What gets written:

- `icon.iconset/` — the Mac's shape, every size `iconutil` wants;
- `icon-gnome/`, `icon-kde/` — the same names in the other two shapes, for
  `install-linux.sh` to choose from;
- `kessler.icon/` — the layered icon for Icon Composer and `actool`: the
  black ground, and the k as a layer of its own;
- `logo.svg` — the icon as a vector;
- `icon-preview.png`, `icon-sizes.png` — to look at.

**The shape.** macOS icons are not rounded rectangles. A rounded rectangle
changes curvature abruptly where the straight side meets the arc, and that
break is exactly what makes a foreign icon look foreign in the Dock. The shape
here has straight sides and quarter-superellipse corners, which leave the side
with zero slope and no jump in curvature. It sits on Apple's grid too: 824
units wide on a 1024 canvas, centred. (The layered icon has no shape: the
system cuts that one itself.)
"""

import io
import json
import math
import pathlib

import cairosvg
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.statisticsPen import StatisticsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from PIL import Image, ImageFilter

HERE = pathlib.Path(__file__).resolve().parent
ICONSET = HERE / "icon.iconset"
LAYERED = HERE / "kessler.icon"
LINUX = {"gnome": HERE / "icon-gnome", "kde": HERE / "icon-kde"}
MUTO = HERE.parent / "fonts" / "Muto" / "MutoBold.otf"
FACE = MUTO if MUTO.exists() else HERE.parent / "fonts" / "Asap" / "Asap-Variable.ttf"
AT = {"wdth": 85, "wght": 700}

CANVAS = 1024

GROUND = (0x00, 0x00, 0x00)
INK = (0xFF, 0xFF, 0xFF)
LIP = tuple(round(c * 0.7) for c in GROUND)

LETTER = 0.56

BY_WEIGHT = 0.5

SHAPES = {
    "mac": ((100 / 1024, 100 / 1024, 924 / 1024, 924 / 1024), ("super", 0.30 * 824 / 1024, 3.5), 924 / 1024),
    "gnome": ((8 / 128, 8 / 128, 120 / 128, 112 / 128), ("round", 12 / 128), 118 / 128),
    "kde": ((3 / 48, 3 / 48, 45 / 48, 45 / 48), ("round", 2.5 / 48), 45 / 48),
}

def hexa(c) -> str:
    return "#" + "".join(f"{v:02x}" for v in c)

def outline(face, corner, steps: int = 48) -> str:
    """The tile's outline on the canvas, as an SVG path."""
    left, top, right, bottom = (v * CANVAS for v in face)
    reach = corner[1] * CANVAS
    power = corner[2] if corner[0] == "super" else 2.0

    def quarter(cx, cy, sx, sy):
        pts = []
        for i in range(steps + 1):
            t = math.pi / 2 * i / steps
            pts.append((cx + sx * reach * math.cos(t) ** (2 / power), cy + sy * reach * math.sin(t) ** (2 / power)))
        return pts

    pts = quarter(right - reach, top + reach, +1, -1)
    pts += quarter(left + reach, top + reach, -1, -1)[::-1]
    pts += quarter(left + reach, bottom - reach, -1, +1)
    pts += quarter(right - reach, bottom - reach, +1, +1)[::-1]
    return "M " + " L ".join(f"{x:.2f},{y:.2f}" for x, y in pts) + " Z"

def letter(face) -> str:
    """The k, as an SVG path, its ink in the middle of the tile's face."""
    font = TTFont(FACE)
    if "fvar" in font:
        font = instancer.instantiateVariableFont(font, AT)
    glyphs = font.getGlyphSet()
    name = font.getBestCmap()[ord("k")]
    bounds = BoundsPen(glyphs)
    glyphs[name].draw(bounds)
    x0, y0, x1, y1 = bounds.bounds
    scale = LETTER * CANVAS / (y1 - y0)
    weight = StatisticsPen(glyphs)
    glyphs[name].draw(weight)
    across = (x0 + x1) / 2 + (weight.meanX - (x0 + x1) / 2) * BY_WEIGHT
    left, top, right, bottom = (v * CANVAS for v in face)
    mx, my = (left + right) / 2, (top + bottom) / 2
    dx = mx - scale * across
    dy = my + scale * (y0 + y1) / 2
    pen = SVGPathPen(glyphs)
    glyphs[name].draw(TransformPen(pen, (scale, 0, 0, -scale, dx, dy)))
    return pen.getCommands()

RIM = 4
RIM_STRENGTH = 0.5
SHADOW_DOWN = 12
SHADOW_SOFT = 14
SHADOW_STRENGTH = 0.5

def svg(shape: str = "mac", with_tile: bool = True, dressed: bool = True) -> str:
    """One whole icon as SVG: the tile, its lip, and the k. Without the tile,
    the k alone on nothing — the layer Icon Composer puts on a ground of its
    own."""
    face, corner, lip = SHAPES[shape]
    tile = ""
    if with_tile:
        if lip > face[3]:
            moved = (face[0], face[1] + lip - face[3], face[2], lip)
            tile += f'<path d="{outline(moved, corner)}" fill="{hexa(LIP)}"/>\n'
        tile += f'<path d="{outline(face, corner)}" fill="{hexa(GROUND)}"/>\n'
        if shape == "mac" and dressed:
            width = 2 * RIM * CANVAS / 1024
            tile += (
                '<defs><clipPath id="face"><path d="' + outline(face, corner) + '"/></clipPath>'
                '<linearGradient id="rim" x1="0" y1="0" x2="0" y2="1">'
                '<stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#000000"/>'
                '</linearGradient></defs>\n'
                f'<path d="{outline(face, corner)}" fill="none" stroke="url(#rim)" stroke-width="{width:.2f}" '
                f'stroke-opacity="{RIM_STRENGTH}" clip-path="url(#face)"/>\n'
            )
    figure = f'<path d="{letter(face)}" fill="{hexa(INK)}"/>'
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{CANVAS}" height="{CANVAS}" '
        f'viewBox="0 0 {CANVAS} {CANVAS}">\n{tile}{figure}\n</svg>\n'
    )

def icon(px: int, shape: str = "mac", with_tile: bool = True) -> Image.Image:
    """One icon at `px` pixels, drawn at that size — not shrunk."""
    png = cairosvg.svg2png(bytestring=svg(shape, with_tile).encode(), output_width=px, output_height=px)
    drawn = Image.open(io.BytesIO(png)).convert("RGBA")
    if shape != "mac" or not with_tile:
        return drawn
    scale = px / 1024
    shade = Image.new("L", (px, px), 0)
    shade.paste(drawn.getchannel("A"), (0, round(SHADOW_DOWN * scale)))
    shade = shade.filter(ImageFilter.GaussianBlur(SHADOW_SOFT * scale / 2))
    shade = shade.point(lambda a: round(a * SHADOW_STRENGTH))
    under = Image.new("RGBA", (px, px), (0, 0, 0, 0))
    under.putalpha(shade)
    return Image.alpha_composite(under, drawn)

def write_iconset(folder: pathlib.Path, shape: str) -> None:
    folder.mkdir(exist_ok=True)
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            px = size * scale
            name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
            icon(px, shape).save(folder / name)

def colour(rgb) -> str:
    return "srgb:" + ",".join(f"{c / 255:.5f}" for c in rgb) + ",1.00000"

def write_layered() -> None:
    """The Icon Composer file: the black ground and the white k, one look for
    light and dark alike. No glass: the k is flat colour."""
    assets = LAYERED / "Assets"
    assets.mkdir(parents=True, exist_ok=True)
    for old in assets.glob("*.png"):
        if old.name != "Image.png":
            old.unlink()
    icon(CANVAS, "mac", with_tile=False).save(assets / "Image.png")
    recipe = {
        "fill-specializations": [{"value": {"solid": colour(GROUND)}}],
        "groups": [
            {
                "layers": [
                    {
                        "glass": False,
                        "image-name-specializations": [{"value": "Image.png"}],
                        "name": "k",
                        "position": {"scale": 1, "translation-in-points": [0, 0]},
                    }
                ],
                "shadow": {"kind": "neutral", "opacity": 0.5},
                "translucency": {"enabled": False, "value": 0},
            }
        ],
        "supported-platforms": {"circles": ["watchOS"], "squares": "shared"},
    }
    (LAYERED / "icon.json").write_text(json.dumps(recipe, indent=2) + "\n")

def write_previews() -> None:
    shapes = list(SHAPES)
    half = CANVAS // 2
    sheet = Image.new("RGBA", (half * len(shapes), CANVAS), (0xE8, 0xE8, 0xED, 255))
    sheet.paste((0x10, 0x10, 0x12, 255), (0, half, half * len(shapes), CANVAS))
    for i, shape in enumerate(shapes):
        big = icon(half, shape)
        sheet.alpha_composite(big, (i * half, 0))
        sheet.alpha_composite(big, (i * half, half))
    sheet.save(HERE / "icon-preview.png")

    sizes = (16, 32, 64, 128, 256)
    band = 256 + 32
    width = sum(sizes) + 16 * (len(sizes) + 1)
    strip = Image.new("RGBA", (width, 2 * band), (0xE8, 0xE8, 0xED, 255))
    strip.paste((0x10, 0x10, 0x12, 255), (0, band, width, 2 * band))
    for row in range(2):
        x = 16
        for size in sizes:
            y = row * band + 16 + (256 - size)
            strip.alpha_composite(icon(size), (x, y))
            x += size + 16
    strip.save(HERE / "icon-sizes.png")

def main() -> None:
    write_iconset(ICONSET, "mac")
    for shape, folder in LINUX.items():
        write_iconset(folder, shape)
    write_layered()
    (HERE / "logo.svg").write_text(svg(dressed=False))
    write_previews()
    print(f"wrote {ICONSET.name}/, {', '.join(f.name + '/' for f in LINUX.values())} {LAYERED.name}/, logo.svg")

if __name__ == "__main__":
    main()
