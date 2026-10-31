import { describe, expect, it } from "vitest";
import { firstRowUnder, floatingDayAt, LINE_SLACK } from "./floating-day";

describe("the floating day line", () => {
  // Rows 40 px tall from y = 100: row i spans 100 + 40 i to 140 + 40 i.
  const bottom = (i: number) => 140 + 40 * i;

  it("finds the first row under the edge in a handful of measures", () => {
    expect(firstRowUnder(0, bottom, 100)).toBe(-1);
    expect(firstRowUnder(10, bottom, 0)).toBe(0);
    expect(firstRowUnder(10, bottom, 139)).toBe(0);
    // A row whose bottom is the edge is gone over it.
    expect(firstRowUnder(10, bottom, 140)).toBe(1);
    expect(firstRowUnder(10, bottom, 230)).toBe(3);
    expect(firstRowUnder(10, bottom, 10_000)).toBe(-1);
    let measured = 0;
    const counted = (i: number) => {
      measured += 1;
      return bottom(i);
    };
    expect(firstRowUnder(1_000, counted, 20_000)).toBe(497);
    expect(measured).toBeLessThanOrEqual(11);
  });

  it("floats the day of the row under the edge only while reading back", () => {
    const row = { top: 80, at: 1_000, start: false };
    expect(floatingDayAt(row, 100, true)).toBe(1_000);
    // At the newest messages, or with no rows, nothing floats.
    expect(floatingDayAt(row, 100, false)).toBeNull();
    expect(floatingDayAt(null, 100, true)).toBeNull();
  });

  it("gives way to a day line that shows at the edge", () => {
    // The row starting a day carries its line at its top: on screen there, nothing floats.
    expect(floatingDayAt({ top: 120, at: 5, start: true }, 100, true)).toBeNull();
    expect(floatingDayAt({ top: 100 - LINE_SLACK, at: 5, start: true }, 100, true)).toBeNull();
    // Gone over the edge: its day floats.
    expect(floatingDayAt({ top: 100 - LINE_SLACK - 1, at: 5, start: true }, 100, true)).toBe(5);
  });
});
