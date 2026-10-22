#!/usr/bin/env python3
"""The launcher's embedded colour emoji font (CHAT2 §3.2): Noto Color Emoji 2.051 (COLRv1, SIL Open
Font License 1.1), subset to the Unicode Emoji 17.0 emoji the picker offers and sliced as
emoji-data.mjs plans it, one WOFF2 per slice next to emoji.css.

Usage (from the repository root, after emoji-data.mjs):
    python apps/launcher/scripts/emoji-font.py --plan <cache>/font-plan.json [--font <Noto-COLRv1.ttf>]

Needs fontTools with WOFF2 support (pip install "fonttools[woff]==4.60.1"). Without --font the pinned
font is downloaded next to the plan; its SHA-256 is checked either way. Each slice is renamed "PL
Emoji" (a modified version under the OFL keeps the copyright and the licence; no reserved name), drops
the vertical metrics nobody uses and keeps every glyph its code points need, the sequences included.
"""

import argparse
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

FONT_VERSION = "2.051"
FONT_URL = f"https://raw.githubusercontent.com/googlefonts/noto-emoji/v{FONT_VERSION}/fonts/Noto-COLRv1.ttf"
FONT_SHA256 = "0ae57fe58645638523ba35f388d93739d292539a9acb84df5700c81b1e1a28d2"
LICENSE_URL = f"https://raw.githubusercontent.com/googlefonts/noto-emoji/v{FONT_VERSION}/fonts/LICENSE"
LICENSE_SHA256 = "6a73f9541c2de74158c0e7cf6b0a58ef774f5a780bf191f2d7ec9cc53efe2bf2"
# chat2.md §3.2: the font may add at most 4 MB to the installer.
BUDGET = 4 * 1024 * 1024


def fetch(url: str, sha256: str, path: Path) -> bytes:
    if path.exists():
        data = path.read_bytes()
    else:
        request = urllib.request.Request(url, headers={"User-Agent": "projects-launcher-emoji-font"})
        with urllib.request.urlopen(request) as response:
            data = response.read()
    actual = hashlib.sha256(data).hexdigest()
    if actual != sha256:
        sys.exit(f"{url}: SHA-256 {actual}, expected {sha256}")
    path.write_bytes(data)
    return data


def rename(font: TTFont, face: str) -> None:
    name = font["name"]
    family = "PL Emoji"
    for record in list(name.names):
        if record.nameID == 16 or record.nameID == 17:
            name.removeNames(nameID=record.nameID)
    name.setName(family, 1, 3, 1, 0x409)
    name.setName(family, 1, 1, 0, 0)
    name.setName(f"{family} {face}", 3, 3, 1, 0x409)
    name.setName(f"{family} {face}", 3, 1, 0, 0)
    name.setName(family, 4, 3, 1, 0x409)
    name.setName(family, 4, 1, 0, 0)
    version = f"Version {FONT_VERSION}; Noto Color Emoji subset for Projects Launcher ({face})"
    name.setName(version, 5, 3, 1, 0x409)
    name.setName(version, 5, 1, 0, 0)
    name.setName(f"PLEmoji-{face}", 6, 3, 1, 0x409)
    name.setName(f"PLEmoji-{face}", 6, 1, 0, 0)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--plan", required=True)
    parser.add_argument("--font")
    args = parser.parse_args()

    plan_path = Path(args.plan)
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    out_dir = Path(plan["outDir"])
    font_path = Path(args.font) if args.font else plan_path.with_name("Noto-COLRv1.ttf")
    fetch(FONT_URL, FONT_SHA256, font_path)
    licence = fetch(LICENSE_URL, LICENSE_SHA256, plan_path.with_name("Noto-LICENSE.txt"))

    total = 0
    for face in plan["faces"]:
        options = subset.Options()
        options.flavor = "woff2"
        options.layout_features = ["*"]
        options.name_IDs = ["*"]
        options.name_languages = ["*"]
        options.name_legacy = True
        options.notdef_outline = False
        options.glyph_names = False
        options.hinting = False
        options.drop_tables += ["vhea", "vmtx"]
        font = TTFont(font_path)
        cmap = font.getBestCmap()
        missing = [cp for cp in face["codepoints"] if cp not in cmap]
        if missing:
            sys.exit(f"{face['id']}: the font lacks {', '.join(f'U+{cp:04X}' for cp in missing)}")
        subsetter = subset.Subsetter(options=options)
        subsetter.populate(unicodes=face["codepoints"])
        subsetter.subset(font)
        rename(font, face["id"])
        target = out_dir / face["file"]
        font.flavor = "woff2"
        font.save(target)
        size = target.stat().st_size
        total += size
        print(f"{face['file']}: {size / 1024:.0f} KB, {len(font.getGlyphOrder())} glyphs")

    header = (
        "PL Emoji (pl-emoji-*.woff2) is a subset of Noto Color Emoji "
        f"{FONT_VERSION} (COLRv1), Copyright 2022 Google Inc.,\n"
        "from https://github.com/googlefonts/noto-emoji, made by apps/launcher/scripts/emoji-font.py.\n"
        "It is licensed under the SIL Open Font License, Version 1.1, as below.\n\n"
    )
    (out_dir / "LICENSE.txt").write_text(header + licence.decode("utf-8"), encoding="utf-8", newline="\n")
    print(f"total {total / 1024 / 1024:.2f} MB (budget {BUDGET / 1024 / 1024:.0f} MB)")
    if total > BUDGET:
        sys.exit("over the budget")


if __name__ == "__main__":
    main()
