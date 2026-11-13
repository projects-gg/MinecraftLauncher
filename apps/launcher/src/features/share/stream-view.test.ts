// The stream viewer's rules (SQOL-U1): the layer a tile asks for, how long a weak connection holds
// the small layer, zoom and pan, the stream menu's order and the keys of a focused tile.
import { describe, expect, it } from "vitest";
import {
  clampZoom,
  layerFor,
  NO_WEAK_HOLD,
  NO_ZOOM,
  panBy,
  SMALL_TILE_PX,
  streamRows,
  tileKey,
  WEAK_AGAIN_MS,
  WEAK_BACK_MAX_MS,
  WEAK_BACK_MS,
  weakBackAt,
  weakStep,
  wheelFactor,
  ZOOM_MAX,
  zoomAt,
  zoomPercent,
  zoomTransform,
} from "./stream-view";

const auto = {
  quality: "auto" as const,
  width: 900,
  weakHeld: false,
  zoomed: false,
  hasSmall: true,
};

describe("the layer a tile asks for", () => {
  it("follows the tile's width under Otomatik", () => {
    expect(layerFor(auto)).toBe("f");
    expect(layerFor({ ...auto, width: SMALL_TILE_PX - 1 })).toBe("q");
    expect(layerFor({ ...auto, width: SMALL_TILE_PX })).toBe("f");
    // Not measured yet: the full one (the launcher's own start).
    expect(layerFor({ ...auto, width: null })).toBe("f");
  });

  it("takes the small one while a weak connection holds it, the full one to read zoomed text", () => {
    expect(layerFor({ ...auto, weakHeld: true })).toBe("q");
    expect(layerFor({ ...auto, width: 300, zoomed: true })).toBe("f");
    // A weak connection cannot carry the full one, zoomed or not.
    expect(layerFor({ ...auto, weakHeld: true, zoomed: true })).toBe("q");
  });

  it("keeps Yüksek and Veri Tasarrufu whatever the size", () => {
    expect(layerFor({ ...auto, quality: "high", width: 200, weakHeld: true })).toBe("f");
    expect(layerFor({ ...auto, quality: "saver", width: 2000, zoomed: true })).toBe("q");
    // A share without a small layer has only the full one.
    expect(layerFor({ ...auto, quality: "saver", hasSmall: false })).toBe("f");
  });
});

describe("a weak connection's hold", () => {
  it("holds the small layer until the connection has been good long enough", () => {
    let hold = weakStep(NO_WEAK_HOLD, true, 0);
    expect(hold.held).toBe(true);
    hold = weakStep(hold, false, 1_000);
    expect(hold).toMatchObject({ held: true, goodSince: 1_000 });
    expect(weakBackAt(hold)).toBe(1_000 + WEAK_BACK_MS);
    // Weak again meanwhile: the count starts over.
    hold = weakStep(hold, true, 5_000);
    expect(hold.goodSince).toBeNull();
    expect(weakBackAt(hold)).toBeNull();
    hold = weakStep(hold, false, 6_000);
    hold = weakStep(hold, false, 6_000 + WEAK_BACK_MS);
    expect(hold.held).toBe(false);
    expect(hold.backAt).toBe(6_000 + WEAK_BACK_MS);
  });

  it("waits longer each time it falls back soon after, up to two minutes", () => {
    let hold = weakStep(NO_WEAK_HOLD, true, 0);
    let now = 0;
    const waits: number[] = [];
    for (let i = 0; i < 4; i++) {
      now += 1_000;
      hold = weakStep(hold, false, now);
      now += hold.wait;
      hold = weakStep(hold, false, now);
      expect(hold.held).toBe(false);
      // Weak again ten seconds after the full layer came back.
      now += 10_000;
      hold = weakStep(hold, true, now);
      waits.push(hold.wait);
    }
    expect(waits).toEqual([WEAK_BACK_MS * 2, WEAK_BACK_MS * 4, WEAK_BACK_MAX_MS, WEAK_BACK_MAX_MS]);
    // A long good spell forgets it.
    hold = weakStep(hold, false, now + 1);
    now += 1 + hold.wait;
    hold = weakStep(hold, false, now);
    hold = weakStep(hold, true, now + 5 * 60_000);
    expect(hold.wait).toBe(WEAK_BACK_MS);
  });

  it("starts from the first wait when weak again only after a good spell (E2E: 24 s after)", () => {
    let hold = weakStep(NO_WEAK_HOLD, true, 0);
    hold = weakStep(hold, false, 1_000);
    hold = weakStep(hold, false, 1_000 + WEAK_BACK_MS);
    expect(hold.held).toBe(false);
    const back = 1_000 + WEAK_BACK_MS;
    hold = weakStep(hold, true, back + 24_000);
    expect(hold.wait).toBe(WEAK_BACK_MS);
    // Within the first seconds after the full layer came back it still doubles.
    hold = weakStep(hold, false, back + 25_000);
    hold = weakStep(hold, false, back + 25_000 + WEAK_BACK_MS);
    hold = weakStep(hold, true, back + 25_000 + WEAK_BACK_MS + 5_000);
    expect(hold.wait).toBe(WEAK_BACK_MS * 2);
    expect(WEAK_AGAIN_MS).toBeLessThan(24_000);
  });

  it("says nothing new while the connection stays as it was", () => {
    expect(weakStep(NO_WEAK_HOLD, false, 10)).toBe(NO_WEAK_HOLD);
    const held = weakStep(NO_WEAK_HOLD, true, 0);
    expect(weakStep(held, true, 2_000)).toBe(held);
  });
});

