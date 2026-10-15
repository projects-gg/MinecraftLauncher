"""Generate the branded NSIS installer bitmaps for Projects Launcher and Projects Studio.

Usage (from the repo root, after `pnpm install` so the Tektur font is available):

    python scripts/release/make_installer_art.py [--preview <dir>]

Writes 24-bit BMP files (the only format NSIS accepts):

    apps/launcher/src-tauri/nsis/header.bmp    150x57   inner pages (MUI header image)
    apps/launcher/src-tauri/nsis/sidebar.bmp   164x314  welcome / finish pages
    apps/studio/src-tauri/nsis/header.bmp
    apps/studio/src-tauri/nsis/sidebar.bmp

Sources: assets/brand/ProjectsLogo.png (dragon emblem) and assets/brand/ProjectsLogoLong.png
(PROJECTS.GG wordmark). Colors follow docs/design.md (obsidian + gold, ember shadows).
`--preview` also writes enlarged PNG previews for review. Requires Pillow.
"""

from __future__ import annotations

import argparse
import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[2]
BRAND = ROOT / "assets" / "brand"
FONT = (
    ROOT
    / "packages/ui/node_modules/@fontsource-variable/tektur/files/tektur-latin-wght-normal.woff2"
)

SS = 4  # supersampling factor; everything is drawn large and downscaled once

OBSIDIAN_TOP = (21, 18, 26)  # between obsidian-900 #121016 and obsidian-800
OBSIDIAN_950 = (10, 9, 12)  # #0A090C
EMBER_900 = (63, 10, 10)  # #3F0A0A
GOLD_300 = (246, 213, 142)  # #F6D58E
GOLD_500 = (232, 154, 44)  # #E89A2C
GOLD_600 = (201, 116, 26)  # #C9741A


def lerp(a: tuple[int, ...], b: tuple[int, ...], t: float) -> tuple[int, ...]:
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


def gradient(size: tuple[int, int], top: tuple[int, ...], bottom: tuple[int, ...], vertical=True):
    w, h = size
    strip = Image.new("RGB", (1, h) if vertical else (w, 1))
    n = h if vertical else w
    for i in range(n):
        color = lerp(top, bottom, i / max(n - 1, 1))
        strip.putpixel((0, i) if vertical else (i, 0), color)
    return strip.resize(size, Image.BILINEAR).convert("RGBA")


def glow(size, center, radius, color, alpha):
    """Soft radial light: a blurred filled ellipse in `color` at `alpha` (0-255)."""
    w, h = size
    mask = Image.new("L", size, 0)
    cx, cy = center
    ImageDraw.Draw(mask).ellipse((cx - radius, cy - radius, cx + radius, cy + radius), fill=alpha)
    mask = mask.filter(ImageFilter.GaussianBlur(radius * 0.55))
    layer = Image.new("RGBA", (w, h), color + (0,))
    layer.putalpha(mask)
    return layer


def emblem(height: int) -> Image.Image:
    logo = Image.open(BRAND / "ProjectsLogo.png").convert("RGBA")
    bbox = logo.getchannel("A").point(lambda v: 255 if v > 8 else 0).getbbox()
    logo = logo.crop(bbox)
    width = round(logo.width * height / logo.height)
    return logo.resize((width, height), Image.LANCZOS)


def wordmark(width: int) -> Image.Image:
    """The PROJECTS.GG lettering from the long logo (text rows only, no underline accents)."""
    long_logo = Image.open(BRAND / "ProjectsLogoLong.png").convert("RGBA")
    text = long_logo.crop((778, 494, 1657, 588))
    bbox = text.getchannel("A").point(lambda v: 255 if v > 24 else 0).getbbox()
    text = text.crop(bbox)
    height = round(text.height * width / text.width)
    return text.resize((width, height), Image.LANCZOS)


def label(text: str, px: int, color, weight: int = 620) -> Image.Image:
    if not FONT.exists():
        raise SystemExit(f"Tektur font not found at {FONT} - run `pnpm install` first.")
    font = ImageFont.truetype(str(FONT), px)
    font.set_variation_by_axes([weight])
    left, top, right, bottom = font.getbbox(text)
    img = Image.new("RGBA", (right - left + 2, bottom - top + 2), color + (0,))
    ImageDraw.Draw(img).text((-left + 1, -top + 1), text, font=font, fill=color + (255,))
    return img


