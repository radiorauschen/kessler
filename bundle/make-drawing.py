#!/usr/bin/env python3
"""The drawing for the menu's „über", as a mask kessler tints.

The scan is black ink on paper (here, transparent). kessler draws it in the accent of
whatever colour scheme is on, so what it needs is not a picture but how much
ink there is at each point: the scan's darkness, as one grey channel, white
where the line is. Paper that is not quite white is taken to nothing, ink
that is not quite black to the whole of it, and the specks of dust the
scanner saw beside the drawing are left out, so they are not drawn as
coloured dots in the corner of the sheet. Then it is cut to the drawing.

Run with the path of the scan (`bundle/drawing-scan.webp`); writes
`src/gui/drawing.png`. A second argument names another file in `src/gui`:
`bundle/drawing-off-scan.webp drawing-off.png` for the drawing over
„alles ist ausgeschaltet".
"""

import sys
from collections import deque
from pathlib import Path

from PIL import Image

PAPER = 60  # darkness up to which it is paper
INK = 170  # darkness from which it is the whole of the line
SPECK = 40  # an island of ink smaller than this many pixels is dust
MARGIN = 8  # pixels kept round the drawing when it is cut

def main() -> None:
    scan = Image.open(sys.argv[1]).convert("RGBA")
    paper = Image.new("RGBA", scan.size, (255, 255, 255, 255))
    scan = Image.alpha_composite(paper, scan).convert("L")
    width, height = scan.size
    dark = [255 - v for v in scan.tobytes()]
    ink = [
        0 if d <= PAPER else 255 if d >= INK else round((d - PAPER) * 255 / (INK - PAPER))
        for d in dark
    ]

    seen = bytearray(width * height)
    for start in range(width * height):
        if ink[start] == 0 or seen[start]:
            continue
        island = [start]
        seen[start] = 1
        queue = deque([start])
        while queue:
            at = queue.popleft()
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
    left, top, right, bottom = mask.getbbox()
    mask = mask.crop(
        (
            max(left - MARGIN, 0),
            max(top - MARGIN, 0),
            min(right + MARGIN, width),
            min(bottom + MARGIN, height),
        )
    )
    name = sys.argv[2] if len(sys.argv) > 2 else "drawing.png"
    out = Path(__file__).resolve().parent.parent / "src" / "gui" / name
    mask.save(out, optimize=True)
    print(f"{out}: {mask.size[0]}×{mask.size[1]}")

if __name__ == "__main__":
    main()
