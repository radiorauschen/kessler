#!/usr/bin/env python3
"""The Misaki font, as kessler draws it dot by dot.

Misaki (美咲フォント, Num Kadoma, littlelimit.net) is an 8×8 dot Japanese
bitmap face: kanji of JIS levels one and two, kana, Latin. Free software:
„Unlimited permission is granted to use, copy, and distribute it, with or
without modification, either commercially and noncommercially."

Its TrueType form has every dot as a square of 256 units on a grid of
2048, so it is read back to dots exactly: each dot's centre, inside the
outline or not. The result is a table kessler builds in, sorted by
character, each record

    u32 little-endian  the character
    u8                 its width in dots, 4 or 8
    8 × u8             its rows from the top, the leftmost dot the high bit

Run with the path of `misaki_gothic.ttf` (npm: @fontpkg/misaki-gothic,
2021.0505); writes `src/misaki/misaki.bin`.
"""

import struct
import sys
from pathlib import Path

from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont

UNIT = 256  # one dot, in font units


def contours(glyph_set, name):
    pen = RecordingPen()
    glyph_set[name].draw(pen)
    out, current = [], []
    for op, points in pen.value:
        if op == "moveTo":
            current = [points[0]]
        elif op == "lineTo":
            current.append(points[0])
        elif op in ("closePath", "endPath"):
            if current:
                out.append(current)
            current = []
        else:
            raise SystemExit(f"{name}: a curve ({op}) in a face of squares")
    return out


def winding(polygons, x, y):
    """The nonzero rule, as TrueType fills."""
    total = 0
    for poly in polygons:
        for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
            if y0 <= y < y1 and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
                total += 1
            elif y1 <= y < y0 and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
                total -= 1
    return total != 0


def main():
    font = TTFont(sys.argv[1])
    glyph_set = font.getGlyphSet()
    ascent = font["hhea"].ascent
    records = []
    for code, name in sorted(font.getBestCmap().items()):
        if code > 0xFFFF or code < 0x20:
            continue
        width = font["hmtx"][name][0] // UNIT
        if width not in (4, 8):
            raise SystemExit(f"U+{code:04X}: {width} dots wide")
        polygons = contours(glyph_set, name)
        rows = []
        for r in range(8):
            y = ascent - UNIT * r - UNIT // 2
            bits = 0
            for c in range(width):
                if winding(polygons, UNIT * c + UNIT // 2, y):
                    bits |= 0x80 >> c
            rows.append(bits)
        records.append(struct.pack("<IB8B", code, width, *rows))
    out = Path(__file__).resolve().parent.parent / "src" / "misaki" / "misaki.bin"
    out.parent.mkdir(exist_ok=True)
    out.write_bytes(b"".join(records))
    print(f"{len(records)} characters, {out.stat().st_size} bytes → {out}")


if __name__ == "__main__":
    main()
