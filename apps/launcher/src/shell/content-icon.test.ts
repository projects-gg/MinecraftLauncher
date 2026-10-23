import { CONTENT_SPRITES, type ContentIconKind, spritePaths, spriteSize } from "@pl/ui";
import { describe, expect, it } from "vitest";

const TONES = new Set([..."hlmdsokFfeJjq"]);
const kinds = Object.keys(CONTENT_SPRITES) as ContentIconKind[];

describe("content sprites", () => {
  it.each(kinds)("%s is 16 x 16 pixels of known tones", (kind) => {
    const { rows } = CONTENT_SPRITES[kind];
    expect(rows).toHaveLength(16);
    for (const row of rows) {
      expect(row).toHaveLength(16);
      for (const pixel of row) expect(pixel === "." || TONES.has(pixel)).toBe(true);
    }
    expect(rows.join("").replaceAll(".", "").length).toBeGreaterThan(20);
  });

  it.each(kinds)("%s keeps its shadow inside the 16 x 16 grid", (kind) => {
    const sprite: { rows: readonly string[]; shadow?: string } = CONTENT_SPRITES[kind];
    if (!sprite.shadow) return;
    expect(sprite.rows[15]).toBe(".".repeat(16));
    for (const row of sprite.rows) expect(row[15]).toBe(".");
  });

  it("draws every pixel once, one path per tone", () => {
    const { layers, shadow } = spritePaths("server");
    const tones = layers.map((l) => l.tone);
    expect(new Set(tones).size).toBe(tones.length);
    const area = (d: string) =>
      [...d.matchAll(/h(\d+)v1/g)].reduce((sum, m) => sum + Number(m[1]), 0);
    const pixels = CONTENT_SPRITES.server.rows.join("").replaceAll(".", "").length;
    expect(layers.reduce((sum, l) => sum + area(l.d), 0)).toBe(pixels);
    expect(shadow && area(shadow.d)).toBe(pixels);
    expect(spritePaths("server")).toBe(spritePaths("server"));
  });

  it("picks whole-pixel sizes for a tile", () => {
    expect(spriteSize(22)).toBe(16);
    expect(spriteSize(36)).toBe(16);
    expect(spriteSize(42)).toBe(32);
    expect(spriteSize(52)).toBe(32);
    expect(spriteSize(72)).toBe(48);
  });
});
