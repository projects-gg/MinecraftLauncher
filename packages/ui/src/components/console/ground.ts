// A server's colour on the console's grounds (CONSOLE-ANY, the owner: the server's colours
// reflected directly). A colour that reads at 4.5:1 on its ground is drawn exactly as sent; one
// that does not keeps its hue and saturation and takes the least lightness that reads. Tooltips
// keep their own rule (`readableColor`, inventory/text.ts).

/** Where a console text sits: the console's own ground, or a plate on it (a chosen row, a button). */
export type ConsoleGround = "console" | "plate";

/**
 * Each ground at its lightest, so a colour that reads there reads wherever the ground is drawn:
 * the console at most the launcher's glass sheet (obsidian-900 at 88 %) over a mid-grey stretch of
 * the art (the site's and the solid panel's obsidian-900 are darker), a plate at most obsidian-700
 * (the chosen row's white veil, a button).
 */
export const GROUND_RGB: Record<ConsoleGround, readonly [number, number, number]> = {
  console: [0x1f, 0x1d, 0x23],
  plate: [0x27, 0x22, 0x2c],
};

/** WCAG's floor for text (design.md §8). */
const FLOOR = 4.5;

function channel(v: number): number {
  const c = v / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** WCAG relative luminance of an sRGB colour (0–255 channels). */
export function luminanceOf(rgb: readonly number[]): number {
  const [r = 0, g = 0, b = 0] = rgb;
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** The contrast of a colour on a ground, as WCAG measures it. */
export function contrastOn(rgb: readonly number[], ground: ConsoleGround): number {
  const a = luminanceOf(rgb);
  const b = luminanceOf(GROUND_RGB[ground]);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** `#rrggbb` as 0–255 channels. */
export function rgbOf(hex: string): [number, number, number] {
  const value = Number.parseInt(hex.slice(1, 7), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}

/** 0–255 channels as hue (degrees), saturation and lightness (0–1). */
export function hslOf(rgb: readonly number[]): [number, number, number] {
  const [r = 0, g = 0, b = 0] = rgb.map((v) => v / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  let h: number;
  if (max === r) h = ((g - b) / d) % 6;
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return [(h * 60 + 360) % 360, s, l];
}

function rgbOfHsl(h: number, s: number, l: number): [number, number, number] {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  const [r, g, b] =
    h < 60
      ? [c, x, 0]
      : h < 120
        ? [x, c, 0]
        : h < 180
          ? [0, c, x]
          : h < 240
            ? [0, x, c]
            : h < 300
              ? [x, 0, c]
              : [c, 0, x];
  return [r + m, g + m, b + m].map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255)) as [
    number,
    number,
    number,
  ];
}

const hexOf = (rgb: readonly number[]) =>
  `#${rgb.map((c) => c.toString(16).padStart(2, "0")).join("")}`;

const seen = new Map<string, string>();

/**
 * A server's colour (`#rrggbb`) on `ground`: the very same string when it reads at 4.5:1 there,
 * else its hue and saturation at the least lightness (in steps of 0.1 %) that reads.
 */
export function groundColor(hex: string, ground: ConsoleGround = "console"): string {
  const key = `${ground}${hex}`;
  const hit = seen.get(key);
  if (hit) return hit;
  const rgb = rgbOf(hex);
  let out = hex;
  if (contrastOn(rgb, ground) < FLOOR) {
    const [h, s, l] = hslOf(rgb);
    out = "#ffffff";
    for (let step = Math.ceil(l * 1000); step <= 1000; step++) {
      const next = rgbOfHsl(h, s, step / 1000);
      if (contrastOn(next, ground) >= FLOOR) {
        out = hexOf(next);
        break;
      }
    }
  }
  if (seen.size > 512) seen.clear();
  seen.set(key, out);
  return out;
}
