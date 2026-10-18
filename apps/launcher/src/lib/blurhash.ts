// BlurHash placeholders for covers that are not on screen yet (https://blurha.sh). The decoder is
// the reference algorithm, small enough to keep here instead of adding a dependency; a decoded
// hash becomes a tiny PNG data URL (32 × 18, the browser smooths it when it is scaled up), made
// once per hash.

const DIGITS =
  "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

function decode83(text: string, start: number, end: number): number {
  let value = 0;
  for (let i = start; i < end; i++) {
    const digit = DIGITS.indexOf(text[i] ?? "");
    if (digit < 0) return Number.NaN;
    value = value * 83 + digit;
  }
  return value;
}

function toLinear(channel: number): number {
  const v = channel / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

function toSrgb(value: number): number {
  const v = Math.min(1, Math.max(0, value));
  return v <= 0.0031308
    ? Math.trunc(v * 12.92 * 255 + 0.5)
    : Math.trunc((1.055 * v ** (1 / 2.4) - 0.055) * 255 + 0.5);
}

const signedSquare = (value: number) => Math.sign(value) * value * value;

/**
 * RGBA pixels of `hash` at `width` × `height`, or `null` when it is not a valid BlurHash (wrong
 * length for its component count, or a character outside the base 83 alphabet).
 */
export function decodeBlurhash(
  hash: string,
  width: number,
  height: number,
): Uint8ClampedArray<ArrayBuffer> | null {
  if (hash.length < 6) return null;
  const sizeFlag = decode83(hash, 0, 1);
  const componentsY = Math.floor(sizeFlag / 9) + 1;
  const componentsX = (sizeFlag % 9) + 1;
  if (Number.isNaN(sizeFlag) || hash.length !== 4 + 2 * componentsX * componentsY) return null;
  const maximum = (decode83(hash, 1, 2) + 1) / 166;
  if (Number.isNaN(maximum)) return null;
  const colors: [number, number, number][] = [];
  for (let i = 0; i < componentsX * componentsY; i++) {
    if (i === 0) {
      const value = decode83(hash, 2, 6);
      if (Number.isNaN(value)) return null;
      colors.push([toLinear(value >> 16), toLinear((value >> 8) & 255), toLinear(value & 255)]);
    } else {
      const value = decode83(hash, 4 + i * 2, 6 + i * 2);
      if (Number.isNaN(value)) return null;
      colors.push([
        signedSquare((Math.floor(value / (19 * 19)) - 9) / 9) * maximum,
        signedSquare(((Math.floor(value / 19) % 19) - 9) / 9) * maximum,
        signedSquare(((value % 19) - 9) / 9) * maximum,
      ]);
    }
  }
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      let r = 0;
      let g = 0;
      let b = 0;
      for (let j = 0; j < componentsY; j++) {
        const basisY = Math.cos((Math.PI * y * j) / height);
        for (let i = 0; i < componentsX; i++) {
          const basis = Math.cos((Math.PI * x * i) / width) * basisY;
          const color = colors[i + j * componentsX] as [number, number, number];
          r += color[0] * basis;
          g += color[1] * basis;
          b += color[2] * basis;
        }
      }
      const at = 4 * (x + y * width);
      pixels[at] = toSrgb(r);
      pixels[at + 1] = toSrgb(g);
      pixels[at + 2] = toSrgb(b);
      pixels[at + 3] = 255;
    }
  }
  return pixels;
}

const PLACEHOLDER_WIDTH = 32;
const PLACEHOLDER_HEIGHT = 18;
const placeholders = new Map<string, string | null>();

/**
 * A data URL showing `hash` (16:9, scale it with `background-size: cover`), made once per hash;
 * `undefined` for an invalid hash or where no canvas is available.
 */
export function blurhashUrl(hash: string | undefined): string | undefined {
  if (!hash) return undefined;
  const known = placeholders.get(hash);
  if (known !== undefined) return known ?? undefined;
  let url: string | null = null;
  try {
    const pixels = decodeBlurhash(hash, PLACEHOLDER_WIDTH, PLACEHOLDER_HEIGHT);
    const canvas = pixels ? document.createElement("canvas") : null;
    const context = canvas?.getContext("2d");
    if (pixels && canvas && context) {
      canvas.width = PLACEHOLDER_WIDTH;
      canvas.height = PLACEHOLDER_HEIGHT;
      context.putImageData(new ImageData(pixels, PLACEHOLDER_WIDTH, PLACEHOLDER_HEIGHT), 0, 0);
      url = canvas.toDataURL("image/png");
    }
  } catch {
    url = null;
  }
  placeholders.set(hash, url);
  return url ?? undefined;
}
