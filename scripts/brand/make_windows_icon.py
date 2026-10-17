"""Windows/Linux app icon: the dragon emblem filling the whole icon box.

Windows draws icons edge to edge (taskbar, Start, tray), so the rounded-square plate of the macOS
icon made ours look small next to other apps: on a dark taskbar the dark plate disappears and only
the emblem (74 % of the box) remains. Here the emblem's gold ring touches the edges and sits on a
dark disc, which also keeps it readable on a light taskbar.

Usage: python scripts/brand/make_windows_icon.py <logo.png> <icons-dir> [<source-out.png>]
  <logo.png>   the transparent emblem whose ring touches the image edges (assets/brand/ProjectsLogo.png)
  <icons-dir>  the Tauri icons folder; icon.ico and the Linux PNGs are replaced
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

SS = 2  # supersampling for smooth edges
SIZE = 1024
EMBLEM = 0.985  # the ring's outer edge, as a share of the box
DISC = 0.94  # dark disc behind the emblem, just inside the ring
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
LINUX_PNGS = {"32x32.png": 32, "64x64.png": 64, "128x128.png": 128, "128x128@2x.png": 256, "icon.png": 512}


def disc_layer(size: int) -> Image.Image:
    """Dark radial gradient disc: warm centre, near-black edge."""
    layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    diameter = int(size * DISC)
    offset = (size - diameter) // 2
    centre, edge = (46, 28, 18), (12, 8, 7)
    gradient = Image.new("RGB", (diameter, diameter))
    px = gradient.load()
    r = diameter / 2
    for y in range(diameter):
        for x in range(diameter):
            t = min(1.0, (((x - r) ** 2 + (y - r * 0.85) ** 2) ** 0.5) / r)
            px[x, y] = tuple(int(centre[i] + (edge[i] - centre[i]) * t) for i in range(3))
    mask = Image.new("L", (diameter, diameter), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, diameter - 1, diameter - 1), fill=255)
    layer.paste(gradient, (offset, offset), mask)
    return layer


def compose(logo_path: str) -> Image.Image:
    big = SIZE * SS
    icon = disc_layer(big)
    logo = Image.open(logo_path).convert("RGBA")
    target = int(big * EMBLEM)
    logo = logo.resize((target, target), Image.LANCZOS)
    offset = (big - target) // 2
    icon.alpha_composite(logo, (offset, offset))
    return icon.resize((SIZE, SIZE), Image.LANCZOS)


def downscale(source: Image.Image, size: int) -> Image.Image:
    """Resize with a light sharpen for the small sizes, where LANCZOS alone looks soft."""
    image = source.resize((size, size), Image.LANCZOS)
    if size <= 48:
        image = image.filter(ImageFilter.UnsharpMask(radius=0.6, percent=60, threshold=1))
    return image


def main(logo_path: str, icons_dir: str, source_out: str | None) -> None:
    source = compose(logo_path)
    if source_out:
        source.save(source_out)
        print(f"wrote {source_out}")
    out = Path(icons_dir)
    frames = [downscale(source, s) for s in ICO_SIZES]
    frames[-1].save(out / "icon.ico", format="ICO", sizes=[(s, s) for s in ICO_SIZES],
                    append_images=frames[:-1])
    print(f"wrote {out / 'icon.ico'} ({', '.join(map(str, ICO_SIZES))})")
    for name, size in LINUX_PNGS.items():
        downscale(source, size).save(out / name)
        print(f"wrote {out / name}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else None)
