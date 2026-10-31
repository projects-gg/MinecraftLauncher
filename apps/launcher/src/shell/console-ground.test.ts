// CONSOLE-ANY: a server's colour on the console's grounds. The owner wants the server's colours
// reflected directly: a colour that reads at 4.5:1 is drawn as sent; one that does not keeps its
// hue and saturation and takes the least lightness that reads (the console, and a plate on it).
import { contrastOn, GROUND_RGB, groundColor, hslOf, lineRuns, rgbOf } from "@pl/ui/console";
import { describe, expect, it } from "vitest";

/** The game's 16 text colours, §0–§f. */
const LEGACY = [
  "#000000",
  "#0000aa",
  "#00aa00",
  "#00aaaa",
  "#aa0000",
  "#aa00aa",
  "#ffaa00",
  "#aaaaaa",
  "#555555",
  "#5555ff",
  "#55ff55",
  "#55ffff",
  "#ff5555",
  "#ff55ff",
  "#ffff55",
  "#ffffff",
];

describe("a server's colour on the console's grounds", () => {
  it("draws each of the game's colours as the console and a plate pin them", () => {
    expect(LEGACY.map((hex) => groundColor(hex, "console"))).toEqual([
      "#7c7c7c",
      "#6868ff",
      "#00aa00",
      "#00aaaa",
      "#f90000",
      "#db00db",
      "#ffaa00",
      "#aaaaaa",
      "#7c7c7c",
      "#6868ff",
      "#55ff55",
      "#55ffff",
      "#ff5555",
      "#ff55ff",
      "#ffff55",
      "#ffffff",
    ]);
    expect(LEGACY.map((hex) => groundColor(hex, "plate"))).toEqual([
      "#8a8a8a",
      "#7b7bff",
      "#00aa00",
      "#00aaaa",
      "#ff4141",
      "#f300f3",
      "#ffaa00",
      "#aaaaaa",
      "#8a8a8a",
      "#7b7bff",
      "#55ff55",
      "#55ffff",
      "#ff5555",
      "#ff55ff",
      "#ffff55",
      "#ffffff",
    ]);
  });

  it("keeps a colour that reads exactly as sent, and lifts the rest in their own hue to 4.5:1", () => {
    for (const ground of ["console", "plate"] as const) {
      for (const hex of [...LEGACY, "#120000", "#2a0a4a", "#3355aa", "#808000"]) {
        const out = groundColor(hex, ground);
        expect(out).toMatch(/^#[0-9a-f]{6}$/);
        expect(contrastOn(rgbOf(out), ground)).toBeGreaterThanOrEqual(4.5);
        if (contrastOn(rgbOf(hex), ground) >= 4.5) expect(out).toBe(hex);
        const [h0, s0, l0] = hslOf(rgbOf(hex));
        const [h1, s1, l1] = hslOf(rgbOf(out));
        expect(l1).toBeGreaterThanOrEqual(l0);
        if (s0 > 0) {
          expect(Math.abs(h1 - h0)).toBeLessThan(2);
          expect(Math.abs(s1 - s0)).toBeLessThan(0.03);
        }
      }
    }
  });

  it("draws a line on the console and a chosen row or a button on its plate", () => {
    expect(lineRuns("Mavi", [[4, "9"]])?.[0]?.color).toBe("#6868ff");
    expect(lineRuns("Mavi", [[4, "9"]], [], "plate")?.[0]?.color).toBe("#7b7bff");
    expect(lineRuns("Yeşil", [[5, "a"]], [], "plate")?.[0]?.color).toBe("#55ff55");
  });

  it("measures each ground at its lightest", () => {
    expect(GROUND_RGB.console).toEqual([0x12, 0x10, 0x16]);
    expect(GROUND_RGB.plate).toEqual([0x27, 0x22, 0x2c]);
  });
});
