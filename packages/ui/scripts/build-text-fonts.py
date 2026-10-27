# Builds the latin subsets of the launcher's text faces, Figtree and Tektur (both OFL-1.1, no
# Reserved Font Name), from the fontsource packages with one kerning pair added, "r" + dotless "ı"
# (in Tektur fading out over display sizes: PAIR_FADE), and in Tektur a longer stem on "Y" (below,
# Y_SHOULDERS).
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
from fontTools.ttLib.tables import otTables as ot
from fontTools.ttLib.tables._f_v_a_r import Axis
from fontTools.varLib.builder import buildVarData, buildVarDevTable, buildVarRegion

UI = Path(__file__).resolve().parent.parent
OUT = UI / "src" / "fonts"

# Units per em are 1000 in both. Measured (headless Edge and WebView2, greyscale antialiasing, 100
# and 125 %): with these the arm and the stem stay apart in every "rı" at 12-16 px (Figtree, 400-600)
# and 13-17 px (Tektur, 500-600); at 15.5 px reading size the pair still sets like its neighbours.
PAIRS = {
    "figtree": {("r", "dotlessi"): 70},
    "tektur": {("r", "dotlessi"): 80},
}
# Tektur's pair fades out in display sizes (SMALL12): at 22 px and up antialiasing no longer closes
# the arm's gap, and +80 set the "ı" 3-5 px apart in titles ("Kurallar ı"). So Tektur gets an
# optical size axis ("opsz", 6-22, default 12) that moves nothing but its pairs: their full value up
# to 17 px, none from 22 px, in steps between. Browsers set opsz from the CSS font size themselves
# (`font-optical-sizing: auto`, the default), so no page changes. Figtree's text sizes stop at 17 px.
PAIR_FADE = {"tektur": {"axis": (6, 12, 22), "full": 17, "none": 22}}
# Tektur's capital Y meets its stem low: the shoulders sit at 0.21 em, so at 13-17 px the stem is
# two or three pixels under a wide V and "Yama notları" read "Vama notları". The four shoulder
# points move up to these heights at the light end of the weight axis (400) and the heavy end
# (900): 0.31 em at the titles' 600. They stay under the arms' inner junction at every weight
# (0.29 em at 400, 0.40 at 900), and the outer two slide along their arm's edge, so the arms keep
# their slope and width.
Y_SHOULDERS = {"tektur": (270, 370)}
MARKS = {
    "figtree": "Projects build: r + dotless i kerning",
    "tektur": "Projects build: r + dotless i kerning fading with optical size, longer Y stem",
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def add_opsz_axis(font: TTFont, low: int, default: int, high: int) -> None:
    """Appends an optical size axis that moves nothing yet: every named instance sits at its
    default, and every existing variation region (glyphs, advances, kerning) ignores it."""
    # gvar reads fvar's axis count when it decompiles: everything is read before the axis comes.
    font.ensureDecompiled()
    fvar = font["fvar"]
    assert all(axis.axisTag != "opsz" for axis in fvar.axes), "opsz already there"
    assert "avar" not in font and "MVAR" not in font and "cvar" not in font, "more tables to extend"
    name_id = font["name"].addMultilingualName(
        {"en": "Optical size"}, ttFont=font, mac=False, minNameID=256
    )
    axis = Axis()
    axis.axisTag = "opsz"
    axis.minValue, axis.defaultValue, axis.maxValue = float(low), float(default), float(high)
    axis.flags = 0
    axis.axisNameID = name_id
    fvar.axes.append(axis)
    for instance in fvar.instances:
        instance.coordinates["opsz"] = float(default)
    # gvar's tuples name their axes (one left out does not matter); the item variation stores
    # list every axis per region: the new one is neutral there (start = peak = end = 0).
    for tag in ("GDEF", "HVAR"):
        regions = font[tag].table.VarStore.VarRegionList
        for region in regions.Region:
            neutral = ot.VarRegionAxis()
            neutral.StartCoord = neutral.PeakCoord = neutral.EndCoord = 0.0
            region.VarRegionAxis.append(neutral)
        regions.RegionAxisCount = len(fvar.axes)
    stat = font["STAT"].table
    record = ot.AxisRecord()
    record.AxisTag = "opsz"
    record.AxisNameID = name_id
    record.AxisOrdering = len(stat.DesignAxisRecord.Axis)
    stat.DesignAxisRecord.Axis.append(record)
    stat.DesignAxisCount = len(stat.DesignAxisRecord.Axis)


def fade_devices(font: TTFont, values: list[int], fade: dict) -> list[ot.Device]:
    """Variation devices that take each value away between the "full" and "none" optical sizes:
    linearly, nothing below "full"; the axis ends at "none", so larger sizes stay at 0. One new
    region on the opsz axis and one delta set per value, in GDEF's variation store."""
    low, default, high = fade["axis"]
    assert fade["none"] == high and default < fade["full"] < high
    start = (fade["full"] - default) / (high - default)
    axis_tags = [axis.axisTag for axis in font["fvar"].axes]
    store = font["GDEF"].table.VarStore
    regions = store.VarRegionList
    regions.Region.append(buildVarRegion({"opsz": (start, 1.0, 1.0)}, axis_tags))
    regions.RegionCount = len(regions.Region)
    deltas = [[-value] for value in values]
    store.VarData.append(buildVarData([regions.RegionCount - 1], deltas, optimize=False))
    store.VarDataCount = len(store.VarData)
    outer = store.VarDataCount - 1
    return [buildVarDevTable((outer << 16) | inner) for inner in range(len(values))]


def add_pairs(font: TTFont, pairs: dict[tuple[str, str], int], fade: dict | None = None) -> None:
    """One pair-positioning lookup at the end of every "kern" feature (after the font's own),
    fading out over an optical size axis when `fade` says so."""
    gpos = font["GPOS"].table
    glyphs = font.getReverseGlyphMap()
    for first, second in pairs:
        assert first in glyphs and second in glyphs, f"missing glyph {first} or {second}"
    if fade:
        add_opsz_axis(font, *fade["axis"])
        devices = fade_devices(font, list(pairs.values()), fade)
        values = [
            buildValue({"XAdvance": value, "XAdvDevice": device})
            for value, device in zip(pairs.values(), devices)
        ]
    else:
        values = [buildValue({"XAdvance": value}) for value in pairs.values()]
    table = {pair: (value, None) for pair, value in zip(pairs, values)}
    lookup = buildLookup([buildPairPosGlyphsSubtable(table, glyphs)])
    gpos.LookupList.Lookup.append(lookup)
    gpos.LookupList.LookupCount = len(gpos.LookupList.Lookup)
    index = gpos.LookupList.LookupCount - 1
    kern = [rec.Feature for rec in gpos.FeatureList.FeatureRecord if rec.FeatureTag == "kern"]
    assert kern, "no kern feature"
    for feature in kern:
        feature.LookupListIndex.append(index)
        feature.LookupCount = len(feature.LookupListIndex)


def raise_y_shoulders(font: TTFont, light: int, heavy: int) -> None:
    """Moves the shoulders of "Y" (its four points at y = 210) to `light` at the axis default and
    `heavy` at its maximum, through the outline and its one gvar tuple."""
    glyph = font["glyf"]["Y"]
    variations = font["gvar"].variations["Y"]
    assert len(variations) == 1 and variations[0].axes == {"wght": (0.0, 1.0, 1.0)}, variations
    deltas = variations[0].coordinates  # one per point, then the four phantom points
    assert all(d is not None for d in deltas), "inferred deltas"
    base = list(glyph.coordinates)
    full = [(x + dx, y + dy) for (x, y), (dx, dy) in zip(base, deltas)]
    count = len(base)
    shoulder = [i for i, (_, y) in enumerate(base) if y == 210]
    assert len(shoulder) == 4, base
    left, right = min(shoulder, key=lambda i: base[i][0]), max(shoulder, key=lambda i: base[i][0])
    # The arms' inner junction: the two points between the shoulders' heights and the top.
    inner = [i for i, (_, y) in enumerate(base) if 210 < y < 700]
    assert len(inner) == 2, base

    def moved(points: list[tuple[float, float]], i: int, height: int) -> tuple[float, float]:
        x, y = points[i]
        if i in (left, right):
            # The outer edge runs to the arm's top: the next point on the left, the previous on the right.
            tx, ty = points[(i + 1) % count] if i == left else points[(i - 1) % count]
            x += (tx - x) / (ty - y) * (height - y)
        return (round(x), height)

    for points, height in ((base, light), (full, heavy)):
        assert all(height < points[i][1] for i in inner), "shoulders above the junction"
    for i in shoulder:
        nx, ny = moved(base, i, light)
        fx, fy = moved(full, i, heavy)
        glyph.coordinates[i] = (nx, ny)
        deltas[i] = (fx - nx, fy - ny)
    glyph.recalcBounds(font["glyf"])


def mark_version(font: TTFont, mark: str) -> None:
    """The version string says the file is ours (the OFL asks derivatives to be told apart)."""
    for record in font["name"].names:
        if record.nameID in (3, 5):
            text = record.toUnicode()
            if mark not in text:
                record.string = f"{text}; {mark}"


def main() -> None:
    manifest = {}
    for family, pairs in PAIRS.items():
        package = UI / "node_modules" / "@fontsource-variable" / family
        version = json.loads((package / "package.json").read_text(encoding="utf-8"))["version"]
        source = package / "files" / f"{family}-latin-wght-normal.woff2"
        # recalcTimestamp off: the same sources build the same bytes (fonts.json pins the hashes).
        font = TTFont(source, recalcTimestamp=False)
        add_pairs(font, pairs, PAIR_FADE.get(family))
        if family in Y_SHOULDERS:
            raise_y_shoulders(font, *Y_SHOULDERS[family])
        mark_version(font, MARKS[family])
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
            # Objects, not lists: Biome keeps them as written, so the file needs no formatting pass.
            **(
                {
                    "pairFade": {
                        "opszAxis": "-".join(str(v) for v in PAIR_FADE[family]["axis"]),
                        "fullTo": PAIR_FADE[family]["full"],
                        "noneFrom": PAIR_FADE[family]["none"],
                    }
                }
                if family in PAIR_FADE
                else {}
            ),
            **(
                {"yShoulders": dict(zip(("wght400", "wght900"), Y_SHOULDERS[family]))}
                if family in Y_SHOULDERS
                else {}
            ),
        }
        print(f"{target.name}: {target.stat().st_size} bytes, from {family} {version}")
    # LF on every platform (write_text would write CRLF on Windows).
    (OUT / "fonts.json").write_bytes((json.dumps(manifest, indent=2) + "\n").encode("utf-8"))


if __name__ == "__main__":
    sys.exit(main())
