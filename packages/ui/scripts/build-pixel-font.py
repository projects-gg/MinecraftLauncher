# Builds "Projects Pixel": the pixels of Idrees Hassan's "Minecraft Font" (OFL-1.1, no reserved
# name) on a clean whole-pixel grid, plus the Turkish letters it lacks (ğ Ğ ş Ş ı İ) and an
# ellipsis, drawn in the same style. One font pixel is 128 units and the em is 8 pixels, so at
# font-size 8n px every pixel covers n x n CSS pixels with its edges on whole pixels.
#
# Usage: python build-pixel-font.py <Minecraft.otf> <out-dir>   (needs fontTools and brotli)
# The source: https://github.com/IdreesInc/Minecraft-Font/releases/download/v1.0/Minecraft.otf
# Copy the output's projects-pixel.woff2 to src/components/inventory/fonts/.
# Writes projects-pixel.woff2, projects-pixel.ttf, glyphs.json (the bitmaps, for review) and
# pixel-glyphs.ts (the same bitmaps for the inventory, which draws the game's words as pixels; copy
# it to src/components/inventory/). `--ts <glyphs.json> <pixel-glyphs.ts>` writes only the module
# (plain Python, no fontTools).
import json
import sys
from pathlib import Path

SRC_UNIT = 2  # source: one pixel is 2 units, glyphs start 1 unit right of the pen
P = 128  # ours: one pixel
UPEM = 8 * P