def embers(canvas: Image.Image, area, count: int, seed: int) -> None:
    """A few tiny ember sparks; deterministic so regenerated files stay byte-identical."""
    rng = random.Random(seed)
    draw = ImageDraw.Draw(canvas)
    x0, y0, x1, y1 = area
    for _ in range(count):
        x = rng.uniform(x0, x1)
        y = rng.uniform(y0, y1)
        r = rng.choice((0.6, 0.8, 1.0, 1.2)) * SS
        color = rng.choice((GOLD_300, GOLD_500, GOLD_600))
        alpha = rng.randint(60, 140)
        draw.ellipse((x - r, y - r, x + r, y + r), fill=color + (alpha,))


def paste_center_x(canvas: Image.Image, img: Image.Image, y: int) -> None:
    canvas.alpha_composite(img, ((canvas.width - img.width) // 2, y))


def sidebar(app_word: str | None) -> Image.Image:
    w, h = 164 * SS, 314 * SS
    canvas = gradient((w, h), OBSIDIAN_TOP, OBSIDIAN_950)
    center = (w // 2, 124 * SS)
    canvas.alpha_composite(glow((w, h), center, 92 * SS, EMBER_900, 210))
    canvas.alpha_composite(glow((w, h), center, 58 * SS, GOLD_600, 70))
    embers(canvas, (18 * SS, 244 * SS, 146 * SS, 300 * SS), 9, seed=7)
    embers(canvas, (20 * SS, 40 * SS, 144 * SS, 64 * SS), 4, seed=11)

    mark = emblem(118 * SS)
    paste_center_x(canvas, mark, 64 * SS)
    words = wordmark(134 * SS)
    paste_center_x(canvas, words, 196 * SS)
    if app_word:
        tag = label(app_word, 15 * SS, GOLD_300)
        paste_center_x(canvas, tag, 218 * SS)

    # Gold hairline where the art meets the white page.
    ImageDraw.Draw(canvas).rectangle((w - SS, 0, w, h), fill=GOLD_600 + (110,))
    return canvas.resize((164, 314), Image.LANCZOS).convert("RGB")


def header(app_word: str) -> Image.Image:
    w, h = 150 * SS, 57 * SS
    canvas = gradient((w, h), OBSIDIAN_TOP, OBSIDIAN_950, vertical=False)
    center = (30 * SS, h // 2)
    canvas.alpha_composite(glow((w, h), center, 34 * SS, EMBER_900, 220))
    canvas.alpha_composite(glow((w, h), center, 22 * SS, GOLD_600, 60))

    mark = emblem(45 * SS)
    canvas.alpha_composite(mark, (8 * SS, (h - mark.height) // 2))
    tag = label(app_word, 16 * SS, GOLD_300)
    canvas.alpha_composite(tag, (60 * SS, (h - tag.height) // 2))

    ImageDraw.Draw(canvas).rectangle((0, h - SS, w, h), fill=GOLD_600 + (130,))
    return canvas.resize((150, 57), Image.LANCZOS).convert("RGB")


def save_bmp(img: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    img.save(path, format="BMP")  # RGB -> 24-bit uncompressed BMP
    print(f"wrote {path.relative_to(ROOT)} {img.size}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--preview", type=Path, help="also write 3x PNG previews here")
    args = parser.parse_args()

    apps = {
        "launcher": {"header": header("Launcher"), "sidebar": sidebar(None)},
        "studio": {"header": header("Studio"), "sidebar": sidebar("Studio")},
    }
    for app, images in apps.items():
        for name, img in images.items():
            save_bmp(img, ROOT / "apps" / app / "src-tauri" / "nsis" / f"{name}.bmp")
            if args.preview:
                args.preview.mkdir(parents=True, exist_ok=True)
                big = img.resize((img.width * 3, img.height * 3), Image.NEAREST)
                big.save(args.preview / f"{app}-{name}.png")


if __name__ == "__main__":
    main()
