"""Builds the overlay's colour emoji (CHAT-EMOJI; the owner, 26 Oct: reactions showed odd glyphs): the launcher's emoji
set drawn as pictures the game's font can show, so a reaction, a message from the launcher or the phone, a name with an
emoji reads as it does in the launcher. Minecraft's own font has no colour emoji (its unifont glyphs are one colour and
small), but its fonts take bitmap providers: one picture per character, drawn at whole device pixels.

    python -m pip install pillow imagequant
    python tools/build-emoji.py <launcher emoji TSV> <download cache dir> src/main/resources/assets/projects_overlay

The emoji are the launcher's (apps/launcher/src/features/emoji/data/emoji.en.tsv: Unicode Emoji 17.0, the picker's
order) that are one code point (a text-style selector after it does not count); sequences (skin tones, families, flags)
keep the game's fallback for their parts. The pictures are Noto Color Emoji's PNGs at the font's version (2.051, the
launcher's "PL Emoji"), Apache License 2.0 (noto-emoji's README: "Tools and most image resources are under the Apache
license"); the licence and a notice go next to the atlases. Each GUI scale gets its own atlas, a cell of 8 GUI pixels at
that scale (16, 24, 32 device pixels for GUI 2, 3, 4; 5 and up reuse the largest), resized once here with a good filter in
premultiplied alpha, so the game samples each picture 1:1. GUI 1 draws the game's own font (ui/Fonts.java) and stays as it
is. The atlases are reduced to 256 colours with libimagequant (as pngquant does; full colour made 5 MB a jar, this 1.1 MB),
which leaves the pictures as they were at these sizes.

Writes textures/font/emoji_<size>.png, font/emoji_<scale>.json (a bitmap provider each), the licence, and adds a
reference to the emoji font of the same scale to every face's definition (build-fonts.py writes the same).
"""
import concurrent.futures
import json
import math
import re
import sys
import urllib.request
from pathlib import Path

import imagequant
from PIL import Image

TSV = Path(sys.argv[1])
CACHE = Path(sys.argv[2])
ASSETS = Path(sys.argv[3])
VERSION = "2.051"
URL = "https://raw.githubusercontent.com/googlefonts/noto-emoji/v{v}/png/128/emoji_u{name}.png"
LICENSE_URL = "https://raw.githubusercontent.com/googlefonts/noto-emoji/v{v}/LICENSE"
# The emoji's height in GUI pixels: a little under the faces' line, as a capital with its ascent.
HEIGHT = 8
ASCENT = 7
SIZES = {2: 16, 3: 24, 4: 32}
MAX_SCALE = 8
COLUMNS = 40
FACES = ["ui", "ui_strong", "small", "display", "tag", "face"]


def emoji_list():
    """The single-code-point emoji of the launcher's list, in its order."""
    out = []
    for line in TSV.read_text(encoding="utf-8").splitlines()[1:]:
        if not line.strip():
            continue
        emoji = line.split("\t", 1)[0]
        cps = [ord(c) for c in emoji if ord(c) != 0xFE0F]
        if len(cps) == 1 and cps[0] not in out:
            out.append(cps[0])
    return out


def fetch(url, path):
    if path.exists():
        return path.read_bytes()
    request = urllib.request.Request(url, headers={"User-Agent": "projects-overlay-emoji"})
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read()
    path.write_bytes(data)
    return data


def picture(cp):
    name = f"{cp:04x}"
    path = CACHE / f"emoji_u{name}.png"
    try:
        fetch(URL.format(v=VERSION, name=name), path)
    except Exception as error:  # noqa: BLE001 - a missing picture leaves the character to the game's font
        print(f"no picture for U+{cp:04X}: {error}", file=sys.stderr)
        return None
    return path


