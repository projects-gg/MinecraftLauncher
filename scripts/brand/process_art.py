"""Convert generated art (PNG) into bundled WebP assets for the launcher.

Usage: python scripts/brand/process_art.py <art_dir> <out_dir>

- Wide art (hero, onboarding, news covers) -> WebP, max 1920px wide, quality 82.
- Square illustrations with alpha (empty states) -> WebP 512px, lossless alpha.
- instance-icons-sheet.png (4x2 grid) -> eight 128px WebP icons cropped to content.
"""

import sys
from pathlib import Path

from PIL import Image

ICON_NAMES = [
    "grass",
    "swords",
    "pickaxe",
    "potion",
    "dragon-egg",
    "tower",
    "map",
    "compass",
]


def save_wide(src: Path, dst: Path, max_w: int = 1920) -> None:
    im = Image.open(src).convert("RGB")
    if im.width > max_w:
        im = im.resize((max_w, round(im.height * max_w / im.width)), Image.LANCZOS)
    im.save(dst, "WEBP", quality=82, method=6)
    print(f"{dst.name}: {im.size}")


def save_square_alpha(src: Path, dst: Path, size: int = 512) -> None:
    im = Image.open(src).convert("RGBA")
    im = im.resize((size, size), Image.LANCZOS)
    im.save(dst, "WEBP", quality=90, method=6)
    print(f"{dst.name}: {im.size}")


def slice_icons(src: Path, out_dir: Path, size: int = 128) -> None:
    sheet = Image.open(src).convert("RGBA")
    cols, rows = 4, 2
    cw, ch = sheet.width / cols, sheet.height / rows
    for index, name in enumerate(ICON_NAMES):
        c, r = index % cols, index // cols
        cell = sheet.crop((round(c * cw), round(r * ch), round((c + 1) * cw), round((r + 1) * ch)))
        bbox = cell.getchannel("A").point(lambda a: 255 if a > 24 else 0).getbbox()
        if bbox:
            cell = cell.crop(bbox)
        # Pad to a square with 8% margin so all icons share an optical size.
        side = round(max(cell.width, cell.height) * 1.08)
        canvas = Image.new("RGBA", (side, side), (0, 0, 0, 0))
        canvas.alpha_composite(cell, ((side - cell.width) // 2, (side - cell.height) // 2))
        canvas = canvas.resize((size, size), Image.LANCZOS)
        dst = out_dir / f"icon-{name}.webp"
        canvas.save(dst, "WEBP", lossless=True, method=6)
        print(f"{dst.name}: {canvas.size}")


def main(art_dir: str, out_dir: str) -> None:
    src = Path(art_dir)
    out = Path(out_dir)
    (out / "icons").mkdir(parents=True, exist_ok=True)
    for name in ["hero-dragon-1", "onboarding-dragon-eye"]:
        if (src / f"{name}.png").exists():
            save_wide(src / f"{name}.png", out / f"{name}.webp")
    for path in sorted(src.glob("news-*.png")):
        save_wide(path, out / f"{path.stem}.webp", max_w=1280)
    for name in ["empty-library", "empty-offline"]:
        if (src / f"{name}.png").exists():
            save_square_alpha(src / f"{name}.png", out / f"{name}.webp")
    if (src / "instance-icons-sheet.png").exists():
        slice_icons(src / "instance-icons-sheet.png", out / "icons")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
