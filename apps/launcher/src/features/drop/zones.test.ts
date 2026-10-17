import { describe, expect, it } from "vitest";
import type { DroppedFile, DroppedKind } from "@/lib/api";
import { cssPoint } from "@/lib/api/drag-point";
import {
  activeZones,
  contains,
  dropTarget,
  hasModpack,
  isSkinFile,
  probePoints,
  WINDOW_ZONE,
  windowBox,
  type ZoneInput,
  zoneAt,
  zoneDimmed,
  zoneState,
} from "./zones";

const f = (name: string, kind: DroppedKind): DroppedFile => ({ path: `C:/x/${name}`, name, kind });
const box = (left: number, top: number, right: number, bottom: number) => ({
  left,
  top,
  right,
  bottom,
});

describe("cursor position", () => {
  it("turns Tauri's physical pixels into CSS pixels", () => {
    expect(cssPoint({ x: 300, y: 150 }, 1.5)).toEqual({ x: 200, y: 100 });
    expect(cssPoint({ x: 250, y: 125 }, 1.25)).toEqual({ x: 200, y: 100 });
    expect(cssPoint({ x: 40, y: 30 }, 1)).toEqual({ x: 40, y: 30 });
    // A broken scale never divides by zero.
    expect(cssPoint({ x: 40, y: 30 }, 0)).toEqual({ x: 40, y: 30 });
  });
});

describe("hit-testing", () => {
  const window = { id: WINDOW_ZONE, box: windowBox(1200, 712), depth: 0 };
  const tab = { id: "mods-tab", box: box(100, 200, 900, 440), depth: 1 };

  it("counts the edges as inside", () => {
    expect(contains(box(0, 0, 10, 10), { x: 10, y: 0 })).toBe(true);
    expect(contains(box(0, 0, 10, 10), { x: 10.5, y: 5 })).toBe(false);
  });

  it("finds nothing without a cursor or outside every zone", () => {
    expect(zoneAt(null, [window])).toBeNull();
    // The window zone keeps a margin along the window's edges.
    expect(zoneAt({ x: 10, y: 300 }, [window])).toBeNull();
    expect(zoneAt({ x: 600, y: 300 }, [window])).toBe(WINDOW_ZONE);
  });

  it("prefers the zone in front, then the most specific one", () => {
    expect(zoneAt({ x: 500, y: 300 }, [window, tab])).toBe("mods-tab");
    expect(zoneAt({ x: 500, y: 600 }, [window, tab])).toBe(WINDOW_ZONE);
    const small = { id: "small", box: box(400, 250, 600, 350), depth: 1 };
    expect(zoneAt({ x: 500, y: 300 }, [tab, small])).toBe("small");
    const claim = { id: "skin", box: box(0, 0, 1200, 712), depth: 2 };
    expect(zoneAt({ x: 500, y: 300 }, [tab, small, claim])).toBe("skin");
  });

  it("sizes the window zone with a comfortable inset", () => {
    expect(windowBox(1200, 712)).toEqual(box(22, 22, 1178, 690));
    expect(windowBox(1024, 640, 16)).toEqual(box(16, 16, 1008, 624));
  });

  it("probes a zone inside itself and inside the window", () => {
    expect(probePoints(box(100, 100, 300, 200), 1200, 712)).toEqual([
      { x: 200, y: 150 },
      { x: 112, y: 112 },
      { x: 288, y: 112 },
      { x: 112, y: 188 },
      { x: 288, y: 188 },
    ]);
    // Scrolled half out of view: only the points still in the window are probed.
    expect(probePoints(box(100, 600, 300, 800), 1200, 712)).toEqual([
      { x: 200, y: 700 },
      { x: 112, y: 612 },
      { x: 288, y: 612 },
    ]);
    expect(probePoints(box(100, 800, 300, 900), 1200, 712)).toEqual([]);
  });
});

describe("which zones take part", () => {
  const page = { id: "mods-tab", layer: "page" as const, shown: true };
  const hidden = { id: "packs-tab", layer: "page" as const, shown: false };
  const skin = { id: "skin", layer: "claim" as const, shown: true };

  it("lets a place with its own handling take the drag alone", () => {
    expect(activeZones([page, skin], true)).toEqual(["skin"]);
    // Even while the window's own drops are off.
    expect(activeZones([skin], false)).toEqual(["skin"]);
  });

  it("uses the zones shown on the page, else the window zone", () => {
    expect(activeZones([page, hidden], true)).toEqual(["mods-tab"]);
    expect(activeZones([hidden], true)).toEqual([WINDOW_ZONE]);
    expect(activeZones([], true)).toEqual([WINDOW_ZONE]);
    expect(activeZones([page], false)).toEqual([]);
  });

  it("shows a drop going into the zone under the cursor, else the newest claim", () => {
    expect(dropTarget(["a", "b"], "b", null)).toBe("b");
    expect(dropTarget(["a", "b"], null, "b")).toBe("b");
    expect(dropTarget(["a", "b"], "elsewhere", null)).toBe("a");
    expect(dropTarget([], null, null)).toBeNull();
  });
});

describe("zone states", () => {
  const base: ZoneInput = {
    active: true,
    accepts: true,
    hovered: null,
    hoveredAccepts: true,
    absorbing: null,
  };

  it("rests without a drag and gets ready when files enter", () => {
    expect(zoneState("a", { ...base, active: false })).toBe("idle");
    expect(zoneState("a", base)).toBe("ready");
  });

  it("lights up only the zone under the cursor; the others recede", () => {
    expect(zoneState("a", { ...base, hovered: "a" })).toBe("hover");
    expect(zoneState("b", { ...base, hovered: "a" })).toBe("recede");
    // A zone that refuses the files does not push the others back.
    expect(zoneState("b", { ...base, hovered: "a", hoveredAccepts: false })).toBe("ready");
  });

  it("stays neutral for files it refuses, under the cursor too", () => {
    expect(zoneState("a", { ...base, accepts: false })).toBe("refuse");
    expect(zoneState("a", { ...base, accepts: false, hovered: "a" })).toBe("refuse");
  });

  it("dims a refusing zone while the cursor is over another one", () => {
    expect(zoneDimmed("b", "refuse", "a")).toBe(true);
    expect(zoneDimmed("b", "refuse", "b")).toBe(false);
    expect(zoneDimmed("b", "refuse", null)).toBe(false);
    expect(zoneDimmed("b", "recede", "a")).toBe(false);
  });

  it("absorbs the drop it took, after the drag ended", () => {
    expect(zoneState("a", { ...base, active: false, absorbing: "a" })).toBe("absorb");
    expect(zoneState("b", { ...base, active: false, absorbing: "a" })).toBe("idle");
  });
});

describe("what a zone takes", () => {
  it("takes PNG files as skins", () => {
    expect(isSkinFile("C:/Users/Oyuncu/Downloads/ejder.PNG")).toBe(true);
    expect(isSkinFile("C:/x/sodium.jar")).toBe(false);
  });

  it("looks for a modpack, welcoming files still being read", () => {
    expect(hasModpack(null)).toBe(true);
    expect(hasModpack([f("a.txt", "unknown"), f("FO.mrpack", "modpack")])).toBe(true);
    expect(hasModpack([f("sodium.jar", "mod")])).toBe(false);
    expect(hasModpack([])).toBe(false);
  });
});
