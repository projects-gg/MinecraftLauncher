# Builds the latin subsets of the launcher's text faces, Figtree and Tektur (both OFL-1.1, no
# Reserved Font Name), from the fontsource packages with one kerning pair added: "r" + dotless "ı".
#
# Turkish writes "rı" in most plurals and possessives ("Ayarları", "Arkadaşlarında"). The arm of
# "r" ends 0.07-0.09 em before the next stem, neither font kerns the pair, and antialiasing closes
# that gap at the launcher's sizes (12-17 px): "rı" read "n" ("Arkadaşlannda"). The pair opens it and
# moves nothing else: letter-spacing would loosen every word, and "ı" only occurs in Turkish (and
# Azerbaijani). The "ı" already comes from the latin file, the one "r" is in (U+0131 is in its
# unicode-range and declared last), so the pair applies; "ğ", "ş", "İ" come from latin-ext.
#
# Usage, from the repository root after `pnpm install` (needs fontTools and brotli):
#   python packages/ui/scripts/build-text-fonts.py
# Reads packages/ui/node_modules/@fontsource-variable/<family>/files/<family>-latin-wght-normal.woff2
# and writes packages/ui/src/fonts/<family>-latin-wght-normal.woff2 plus fonts.json (source versions
# and hashes: a launcher test keeps them in step with package.json and the installed packages).
import hashlib
import json
import sys
from pathlib import Path

from fontTools.otlLib.builder import buildLookup, buildPairPosGlyphsSubtable, buildValue
from fontTools.ttLib import TTFont

UI = Path(__file__).resolve().parent.parent
OUT = UI / "src" / "fonts"

# Units per em are 1000 in both. Measured (headless Edge and WebView2, greyscale antialiasing, 100
# and 125 %): with these the arm and the stem stay apart in every "rı" at 12-16 px (Figtree, 400-600)
# and 13-17 px (Tektur, 500-600); at 15.5 px reading size the pair still sets like its neighbours.
PAIRS = {
    "figtree": {("r", "dotlessi"): 70},
    "tektur": {("r", "dotlessi"): 80},
}
MARK = "Projects build: r + dotless i kerning"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def add_pairs(font: TTFont, pairs: dict[tuple[str, str], int]) -> None:
    """One pair-positioning lookup at the end of every "kern" feature (after the font's own)."""
    gpos = font["GPOS"].table
    glyphs = font.getReverseGlyphMap()
    for first, second in pairs:
        assert first in glyphs and second in glyphs, f"missing glyph {first} or {second}"
    table = {pair: (buildValue({"XAdvance": value}), None) for pair, value in pairs.items()}
    lookup = buildLookup([buildPairPosGlyphsSubtable(table, glyphs)])
    gpos.LookupList.Lookup.append(lookup)
    gpos.LookupList.LookupCount = len(gpos.LookupList.Lookup)
    index = gpos.LookupList.LookupCount - 1
    kern = [rec.Feature for rec in gpos.FeatureList.FeatureRecord if rec.FeatureTag == "kern"]
    assert kern, "no kern feature"
    for feature in kern:
        feature.LookupListIndex.append(index)
        feature.LookupCount = len(feature.LookupListIndex)


def mark_version(font: TTFont) -> None:
    """The version string says the file is ours (the OFL asks derivatives to be told apart)."""
    for record in font["name"].names:
        if record.nameID in (3, 5):
            text = record.toUnicode()
            if MARK not in text:
                record.string = f"{text}; {MARK}"


def main() -> None:
    manifest = {}
    for family, pairs in PAIRS.items():
        package = UI / "node_modules" / "@fontsource-variable" / family
        version = json.loads((package / "package.json").read_text(encoding="utf-8"))["version"]
        source = package / "files" / f"{family}-latin-wght-normal.woff2"
        font = TTFont(source)
        add_pairs(font, pairs)
        mark_version(font)
        font.flavor = "woff2"
        target = OUT / source.name
        font.save(target)
        manifest[family] = {
            "package": f"@fontsource-variable/{family}",
            "version": version,
            "source": sha256(source),
            "file": target.name,
            "sha256": sha256(target),
            "pairs": {f"{a} {b}": value for (a, b), value in pairs.items()},
        }
        print(f"{target.name}: {target.stat().st_size} bytes, from {family} {version}")
    # LF on every platform (write_text would write CRLF on Windows).
    (OUT / "fonts.json").write_bytes((json.dumps(manifest, indent=2) + "\n").encode("utf-8"))


if __name__ == "__main__":
    sys.exit(main())
