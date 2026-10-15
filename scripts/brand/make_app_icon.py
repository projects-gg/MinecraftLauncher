"""Compose the 1024x1024 app icon source from the transparent dragon logo.

Usage: python scripts/brand/make_app_icon.py <logo.png> <out.png>
Then:  pnpm --filter @pl/launcher tauri icon <out.png>
"""

import sys

from PIL import Image, ImageDraw, ImageFilter

SCALE = 2  # supersample for smooth edges
SIZE = 1024 * SCALE
INSET = 36 * SCALE
RADIUS = 228 * SCALE


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def main(logo_path: str, out_path: str) -> None:
    # Diagonal obsidian gradient: warm top-left to near-black bottom-right.
    top, bottom = (40, 26, 17), (9, 6, 5)
    gradient = Image.new("RGB", (SIZE, SIZE))
    px = gradient.load()
    for y in range(0, SIZE, 2):
        for x in range(0, SIZE, 2):
            color = lerp(top, bottom, (x + y) / (2 * SIZE))
            px[x, y] = color
            px[x + 1, y] = color
            px[x, y + 1] = color
            px[x + 1, y + 1] = color

    # Soft amber glow behind the dragon.
    glow = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(glow).ellipse(
        (SIZE * 0.2, SIZE * 0.18, SIZE * 0.8, SIZE * 0.78), fill=95
    )
    glow = glow.filter(ImageFilter.GaussianBlur(160 * SCALE // 2))
    amber = Image.new("RGB", (SIZE, SIZE), (232, 150, 52))
    base = Image.composite(amber, gradient, glow)

    mask = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (INSET, INSET, SIZE - INSET, SIZE - INSET), radius=RADIUS, fill=255
    )
    icon = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    icon.paste(base, (0, 0), mask)

    # Hairline gold rim for a premium edge.
    rim = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(rim).rounded_rectangle(
        (INSET, INSET, SIZE - INSET, SIZE - INSET),
        radius=RADIUS,
        outline=(240, 190, 110, 70),
        width=4 * SCALE,
    )
    icon = Image.alpha_composite(icon, rim)

    logo = Image.open(logo_path).convert("RGBA")
    target = int(SIZE * 0.74)
    logo = logo.resize((target, target), Image.LANCZOS)
    offset = ((SIZE - target) // 2, (SIZE - target) // 2 + 6 * SCALE)
    icon.alpha_composite(logo, offset)

    icon.resize((1024, 1024), Image.LANCZOS).save(out_path)
    print(f"wrote {out_path}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