def decode(src):
    """Every mapped glyph of the source as {char: (advance, {(col, row)})}, rows up from the baseline."""
    from fontTools.pens.boundsPen import BoundsPen
    from fontTools.pens.pointInsidePen import PointInsidePen

    gs = src.getGlyphSet()
    hmtx = src["hmtx"]
    out = {}
    for cp, name in src.getBestCmap().items():
        if cp < 0x20:
            continue
        adv = hmtx[name][0] // SRC_UNIT
        bp = BoundsPen(gs)
        gs[name].draw(bp)
        pixels = set()
        if bp.bounds:
            xmin, ymin, xmax, ymax = bp.bounds
            for row in range(int(ymin // SRC_UNIT) - 1, int(-(-ymax // SRC_UNIT)) + 1):
                for col in range(0, int(-(-xmax // SRC_UNIT)) + 1):
                    pen = PointInsidePen(gs, (1 + SRC_UNIT * col + 1, SRC_UNIT * row + 1))
                    gs[name].draw(pen)
                    if pen.getResult():
                        pixels.add((col, row))
        out[chr(cp)] = (adv, pixels)
    return out


def add_turkish(glyphs):
    """The Turkish letters the source lacks, built from its own shapes."""
    adv_i, i_pixels = glyphs["i"]
    top_i = max(r for _, r in i_pixels)
    glyphs["ı"] = (adv_i, {(c, r) for c, r in i_pixels if r < top_i - 1})

    adv_I, I_pixels = glyphs["I"]
    width_I = max(c for c, _ in I_pixels) + 1
    glyphs["İ"] = (adv_I, I_pixels | {(width_I // 2, 8)})

    # The cedilla as the source's ç draws it: a pixel under the bottom stroke, one down-left of it.
    # Under the small s it sits one pixel further left, where that letter's bottom stroke centres.
    def cedilla(base, shift):
        adv, pixels = glyphs[base]
        right = max(c for c, r in pixels if r == 0) - shift
        return adv, pixels | {(right, -1), (right - 1, -2)}

    glyphs["ş"] = cedilla("s", 1)
    glyphs["Ş"] = cedilla("S", 0)

    # The breve: two rows, a cup four pixels wide over the letter's middle.
    def breve(base, row):
        adv, pixels = glyphs[base]
        return adv, pixels | {(1, row + 1), (4, row + 1), (2, row), (3, row)}

    glyphs["ğ"] = breve("g", 6)
    glyphs["Ğ"] = breve("G", 8)

    # The ellipsis: three dots like the source's period, one pixel apart.
    glyphs["…"] = (6, {(0, 0), (2, 0), (4, 0)})


def runs(pixels):
    """Rectangles (col0, row0, col1, row1) covering the pixels: row runs merged down equal columns."""
    by_row = {}
    for c, r in pixels:
        by_row.setdefault(r, []).append(c)
    spans = {}
    for r, cols in by_row.items():
        cols.sort()
        start = prev = cols[0]
        for c in cols[1:] + [None]:
            if c is not None and c == prev + 1:
                prev = c
                continue
            spans.setdefault((start, prev), []).append(r)
            if c is not None:
                start = prev = c
    rects = []
    for (c0, c1), rows in spans.items():
        rows.sort()
        top = bottom = rows[0]
        for r in rows[1:] + [None]:
            if r is not None and r == top + 1:
                top = r
                continue
            rects.append((c0, bottom, c1 + 1, top + 1))
            if r is not None:
                bottom = top = r
    return rects


def glyph_name(ch):
    return "space" if ch == " " else f"uni{ord(ch):04X}"


def build(glyphs, out_dir: Path):
    from fontTools.fontBuilder import FontBuilder
    from fontTools.pens.ttGlyphPen import TTGlyphPen
    from fontTools.ttLib import TTFont

    order = [".notdef"] + [glyph_name(ch) for ch in sorted(glyphs)]
    fb = FontBuilder(UPEM, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({ord(ch): glyph_name(ch) for ch in glyphs})
    tt_glyphs = {}
    metrics = {}
    # .notdef: an empty box, the size of a capital.
    pen = TTGlyphPen(None)
    for x0, y0, x1, y1 in [(0, 0, 5, 1), (0, 6, 5, 7), (0, 1, 1, 6), (4, 1, 5, 6)]:
        pen.moveTo((x0 * P, y0 * P))
        pen.lineTo((x0 * P, y1 * P))
        pen.lineTo((x1 * P, y1 * P))
        pen.lineTo((x1 * P, y0 * P))
        pen.closePath()
    tt_glyphs[".notdef"] = pen.glyph()
    metrics[".notdef"] = (6 * P, 0)
    for ch, (adv, pixels) in glyphs.items():
        pen = TTGlyphPen(None)
        for c0, r0, c1, r1 in runs(pixels):
            pen.moveTo((c0 * P, r0 * P))
            pen.lineTo((c0 * P, r1 * P))
            pen.lineTo((c1 * P, r1 * P))
            pen.lineTo((c1 * P, r0 * P))
            pen.closePath()
        name = glyph_name(ch)
        tt_glyphs[name] = pen.glyph()
        lsb = min((c for c, _ in pixels), default=0) * P
        metrics[name] = (adv * P, lsb)
    fb.setupGlyf(tt_glyphs)
    fb.setupHorizontalMetrics(metrics)
    # Ascent 8 and descent 2 pixels: a line box of 10 (the game's tooltip pitch) puts the baseline
    # on a whole pixel. Accents of capitals reach two rows higher, into the line above, as in game.
    fb.setupHorizontalHeader(ascent=8 * P, descent=-2 * P, lineGap=0)
    fb.setupOS2(
        sTypoAscender=8 * P,
        sTypoDescender=-2 * P,
        sTypoLineGap=0,
        usWinAscent=10 * P,
        usWinDescent=2 * P,
        sxHeight=5 * P,
        sCapHeight=7 * P,
        fsSelection=0x40 | 0x80,  # regular, use typo metrics
        achVendID="PJGG",
        usWeightClass=400,
        version=4,
    )
    fb.setupNameTable(
        {
            "familyName": "Projects Pixel",
            "styleName": "Regular",
            "uniqueFontIdentifier": "Projects Pixel Regular 1.0",
            "fullName": "Projects Pixel Regular",
            "psName": "ProjectsPixel-Regular",
            "version": "Version 1.000",
            "copyright": "Copyright (c) 2022, Idrees Hassan (https://github.com/IdreesInc/Minecraft-Font); "
            "Turkish letters and whole-pixel metrics (c) 2026, Projects",
            "description": "A whole-pixel rebuild of Minecraft Font by Idrees Hassan, with Turkish letters.",
            "licenseDescription": "This Font Software is licensed under the SIL Open Font License, Version 1.1.",
            "licenseInfoURL": "https://openfontlicense.org",
        }
    )
    fb.setupPost(isFixedPitch=0, underlinePosition=-P, underlineThickness=P)
    fb.font["head"].flags |= 1 << 3  # integer scaling (ppem rounding)
    fb.save(str(out_dir / "projects-pixel.ttf"))
    font = TTFont(str(out_dir / "projects-pixel.ttf"))
    font.flavor = "woff2"
    font.save(str(out_dir / "projects-pixel.woff2"))


def ts_key(ch):
    """A glyph's key as Biome writes it: bare when a name, quoted otherwise, escaped when unseen."""
    if ch == "$" or ch.isidentifier():
        return ch
    if ch == '"':
        return "'\"'"
    if ch == " " or (ch.isprintable() and not ch.isspace()):
        return json.dumps(ch, ensure_ascii=False)
    return json.dumps(ch, ensure_ascii=True)


def write_ts(review, path):
    """The bitmaps as a TypeScript module: [advance, top, ...rows as bit masks (bit 0 = column 0)]."""
    lines = [
        "// Generated by packages/ui/scripts/build-pixel-font.py from the Projects Pixel glyphs (Minecraft",
        "// Font by Idrees Hassan, OFL-1.1; fonts/OFL.txt, fonts/FONTLOG.txt). Do not edit by hand.",
        "// A glyph: [advance, top row, ...its rows from the top down as bit masks, bit 0 the pen's",
        "// column]; rows count up from the baseline (0 is the row just above it, negatives go below).",
        "export const PIXEL_GLYPHS: Readonly<Record<string, readonly number[]>> = {",
    ]
    for ch in sorted(review):
        glyph = review[ch]
        rows = glyph["rows"]
        masks = [sum(1 << c for c, bit in enumerate(row) if bit == "#") for row in rows]
        values = [glyph["advance"], glyph.get("top", 0), *masks]
        lines.append(f"  {ts_key(ch)}: [{', '.join(str(v) for v in values)}],")
    lines.append("};")
    with open(path, "w", encoding="utf-8", newline="\n") as out:
        out.write("\n".join(lines) + "\n")


def main():
    if sys.argv[1] == "--ts":
        write_ts(json.loads(Path(sys.argv[2]).read_text(encoding="utf-8")), sys.argv[3])
        return
    from fontTools.ttLib import TTFont

    src = TTFont(sys.argv[1])
    out_dir = Path(sys.argv[2])
    out_dir.mkdir(parents=True, exist_ok=True)
    glyphs = decode(src)
    add_turkish(glyphs)
    build(glyphs, out_dir)
    review = {}
    for ch, (adv, pixels) in sorted(glyphs.items()):
        if not pixels:
            review[ch] = {"advance": adv, "rows": []}
            continue
        top = max(r for _, r in pixels)
        bottom = min(r for _, r in pixels)
        width = max(c for c, _ in pixels) + 1
        rows = ["".join("#" if (c, r) in pixels else "." for c in range(width)) for r in range(top, bottom - 1, -1)]
        review[ch] = {"advance": adv, "top": top, "rows": rows}
    (out_dir / "glyphs.json").write_text(json.dumps(review, ensure_ascii=False, indent=1), encoding="utf-8")
    write_ts(review, out_dir / "pixel-glyphs.ts")
    print("glyphs", len(glyphs))


if __name__ == "__main__":
    main()
