"""Builds the overlay's bundled fonts from the launcher's two families (SIL Open Font License 1.1).

    python -m pip install fonttools
    python tools/build-fonts.py <dir with "Figtree[wght].ttf" and "Tektur[wdth,wght].ttf"> \
        src/main/resources/assets/projects_overlay

The sources are the Google Fonts releases (github.com/google/fonts, ofl/figtree and ofl/tektur). Each
weight the overlay uses becomes a small static TrueType file with only the characters a Turkish or
English interface shows (Latin, Latin-1, Latin Extended-A, common punctuation, the lira sign). Layout
tables go: Minecraft draws glyph by glyph without kerning or ligatures. Anything else (a Cyrillic name,
an emoji) falls back to the game's own fonts (the reference providers below).

The font definitions are written here too, one per GUI scale: the game rasterizes a TrueType glyph at
"size x oversample" pixels and samples it without filtering, so text is crisp only when the oversample
equals the GUI scale. The overlay picks the definition for the current scale (ui/Fonts.java).
"""
import json
import re
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

SRC = Path(sys.argv[1])
ASSETS = Path(sys.argv[2])
FONT_DIR = ASSETS / "font"

UNICODES = (
    list(range(0x20, 0x7F))
    + list(range(0xA0, 0x100))
    + list(range(0x100, 0x180))
    + [0x2010, 0x2011, 0x2013, 0x2014, 0x2018, 0x2019, 0x201A, 0x201C, 0x201D, 0x201E, 0x2022, 0x2026]
    + [0x2039, 0x203A, 0x20AC, 0x20BA, 0x2122, 0x2212]
)

# TrueType files: name -> (source, axis values)
FILES = {
    "figtree-medium": ("Figtree[wght].ttf", {"wght": 500}),
    "figtree-semibold": ("Figtree[wght].ttf", {"wght": 620}),
    "tektur-semibold": ("Tektur[wdth,wght].ttf", {"wdth": 100, "wght": 600}),
}

# Font definitions: name -> (file, size in GUI px, vertical shift in GUI px)
FACES = {
    "ui": ("figtree-medium", 8.5, 0.0),
    "ui_strong": ("figtree-semibold", 8.5, 0.0),
    "small": ("figtree-medium", 7.5, 0.0),
    "display": ("tektur-semibold", 9.0, 0.0),
    # A crew tag's chip (TEAM6): the launcher's TeamTag is Tektur too, 11 px in an 18 px chip.
    "tag": ("tektur-semibold", 6.0, 0.0),
}
SCALES = range(1, 9)


def build_file(name, source, axes):
    font = TTFont(SRC / source)
    static = instancer.instantiateVariableFont(font, axes, updateFontNames=False)
    options = subset.Options()
    options.layout_features = []
    options.drop_tables += ["GSUB", "GPOS", "GDEF", "STAT", "DSIG", "meta"]
    options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14]
    options.notdef_outline = True
    options.hinting = True
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=UNICODES)
    subsetter.subset(static)
    out = FONT_DIR / f"{name}.ttf"
    static.save(out)
    return out


def main():
    FONT_DIR.mkdir(parents=True, exist_ok=True)
    for name, (source, axes) in FILES.items():
        out = build_file(name, source, axes)
        print(f"{out.name}: {out.stat().st_size} bytes")
    for name, (file, size, shift) in FACES.items():
        for scale in SCALES:
            definition = {
                "providers": [
                    {
                        "type": "ttf",
                        "file": f"projects_overlay:{file}.ttf",
                        "size": size,
                        "oversample": float(scale),
                        "shift": [0.0, shift],
                        "skip": "",
                    },
                    {"type": "reference", "id": "minecraft:include/default"},
                    {"type": "reference", "id": "minecraft:include/unifont"},
                ]
            }
            path = FONT_DIR / f"{name}_{scale}.json"
            path.write_text(biome_style(json.dumps(definition, indent=2)) + "\n", encoding="utf-8", newline="\n")


def biome_style(text):
    """JSON as the repo's Biome formatter prints it: an array of numbers on one line ("shift": [0.0, 0.0])."""
    return re.sub(
        r"\[\s+([^\[\]{}\"]*?)\s+\]",
        lambda m: "[" + ", ".join(v.strip() for v in m.group(1).split(",")) + "]",
        text,
    )


if __name__ == "__main__":
    main()
