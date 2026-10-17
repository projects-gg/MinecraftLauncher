import { describe, expect, it } from "vitest";
import {
  clampPage,
  hasPreviousSkins,
  insertAt,
  moveFocus,
  PAGE_SIZE,
  pageCount,
  pageItems,
  pageOf,
  reservedRows,
  steadyOrder,
} from "./history-model";

describe("pages", () => {
  it("count, clamp and slice by ten", () => {
    expect(PAGE_SIZE).toBe(10);
    expect(pageCount(0)).toBe(1);
    expect(pageCount(10)).toBe(1);
    expect(pageCount(11)).toBe(2);
    expect(pageCount(23)).toBe(3);
    expect(clampPage(5, 23)).toBe(2);
    expect(clampPage(-1, 23)).toBe(0);
    // The last page emptied: back one.
    expect(clampPage(2, 20)).toBe(1);
    expect(pageOf(9)).toBe(0);
    expect(pageOf(10)).toBe(1);
    const items = Array.from({ length: 23 }, (_, i) => i);
    expect(pageItems(items, 2)).toEqual([20, 21, 22]);
    expect(pageItems(items, 0)).toHaveLength(10);
  });

  it("reserve two rows once there are pages, so turning them never changes the height", () => {
    expect(reservedRows(1)).toBe(1);
    expect(reservedRows(5)).toBe(1);
    expect(reservedRows(6)).toBe(2);
    expect(reservedRows(10)).toBe(2);
    expect(reservedRows(11)).toBe(2);
    expect(reservedRows(0)).toBe(1);
  });
});

describe("moveFocus", () => {
  it("moves within a page and across its ends", () => {
    expect(moveFocus(0, "ArrowRight", 23)).toBe(1);
    expect(moveFocus(9, "ArrowRight", 23)).toBe(10);
    expect(moveFocus(10, "ArrowLeft", 23)).toBe(9);
    expect(moveFocus(0, "ArrowLeft", 23)).toBeNull();
    expect(moveFocus(22, "ArrowRight", 23)).toBeNull();
    expect(moveFocus(2, "ArrowDown", 23)).toBe(7);
    expect(moveFocus(7, "ArrowUp", 23)).toBe(2);
    // Rows do not spill into the next page.
    expect(moveFocus(7, "ArrowDown", 23)).toBeNull();
    // A short last page: nothing below.
    expect(moveFocus(21, "ArrowDown", 23)).toBeNull();
    expect(moveFocus(3, "Home", 23)).toBe(0);
    expect(moveFocus(13, "End", 23)).toBe(19);
    expect(moveFocus(21, "End", 23)).toBe(22);
    expect(moveFocus(0, "Home", 23)).toBeNull();
  });

  it("turns pages with Page Up and Page Down", () => {
    expect(moveFocus(3, "PageDown", 23)).toBe(13);
    expect(moveFocus(18, "PageDown", 23)).toBe(22);
    expect(moveFocus(22, "PageDown", 23)).toBeNull();
    expect(moveFocus(22, "PageUp", 23)).toBe(12);
    expect(moveFocus(4, "PageUp", 23)).toBeNull();
    expect(moveFocus(4, "Enter", 23)).toBeNull();
    expect(moveFocus(0, "ArrowRight", 0)).toBeNull();
  });
});

describe("steadyOrder", () => {
  it("keeps places, puts new skins first and drops gone ones", () => {
    expect(steadyOrder(null, ["a", "b", "c"])).toEqual(["a", "b", "c"]);
    // The server now sorts "c" first (just worn): it stays where it was.
    expect(steadyOrder(["a", "b", "c"], ["c", "a", "b"])).toEqual(["a", "b", "c"]);
    expect(steadyOrder(["a", "b", "c"], ["n", "c", "a"])).toEqual(["n", "a", "c"]);
  });

  it("puts an undone removal back at its place", () => {
    expect(insertAt(["a", "c"], "b", 1)).toEqual(["a", "b", "c"]);
    expect(insertAt(["a", "c"], "b", 9)).toEqual(["a", "c", "b"]);
    expect(insertAt(["a", "b", "c"], "b", 0)).toEqual(["b", "a", "c"]);
  });

  it("knows when there is something to go back to", () => {
    expect(hasPreviousSkins([], null)).toBe(false);
    expect(hasPreviousSkins(["a"], "a")).toBe(false);
    expect(hasPreviousSkins(["a"], null)).toBe(true);
    expect(hasPreviousSkins(["a", "b"], "a")).toBe(true);
  });
});