def square(image, size):
    """The picture's drawn part, fitted into a size x size cell, resized in premultiplied alpha (no dark fringe)."""
    image = image.convert("RGBA")
    box = image.getbbox() or (0, 0, image.width, image.height)
    image = image.crop(box)
    scale = size / max(image.width, image.height)
    w = max(1, round(image.width * scale))
    h = max(1, round(image.height * scale))
    small = image.convert("RGBa").resize((w, h), Image.LANCZOS).convert("RGBA")
    cell = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    cell.paste(small, ((size - w) // 2, (size - h) // 2))
    return cell


def biome_style(text):
    """JSON as the repo's Biome formatter prints it (build-fonts.py's): an array of numbers on one line."""
    return re.sub(
        r"\[\s+([^\[\]{}\"]*?)\s+\]",
        lambda m: "[" + ", ".join(v.strip() for v in m.group(1).split(",")) + "]",
        text,
    )


def main():
    CACHE.mkdir(parents=True, exist_ok=True)
    cps = emoji_list()
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        paths = list(pool.map(picture, cps))
    pairs = [(cp, p) for cp, p in zip(cps, paths) if p is not None]
    rows = math.ceil(len(pairs) / COLUMNS)
    textures = ASSETS / "textures" / "font"
    textures.mkdir(parents=True, exist_ok=True)
    for size in sorted(set(SIZES.values())):
        atlas = Image.new("RGBA", (COLUMNS * size, rows * size), (0, 0, 0, 0))
        for i, (_, path) in enumerate(pairs):
            with Image.open(path) as image:
                atlas.paste(square(image, size), ((i % COLUMNS) * size, (i // COLUMNS) * size))
        reduced = imagequant.quantize_pil_image(atlas, dithering_level=0.5, max_colors=256, min_quality=0,
                                                max_quality=100)
        reduced.save(textures / f"emoji_{size}.png", optimize=True)
    chars = []
    for r in range(rows):
        row = [chr(cp) for cp, _ in pairs[r * COLUMNS:(r + 1) * COLUMNS]]
        row += ["\u0000"] * (COLUMNS - len(row))
        chars.append("".join(row))
    fonts = ASSETS / "font"
    for scale in range(2, MAX_SCALE + 1):
        size = SIZES.get(scale, max(SIZES.values()))
        provider = {
            "type": "bitmap",
            "file": f"projects_overlay:font/emoji_{size}.png",
            "height": HEIGHT,
            "ascent": ASCENT,
            "chars": chars,
        }
        (fonts / f"emoji_{scale}.json").write_text(
            json.dumps({"providers": [provider]}, ensure_ascii=True, indent=2) + "\n", encoding="utf-8", newline="\n")
        for face in FACES:
            path = fonts / f"{face}_{scale}.json"
            definition = json.loads(path.read_text(encoding="utf-8"))
            ref = {"type": "reference", "id": f"projects_overlay:emoji_{scale}"}
            providers = [p for p in definition["providers"] if p != ref]
            # After the face's own letters, before the game's fonts: an emoji the face lacks is ours.
            at = next((i for i, p in enumerate(providers) if p.get("type") == "reference"), len(providers))
            providers.insert(at, ref)
            definition["providers"] = providers
            path.write_text(biome_style(json.dumps(definition, indent=2)) + "\n", encoding="utf-8", newline="\n")
    # The characters, as ranges of hex code points, for the overlay's text: an emoji in white keeps its picture's colours.
    ranges = []
    for cp in sorted(cp for cp, _ in pairs):
        if ranges and ranges[-1][1] == cp - 1:
            ranges[-1][1] = cp
        else:
            ranges.append([cp, cp])
    lines = [f"{a:x}-{b:x}" if a != b else f"{a:x}" for a, b in ranges]
    (fonts / "emoji_chars.txt").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    notice = textures / "emoji-LICENSE.txt"
    license_text = fetch(LICENSE_URL.format(v=VERSION), CACHE / "LICENSE").decode("utf-8")
    notice.write_text(
        f"The pictures in emoji_16.png, emoji_24.png and emoji_32.png are Noto Color Emoji {VERSION}'s PNG images\n"
        "(https://github.com/googlefonts/noto-emoji, png/128), Copyright Google Inc., resized and placed in one\n"
        "picture each by mods/projects-overlay/tools/build-emoji.py, under the Apache License, Version 2.0:\n\n"
        + license_text,
        encoding="utf-8", newline="\n")
    sizes = {p.name: p.stat().st_size for p in textures.glob("emoji_*.png")}
    print(f"{len(pairs)} emoji of {len(cps)}, {rows} rows; {sizes}")


main()
