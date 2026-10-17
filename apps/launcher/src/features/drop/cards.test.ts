import { describe, expect, it } from "vitest";
import type { DroppedFile, DroppedKind } from "@/lib/api";
import { CARD_REACH, cardsJoin, cardTakes, cardTarget, guessKind } from "./cards";
import { hitBox, hoveredZone, type ZoneBox, zoneAt } from "./zones";

const f = (name: string, kind: DroppedKind): DroppedFile => ({ path: `C:/x/${name}`, name, kind });
const box = (left: number, top: number, right: number, bottom: number) => ({
  left,
  top,
  right,
  bottom,
});
const fabric = { id: "yaratici", modded: true };
const vanilla = { id: "vanilla", modded: false };

describe("what the cards take", () => {
  it("guesses from the name until the files are read", () => {
    expect(guessKind("C:/Users/Oyuncu/Downloads/sodium-fabric-0.6.13.JAR")).toBe("mod");
    expect(guessKind("C:/x/eski.litemod")).toBe("mod");
    expect(guessKind("C:/x/Stay-True-1.21.zip")).toBe("pack");
    expect(guessKind("C:/x/Fabulously Optimized 6.4.0.mrpack")).toBe("modpack");
    expect(guessKind("C:/x/notlar.txt")).toBe("unknown");
  });

  it("take part in drags with files an instance takes in", () => {
    expect(cardsJoin(["C:/x/sodium.jar"], null)).toBe(true);
    expect(cardsJoin(["C:/x/paket.zip"], null)).toBe(true);
    // A modpack alone opens the import flow: the window zone shows that, as on every page.
    expect(cardsJoin(["C:/x/FO.mrpack"], null)).toBe(false);
    expect(cardsJoin(["C:/x/notlar.txt"], null)).toBe(false);
    // Once read, the contents decide: a ".zip" can be a modpack too.
    expect(cardsJoin(["C:/x/paket.zip"], [f("paket.zip", "modpack")])).toBe(false);
    expect(cardsJoin(["C:/x/a", "C:/x/b"], [f("a", "unknown"), f("b", "shaderPack")])).toBe(true);
  });

  it("leave a vanilla instance's card neutral for mods", () => {
    const mods = [f("sodium.jar", "mod"), f("lithium.jar", "mod")];
    const paths = mods.map((m) => m.path);
    expect(cardTakes(paths, mods, fabric)).toBe(true);
    expect(cardTakes(paths, mods, vanilla)).toBe(false);
    // Already neutral while the files are read: the card never lights up and goes out again.
    expect(cardTakes(paths, null, vanilla)).toBe(false);
    expect(cardTakes(paths, null, fabric)).toBe(true);
  });

  it("let any instance take resource and shader packs", () => {
    const packs = [f("Stay-True.zip", "resourcePack"), f("BSL.zip", "shaderPack")];
    expect(cardTakes([], packs, vanilla)).toBe(true);
    expect(cardTakes(["C:/x/Stay-True.zip"], null, vanilla)).toBe(true);
    // Mods and a pack: a vanilla instance still takes the pack (the mods' notice follows).
    const mixed = [f("sodium.jar", "mod"), f("Stay-True.zip", "resourcePack")];
    expect(cardTakes([], mixed, vanilla)).toBe(true);
    // A modpack never goes into a card: it opens the import flow wherever it lands.
    expect(cardTakes([], [f("sodium.jar", "mod"), f("FO.mrpack", "modpack")], vanilla)).toBe(false);
  });
});

describe("where a drop on the cards goes", () => {
  it("goes into the card under the cursor, else into none", () => {
    expect(cardTarget(["a", "b"], "b")).toBe("b");
    expect(cardTarget(["a", "b"], null)).toBeNull();
    // Not a card of this drag (the window zone, a zone that left).
    expect(cardTarget(["a", "b"], "window")).toBeNull();
  });
});

describe("hit-testing the cards", () => {
  // Two cards side by side with the library's gap between them, and one below.
  const reach = CARD_REACH;
  const scroller = box(80, 110, 1180, 640);
  const cards: ZoneBox[] = [
    { id: "a", box: hitBox(box(86, 142, 295, 307), reach, scroller) ?? box(0, 0, 0, 0), depth: 1 },
    { id: "b", box: hitBox(box(306, 142, 515, 307), reach, scroller) ?? box(0, 0, 0, 0), depth: 1 },
    { id: "c", box: hitBox(box(86, 318, 295, 483), reach, scroller) ?? box(0, 0, 0, 0), depth: 1 },
  ];

  it("leaves no gap between neighbouring cards", () => {
    expect(zoneAt({ x: 298, y: 200 }, cards)).toBe("a");
    expect(zoneAt({ x: 303, y: 200 }, cards)).toBe("b");
    expect(zoneAt({ x: 150, y: 311 }, cards)).toBe("a");
    expect(zoneAt({ x: 150, y: 314 }, cards)).toBe("c");
    // Beside the grid there is no card.
    expect(zoneAt({ x: 700, y: 200 }, cards)).toBeNull();
  });

  it("cuts a card to the scroll area that shows it", () => {
    // Half scrolled under the header: only the part still in the list is under the cursor.
    expect(hitBox(box(86, 40, 295, 205), reach, scroller)).toEqual(box(80, 110, 301, 211));
    // Scrolled out entirely.
    expect(hitBox(box(86, -300, 295, -135), reach, scroller)).toBeNull();
    // Zones outside a scroll area only reach.
    expect(hitBox(box(10, 10, 20, 20), 2, null)).toEqual(box(8, 8, 22, 22));
  });

  it("keeps the card under the cursor on the line between two cards", () => {
    // Just past the middle of the gap the first card holds on for a few pixels…
    expect(hoveredZone({ x: 302, y: 200 }, cards, "a")).toBe("a");
    expect(hoveredZone({ x: 302, y: 200 }, cards, "b")).toBe("b");
    // …and lets go further in.
    expect(hoveredZone({ x: 320, y: 200 }, cards, "a")).toBe("b");
    // A lifted card (2 px up) does not lose the cursor at its bottom edge.
    const lifted = cards.map((c) =>
      c.id === "a" ? { ...c, box: { ...c.box, top: c.box.top - 2, bottom: c.box.bottom - 2 } } : c,
    );
    expect(hoveredZone({ x: 150, y: 312 }, lifted, "a")).toBe("a");
    // Leaving the grid lets go.
    expect(hoveredZone({ x: 700, y: 200 }, cards, "a")).toBeNull();
  });

  it("never holds a zone in front of a more specific one", () => {
    const window: ZoneBox = { id: "window", box: box(22, 22, 1178, 690), depth: 0 };
    const tab: ZoneBox = { id: "tab", box: box(100, 200, 900, 440), depth: 1 };
    // Into the empty tab's zone from the window zone: the tab takes over at once.
    expect(hoveredZone({ x: 101, y: 300 }, [window, tab], "window")).toBe("tab");
    // Out of it: the tab holds for a few pixels, then the window zone takes over.
    expect(hoveredZone({ x: 97, y: 300 }, [window, tab], "tab")).toBe("tab");
    expect(hoveredZone({ x: 90, y: 300 }, [window, tab], "tab")).toBe("window");
    expect(hoveredZone(null, [window, tab], "tab")).toBeNull();
  });
});
