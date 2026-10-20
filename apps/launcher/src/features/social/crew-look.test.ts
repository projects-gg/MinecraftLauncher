import { describe, expect, it } from "vitest";
import {
  COLORS,
  CREW_COLORS,
  crewColor,
  crewFrame,
  EMBLEMS,
  edgeWidth,
  emblemsFor,
  FIRST_EMBLEMS,
  FRAMES,
  shuffleLook,
  tagTint,
} from "./crew-look";

/** WCAG contrast of two `#RRGGBB` colours. */
function contrast(a: string, b: string): number {
  const luminance = (hex: string) => {
    const value = Number.parseInt(hex.slice(1), 16);
    const channel = (c: number) => {
      const v = c / 255;
      return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
    };
    return (
      0.2126 * channel((value >> 16) & 255) +
      0.7152 * channel((value >> 8) & 255) +
      0.0722 * channel(value & 255)
    );
  };
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

describe("crew looks", () => {
  it("has the service's 12 colours, 4 frames and 36 emblems", () => {
    expect(COLORS).toEqual([
      "gold",
      "copper",
      "crimson",
      "rose",
      "purple",
      "navy",
      "blue",
      "teal",
      "green",
      "silver",
      "ivory",
      "obsidian",
    ]);
    expect(FRAMES).toEqual(["cut", "shield", "banner", "hex"]);
    expect(EMBLEMS).toHaveLength(36);
    expect(new Set(EMBLEMS).size).toBe(36);
    expect(FIRST_EMBLEMS).toHaveLength(20);
    expect(FIRST_EMBLEMS.every((emblem) => EMBLEMS.includes(emblem))).toBe(true);
  });

  it("offers the first 20 emblems to a service without looks, in the picker's order", () => {
    expect(emblemsFor(true)).toBe(EMBLEMS);
    const old = emblemsFor(false);
    expect(old).toHaveLength(20);
    expect(old[0]).toBe("sword");
    expect(old).not.toContain("controller");
  });

  it("keeps the emblem readable on every colour and the edge visible on the panel", () => {
    const panel = "#16131A";
    for (const color of COLORS) {
      const { fill, edge, glyph } = CREW_COLORS[color];
      expect(contrast(glyph, fill), `${color} emblem`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(edge, panel), `${color} edge`).toBeGreaterThanOrEqual(3);
    }
  });

  it("keeps the tag's bone text readable on every colour's tint", () => {
    const panel = [0x16, 0x13, 0x1a];
    for (const color of COLORS) {
      const edge = Number.parseInt(CREW_COLORS[color].edge.slice(1), 16);
      const tint = [(edge >> 16) & 255, (edge >> 8) & 255, edge & 255].map((c, i) =>
        Math.round(c * 0.24 + (panel[i] ?? 0) * 0.76),
      );
      const hex = `#${tint.map((c) => c.toString(16).padStart(2, "0")).join("")}`;
      expect(contrast("#fbf6ec", hex), `${color} tag`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("falls back to today's look for what it does not know", () => {
    expect(crewColor("chartreuse")).toBe("gold");
    expect(crewColor(undefined)).toBe("gold");
    expect(crewFrame("star")).toBe("cut");
    expect(crewFrame("banner")).toBe("banner");
    expect(tagTint("teal")).toBe("rgb(70 189 180 / 0.24)");
  });

  it("thins the frame's edge as the mark grows", () => {
    expect(edgeWidth(22)).toBe(2.2);
    expect(edgeWidth(64)).toBe(1.4);
    expect(edgeWidth(10)).toBe(2.2);
    expect(edgeWidth(43)).toBeCloseTo(1.8, 1);
  });

  it("shuffles all three at once, never keeping one as it was", () => {
    const current = { color: "gold" as const, frame: "cut" as const, emblem: "sword" as const };
    for (const roll of [0, 0.3, 0.99]) {
      const next = shuffleLook(current, EMBLEMS, () => roll);
      expect(next.color).not.toBe("gold");
      expect(next.frame).not.toBe("cut");
      expect(next.emblem).not.toBe("sword");
    }
  });
});
