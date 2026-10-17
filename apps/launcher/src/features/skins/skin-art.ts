// Texture helpers for the appearance panel: arm-model detection and pixel-sharp crops.
import type { CSSProperties } from "react";
import type { SkinVariant } from "@/lib/api";

export function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.decoding = "async";
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("image failed to load"));
    image.src = src;
  });
}

/** Columns slim (3 px) arms leave unused in a 64×64 skin, as [x, y, width, height]. */
const SLIM_GAPS: Array<[number, number, number, number]> = [
  [50, 16, 2, 4],
  [54, 20, 2, 12],
  [42, 48, 2, 4],
  [46, 52, 2, 12],
];

/**
 * Guesses the arm model of a skin texture (a data URL, so the canvas stays readable): slim skins
 * leave the fourth arm column transparent, or paint it solid black. 64×32 skins predate slim arms.
 */
export async function inferVariant(dataUrl: string): Promise<SkinVariant> {
  const image = await loadImage(dataUrl);
  const { naturalWidth: width, naturalHeight: height } = image;
  if (width === 0 || height < width) return "classic";
  const scale = width / 64;
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return "classic";
  ctx.drawImage(image, 0, 0);
  const areas = SLIM_GAPS.map(
    ([x, y, w, h]) => ctx.getImageData(x * scale, y * scale, w * scale, h * scale).data,
  );
  const transparent = areas.some((data) => {
    for (let i = 3; i < data.length; i += 4) if (data[i] !== 255) return true;
    return false;
  });
  const black = areas.every((data) => {
    for (let i = 0; i < data.length; i += 4) {
      if (data[i] || data[i + 1] || data[i + 2] || data[i + 3] !== 255) return false;
    }
    return true;
  });
  return transparent || black ? "slim" : "classic";
}

/**
 * Background that shows the outside of a cape (the 10×16 front at 1,1 of a 64×32 texture) in an
 * element with a 10:16 aspect ratio, pixel-sharp at any size.
 */
export function capeFront(url: string): CSSProperties {
  return {
    backgroundImage: `url("${url}")`,
    backgroundRepeat: "no-repeat",
    backgroundSize: "640% auto",
    backgroundPosition: `${(1 / 54) * 100}% ${(1 / 16) * 100}%`,
    imageRendering: "pixelated",
  };
}
