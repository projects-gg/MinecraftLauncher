import { describe, expect, it } from "vitest";
import { base64Bytes, FRAME_BYTES, jpegOf } from "./frame";

/** A canvas whose JPEG weighs `bytes(width, quality)`. */
function fakeCanvas(bytes: (width: number, quality: number) => number) {
  const tried: Array<[number, number]> = [];
  const canvas = {
    width: 0,
    height: 0,
    getContext: () => ({ drawImage: () => {} }),
    toDataURL: (_type: string, quality: number) => {
      tried.push([canvas.width, quality]);
      const size = bytes(canvas.width, quality);
      return `data:image/jpeg;base64,/9j/${"A".repeat(Math.ceil((size * 4) / 3) - 4)}`;
    },
  };
  return { canvas: canvas as unknown as HTMLCanvasElement, tried };
}

const source = {} as CanvasImageSource;

describe("a share report's still frame", () => {
  it("counts a base64 text's bytes", () => {
    expect(base64Bytes(btoa("abc"))).toBe(3);
    expect(base64Bytes(btoa("abcd"))).toBe(4);
    expect(base64Bytes(btoa("abcde"))).toBe(5);
  });

  it("steps the quality down until it fits", () => {
    const { canvas, tried } = fakeCanvas((_w, q) => (q > 0.6 ? 90_000 : 40_000));
    const frame = jpegOf(source, 1920, 1080, FRAME_BYTES, canvas);
    expect(frame).not.toBeNull();
    expect(base64Bytes(frame ?? "")).toBeLessThanOrEqual(FRAME_BYTES);
    expect(tried.map(([, q]) => q)).toEqual([0.82, 0.7, 0.58]);
    expect(canvas.width).toBe(1280);
    expect(canvas.height).toBe(720);
  });

  it("then the size, and gives up when nothing fits", () => {
    const { canvas, tried } = fakeCanvas((w) => (w > 640 ? 200_000 : 30_000));
    expect(jpegOf(source, 1920, 1080, FRAME_BYTES, canvas)).not.toBeNull();
    expect(tried.at(-1)).toEqual([640, 0.82]);
    const none = fakeCanvas(() => 200_000);
    expect(jpegOf(source, 1920, 1080, FRAME_BYTES, none.canvas)).toBeNull();
    expect(jpegOf(source, 0, 0, FRAME_BYTES, none.canvas)).toBeNull();
  });
});