describe("zoom and pan", () => {
  const w = 800;
  const h = 450;

  it("keeps the point under the pointer where it is", () => {
    const zoomed = zoomAt(NO_ZOOM, 2, 200, 100, w, h);
    expect(zoomed.s).toBe(2);
    // The picture's point (200, 100) is still under (200, 100).
    expect((200 - zoomed.x) / zoomed.s).toBeCloseTo(200);
    expect((100 - zoomed.y) / zoomed.s).toBeCloseTo(100);
    const again = zoomAt(zoomed, 1.5, 600, 300, w, h);
    expect((600 - again.x) / again.s).toBeCloseTo((600 - zoomed.x) / zoomed.s);
  });

  it("stays between %100 and %400 and never shows an empty edge", () => {
    expect(zoomAt(NO_ZOOM, 0.5, 10, 10, w, h)).toEqual(NO_ZOOM);
    expect(zoomAt(NO_ZOOM, 10, 400, 225, w, h).s).toBe(ZOOM_MAX);
    // In a corner: the picture's own corner stays at the tile's.
    const corner = zoomAt(NO_ZOOM, 3, 0, 0, w, h);
    expect(corner).toMatchObject({ x: 0, y: 0 });
    const far = zoomAt(NO_ZOOM, 3, w, h, w, h);
    expect(far.x).toBeCloseTo(w * (1 - 3));
    expect(far.y).toBeCloseTo(h * (1 - 3));
    // Back to %100: the whole picture, nothing moved.
    expect(clampZoom({ s: 1.01, x: -5, y: -5 }, w, h)).toEqual(NO_ZOOM);
  });

  it("pans a zoomed picture inside its tile, and an unzoomed one not at all", () => {
    const zoomed = zoomAt(NO_ZOOM, 2, 400, 225, w, h);
    const moved = panBy(zoomed, 1000, -1000, w, h);
    expect(moved.x).toBe(0);
    expect(moved.y).toBeCloseTo(h * (1 - 2));
    expect(panBy(NO_ZOOM, 50, 50, w, h)).toBe(NO_ZOOM);
  });

  it("zooms about a fifth a wheel notch, in and out alike", () => {
    expect(wheelFactor(-100)).toBeGreaterThan(1.2);
    expect(wheelFactor(-100)).toBeLessThan(1.25);
    expect(wheelFactor(100) * wheelFactor(-100)).toBeCloseTo(1);
    // Lines and pages count as pixels; a wild step stays bounded.
    expect(wheelFactor(-3, 1)).toBeCloseTo(wheelFactor(-48));
    expect(wheelFactor(-10, 2)).toBe(1.5);
    expect(zoomPercent(zoomAt(NO_ZOOM, 2.5, 0, 0, w, h))).toBe(250);
  });

  it("gives the picture the zoom the chip says (none at %100)", () => {
    expect(zoomTransform(NO_ZOOM)).toBe("");
    const zoom = zoomAt(NO_ZOOM, 2, w, h, w, h);
    expect(zoomTransform(zoom)).toBe(`translate3d(${-w}px, ${-h}px, 0) scale(2)`);
    expect(zoomPercent(zoom)).toBe(200);
  });
});

