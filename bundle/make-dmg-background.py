#!/usr/bin/env python3
"""The picture the disk image opens on : cream ground, the two dogs on the
seesaw and the arrow from kessler to Applications, drawn by felix h-w.

Writes `bundle/dmg/background.png` (the window's points) and
`background@2x.png` (Retina); `make-app.sh` writes the version into the
foot of both on the Mac (`dmg-stamp.swift`), joins them into one TIFF and
has the Finder show it. The icons themselves are not in the
picture: the Finder sets kessler.app and the link to Applications where
`ICON_AT` says, over it.

Run from anywhere: python3 bundle/make-dmg-background.py
"""

from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent / "dmg"

WIDTH, HEIGHT = 680, 700
CANVAS = (2000, 1300)
GROUND = (0xEF, 0xE6, 0xD2)
INK = (0xCC, 0x81, 0x7C)
SEESAW_BOX = (95, 228, 565, 612)
ARROW_BOX = (198, 92, 356, 162)

PAPER = 60  # darkness up to which the scan is paper
INK_FROM = 170  # darkness from which it is the whole of the line
SPECK = 40  # islands of ink smaller than this are dust

def mask_of(scan_path):
    """How much ink there is at each point of the scan, cut to the drawing,
    the specks of dust left out — as `make-drawing.py` does."""
    scan = Image.open(scan_path).convert("RGBA")
    paper = Image.new("RGBA", scan.size, (255, 255, 255, 255))
    grey = Image.alpha_composite(paper, scan).convert("L")
    width, height = grey.size
    dark = [255 - v for v in grey.tobytes()]
    ink = [0 if d <= PAPER else 255 if d >= INK_FROM else round((d - PAPER) * 255 / (INK_FROM - PAPER)) for d in dark]
    seen = bytearray(width * height)
    for start in range(width * height):
        if ink[start] == 0 or seen[start]:
            continue
        island, queue = [start], [start]
        seen[start] = 1
        while queue:
            at = queue.pop()
            x, y = at % width, at // width
            for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
                if 0 <= nx < width and 0 <= ny < height:
                    near = ny * width + nx
                    if ink[near] and not seen[near]:
                        seen[near] = 1
                        island.append(near)
                        queue.append(near)
        if len(island) < SPECK:
            for at in island:
                ink[at] = 0
    mask = Image.new("L", (width, height))
    mask.putdata(ink)
    return mask.crop(mask.getbbox())

def place(canvas, mask, box, scale):
    left, top, right, bottom = (v * scale for v in box)
    fit = min((right - left) / mask.width, (bottom - top) / mask.height)
    size = (round(mask.width * fit), round(mask.height * fit))
    small = mask.resize(size, Image.LANCZOS)
    at = (round(left + (right - left - size[0]) / 2), round(top + (bottom - top - size[1]) / 2))
    colour = Image.new("RGB", size, INK)
    canvas.paste(colour, at, small)

def main():
    seesaw = mask_of(HERE / "seesaw-scan.webp")
    arrow = mask_of(HERE / "arrow-scan.webp")
    for scale, name in ((1, "background.png"), (2, "background@2x.png")):
        canvas = Image.new("RGB", (CANVAS[0] * scale, CANVAS[1] * scale), GROUND)
        place(canvas, seesaw, SEESAW_BOX, scale)
        place(canvas, arrow, ARROW_BOX, scale)
        canvas.save(HERE / name, optimize=True)
        print(HERE / name, canvas.size)

if __name__ == "__main__":
    main()