describe("the stream menu", () => {
  it("has the contract's rows in the contract's order in the launcher's window", () => {
    expect(streamRows({ popout: false, two: false, forward: false })).toEqual([
      "volume",
      "mute",
      "quality",
      "duck",
      null,
      "fullscreen",
      "popout",
      "info",
      null,
      "report",
      "stop",
    ]);
  });

  it("brings one of two forward, or puts them side by side again", () => {
    const rows = streamRows({ popout: false, two: true, forward: false });
    expect(rows.slice(5, 9)).toEqual(["fullscreen", "popout", "spotlight", "info"]);
    expect(streamRows({ popout: false, two: true, forward: true })).toContain("sideBySide");
  });

  it("has the window's own rows and no report in the Yayın window", () => {
    const rows = streamRows({ popout: true, two: false, forward: false });
    expect(rows).toEqual([
      "volume",
      "mute",
      "quality",
      "duck",
      null,
      "fullscreen",
      "popin",
      "onTop",
      "info",
      null,
      "stop",
    ]);
  });
});

describe("keys on a focused tile", () => {
  const key = (
    k: string,
    extra: Partial<{ code: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean }> = {},
  ) => ({
    key: k,
    code: extra.code ?? "",
    ctrlKey: extra.ctrlKey ?? false,
    metaKey: false,
    altKey: extra.altKey ?? false,
    shiftKey: extra.shiftKey ?? false,
  });
  const onTile = { onTile: true, zoomed: false, sound: true };

  it("has F, M and the arrows for the sound, only on the tile itself for the arrows", () => {
    expect(tileKey(key("f"), onTile)).toEqual({ kind: "fullscreen" });
    expect(tileKey(key("F"), onTile)).toEqual({ kind: "fullscreen" });
    expect(tileKey(key("m"), onTile)).toEqual({ kind: "mute" });
    expect(tileKey(key("ArrowUp"), onTile)).toEqual({ kind: "volume", by: 5 });
    expect(tileKey(key("ArrowDown"), onTile)).toEqual({ kind: "volume", by: -5 });
    expect(tileKey(key("ArrowUp"), { ...onTile, onTile: false })).toBeNull();
    // The player's own preview has no sound.
    expect(tileKey(key("m"), { ...onTile, sound: false })).toBeNull();
  });

  it("zooms with Ctrl and one more key, never with AltGr", () => {
    expect(tileKey(key("=", { ctrlKey: true }), onTile)).toEqual({ kind: "zoom", by: "in" });
    // Turkish Q types "=" with Shift + 0.
    expect(tileKey(key("=", { ctrlKey: true, shiftKey: true }), onTile)).toEqual({
      kind: "zoom",
      by: "in",
    });
    expect(tileKey(key("+", { ctrlKey: true, code: "NumpadAdd" }), onTile)).toEqual({
      kind: "zoom",
      by: "in",
    });
    expect(tileKey(key("-", { ctrlKey: true }), onTile)).toEqual({ kind: "zoom", by: "out" });
    expect(tileKey(key("0", { ctrlKey: true }), onTile)).toEqual({ kind: "zoom", by: "reset" });
    expect(tileKey(key("=", { ctrlKey: true, altKey: true }), onTile)).toBeNull();
    expect(tileKey(key("f", { ctrlKey: true }), onTile)).toBeNull();
  });

  it("pans with Shift and the arrows only while zoomed", () => {
    expect(tileKey(key("ArrowLeft", { shiftKey: true }), onTile)).toBeNull();
    const zoomed = { ...onTile, zoomed: true };
    expect(tileKey(key("ArrowLeft", { shiftKey: true }), zoomed)).toEqual({
      kind: "pan",
      dx: 1,
      dy: 0,
    });
    expect(tileKey(key("ArrowDown", { shiftKey: true }), zoomed)).toEqual({
      kind: "pan",
      dx: 0,
      dy: -1,
    });
  });
});
