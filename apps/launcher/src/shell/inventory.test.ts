// INV-UI: the cloud inventory's logic (packages/ui/src/components/inventory): the game's screen
// layouts, anyone's text, the game's click rules, what each input sends, and the moves' lifecycle.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MoveQueue } from "../../../../packages/ui/src/components/inventory/controller";
import {
  SAMPLE_INVENTORY,
  SAMPLE_KITS,
  withCursor,
} from "../../../../packages/ui/src/components/inventory/fixtures";
import { tooltipPlace } from "../../../../packages/ui/src/components/inventory/float";
import {
  neighbor,
  screenFor,
  slotKey,
} from "../../../../packages/ui/src/components/inventory/layout";
import { createMockMachine } from "../../../../packages/ui/src/components/inventory/mock-machine";
import {
  canDraw,
  clipText,
  textAdvance,
  textPath,
  wrapText,
} from "../../../../packages/ui/src/components/inventory/pixel-font";
import {
  type PlanState,
  planClick,
  planDrop,
  planHotbarSwap,
  planPutBack,
  planQuickMove,
} from "../../../../packages/ui/src/components/inventory/plan";
import * as rules from "../../../../packages/ui/src/components/inventory/rules";
import { safeInvItem } from "../../../../packages/ui/src/components/inventory/safe-item";
import {
  cleanText,
  durabilityColor,
  durabilityWidth,
  rarityOf,
  shadowOf,
  TEXT_COLOR,
  titleGlyphs,
  titleRuns,
  tooltipLines,
} from "../../../../packages/ui/src/components/inventory/text";
import { fill, INVENTORY_TEXTS } from "../../../../packages/ui/src/components/inventory/texts";
import { layoutTitle } from "../../../../packages/ui/src/components/inventory/title";
import type {
  InvItem,
  InvResult,
  TitleGlyph,
} from "../../../../packages/ui/src/components/inventory/types";
import { toInvItem } from "../features/afk/cloud/inventory/model";

const tr = INVENTORY_TEXTS.tr;

describe("inventory layouts (the game's container screens)", () => {
  it("draws a 9x3 chest like ChestMenu: slots, then the player's rows under the label", () => {
    const screen = screenFor({ type: "generic_9x3", title: "Sandık", size: 27 });
    expect(screen.height).toBe(168);
    const at = (k: string) => screen.slots.find((s) => s.key === k);
    expect(at("win:0")).toMatchObject({ x: 8, y: 18 });
    expect(at("win:26")).toMatchObject({ x: 152, y: 54 });
    expect(at("inv:9")).toMatchObject({ x: 8, y: 85 });
    expect(at("inv:36")).toMatchObject({ x: 8, y: 143 });
    expect(screen.inventoryLabel).toEqual({ x: 8, y: 74 });
    // No armour or off hand under a window (the contract refuses them there).
    expect(at("inv:5")).toBeUndefined();
    expect(at("inv:45")).toBeUndefined();
  });

  it("follows the hopper, dispenser, shulker and double chest geometry", () => {
    const hopper = screenFor({ type: "hopper", title: "", size: 5 });
    expect(hopper.height).toBe(133);
    expect(hopper.slots.find((s) => s.key === "win:4")).toMatchObject({ x: 116, y: 20 });
    expect(hopper.slots.find((s) => s.key === "inv:36")).toMatchObject({ y: 109 });
    const dispenser = screenFor({ type: "generic_3x3", title: "", size: 9 });
    expect(dispenser.title?.align).toBe("center");
    expect(dispenser.slots.find((s) => s.key === "win:8")).toMatchObject({ x: 98, y: 53 });
    const shulker = screenFor({ type: "shulker_box", title: "", size: 27 });
    expect(shulker.height).toBe(167);
    expect(shulker.slots.find((s) => s.key === "inv:9")).toMatchObject({ y: 84 });
    const big = screenFor({ type: "generic_9x6", title: "", size: 54 });
    expect(big.height).toBe(222);
    expect(big.slots.find((s) => s.key === "inv:9")).toMatchObject({ y: 139 });
  });

  it("draws unknown windows as rows of 9 and a lectern without the player's rows", () => {
    const anvil = screenFor({ type: "anvil", title: "", size: 3 });
    expect(anvil.slots.filter((s) => s.area === "win")).toHaveLength(3);
    expect(anvil.windowRows).toBe(1);
    const lectern = screenFor({ type: "lectern", title: "", size: 1 });
    expect(lectern.slots.every((s) => s.area === "win")).toBe(true);
  });

  it("gives the player's own screen the hopper's height: equipment, then the inventory", () => {
    const screen = screenFor(null);
    expect(screen.slots.map((s) => s.slot).sort((a, b) => a - b)).toEqual([
      5,
      6,
      7,
      8,
      ...Array.from({ length: 36 }, (_, i) => 9 + i),
      45,
    ]);
    expect(screen.sections.map((s) => s.id)).toEqual(["equipment", "main", "hotbar"]);
    const at = (k: string) => screen.slots.find((s) => s.key === k);
    // Head, chest, legs and feet from the grid's left edge, the off hand a group's gap after them.
    expect([5, 6, 7, 8, 45].map((slot) => at(slotKey("inv", slot)))).toMatchObject([
      { x: 8, y: 20, kind: "head" },
      { x: 26, y: 20, kind: "chest" },
      { x: 44, y: 20, kind: "legs" },
      { x: 62, y: 20, kind: "feet" },
      { x: 84, y: 20, kind: "offhand" },
    ]);
    expect(at("inv:9")).toMatchObject({ x: 8, y: 51 });
    expect(at("inv:36")).toMatchObject({ x: 8, y: 109 });
    expect(screen.height).toBe(133);
    expect(screen.title).toEqual({ x: 8, y: 6, align: "start" });
    expect(screen.inventoryLabel).toEqual({ x: 8, y: 39 });
    // Down from the equipment row lands on the inventory's nearest column.
    expect(neighbor(screen.slots, at("inv:5") as (typeof screen.slots)[0], "down").key).toBe(
      "inv:9",
    );
    expect(neighbor(screen.slots, at("inv:45") as (typeof screen.slots)[0], "down").key).toBe(
      "inv:13",
    );
  });

  it("moves the keyboard focus by rows and columns across the grids", () => {
    const screen = screenFor({ type: "generic_9x3", title: "", size: 27 });
    const at = (k: string) => screen.slots.find((s) => s.key === k) as (typeof screen.slots)[0];
    expect(neighbor(screen.slots, at("win:18"), "down").key).toBe("inv:9");
    expect(neighbor(screen.slots, at("inv:27"), "down").key).toBe("inv:36");
    expect(neighbor(screen.slots, at("inv:36"), "up").key).toBe("inv:27");
    expect(neighbor(screen.slots, at("win:0"), "left").key).toBe("win:0");
    expect(neighbor(screen.slots, at("win:0"), "end").key).toBe("win:8");
    expect(neighbor(screen.slots, at("win:5"), "last").key).toBe("inv:44");
  });
});

describe("menu titles with IA-ART's glyphs", () => {
  const glyphs = new Map<number, TitleGlyph>([
    [0xf808, { advance: -8 }],
    [0xe052, { advance: 257, image: { src: "market.png", width: 256, height: 256, ascent: 36 } }],
  ]);
  const glyph = (codePoint: number) => glyphs.get(codePoint);
  const title = `${String.fromCodePoint(0xf808, 0xe052, 0xe999)}Pazar`;

  it("draws a picture at the pen, moves by advances and leaves unknown glyphs out", () => {
    const layout = layoutTitle(titleRuns(title), 8, 6, glyph);
    // x 8 - 8; top 6 + 7 - 36; the unknown U+E999 neither draws nor moves.
    expect(layout.pictures).toEqual([{ x: 0, y: -23, width: 256, height: 256, src: "market.png" }]);
    expect(layout.texts).toEqual([{ x: 257, text: "Pazar" }]);
    expect(layout.advance).toBe(-8 + 257 + textAdvance("Pazar"));
  });

  it("leaves every glyph out without the item art document", () => {
    const layout = layoutTitle(titleRuns(title), 8, 6);
    expect(layout.pictures).toEqual([]);
    expect(layout.texts).toEqual([{ x: 8, text: "Pazar" }]);
  });

  it("measures words with the game font's advances", () => {
    expect(textAdvance("Sandık")).toBe(31);
    expect(textAdvance("Fırlatıcı")).toBe(37);
    expect(textAdvance("İl")).toBe(7);
  });
});

describe("the game's letters as pixels", () => {
  it("lights a glyph's pixels row by row from the letters' top", () => {
    // "1": the flag, the stem, the foot five wide on the baseline's row (6).
    const one = textPath("1");
    expect(one.startsWith("M2 0h1v1h-1zM1 1h2v1h-2zM2 2h1v1h-1z")).toBe(true);
    expect(one.endsWith("M0 6h5v1h-5z")).toBe(true);
    // The pen moves by each advance: the second "1" starts 6 further.
    expect(textPath("11")).toContain("M6 6h5v1h-5z");
    // Accents of capitals reach above the box, tails below it.
    expect(textPath("İ").startsWith("M1 -2h1v1h-1z")).toBe(true);
    expect(textPath("ş")).toContain("M2 7h1v1h-1zM1 8h1v1h-1z");
    expect(textPath(" ")).toBe("");
  });

  it("leans italic rows as the game's glyph quad does, the top right and the bottom left", () => {
    const one = textPath("1", true);
    // Row 0 moves 0.875 right, the baseline's row (6) 0.625 left; the upright path is untouched.
    expect(one.startsWith("M2.875 0h1v1h-1z")).toBe(true);
    expect(one.endsWith("M-0.625 6h5v1h-5z")).toBe(true);
    expect(textPath("1").startsWith("M2 0h1v1h-1z")).toBe(true);
  });

  it("knows which lines it can draw, and wraps and clips by advances", () => {
    expect(canDraw("Dayanıklılık: 1440 / 1561")).toBe(true);
    expect(canDraw("Ейдер")).toBe(false);
    expect(wrapText("aa aa", 12)).toEqual(["aa", "aa"]);
    expect(wrapText("aaaa", 12)).toEqual(["aa", "aa"]);
    expect(wrapText("Elmas zırh, kalkan", 200)).toEqual(["Elmas zırh, kalkan"]);
    // A plain title stops at the window's edge, on whole letters.
    expect(clipText("Sandık", 20)).toBe("San");
  });
});

describe("anyone's text", () => {
  it("drops controls, direction marks, formatting codes and private-use glyphs", () => {
    expect(cleanText("‮Admin⁦ §cKılıç\u0007")).toBe("Admin Kılıç");
    expect(cleanText("Market")).toBe("Market");
  });

  it("splits a title into glyph and text runs for menu pictures", () => {
    expect(titleRuns("Market")).toEqual([
      { glyph: 0xe001 },
      { glyph: 0xf801 },
      { text: "Market" },
    ]);
    expect(titleGlyphs("Sandık")).toEqual([]);
  });

  it("colours names by rarity and lines as the game does", () => {
    const sword = SAMPLE_INVENTORY.items.find((i) => i.slot === 36) as InvItem;
    expect(rarityOf(sword)).toBe("rare");
    const lines = tooltipLines(sword, "tr", tr);
    expect(lines[0]).toEqual({ text: "Elmas Kılıç", color: TEXT_COLOR.aqua });
    expect(lines.map((l) => l.text)).toContain("Keskinlik V");
    expect(lines.at(-1)?.text).toBe("Dayanıklılık: 1440 / 1561");
    const shulker = SAMPLE_INVENTORY.items.find((i) => i.slot === 17) as InvItem;
    const insideLines = tooltipLines(shulker, "tr", tr);
    const inside = insideLines.map((l) => l.text);
    // The game's Turkish puts spaces round the "x" ("%s x %s"; English "%s x%s").
    expect(inside).toContain("Elmas x 64");
    // Six stacks hold all 292: five listed, one more (the game counts stacks, not items), in the
    // game's white italic.
    expect(insideLines.at(-1)).toEqual({
      text: "ve 1 tane daha...",
      color: TEXT_COLOR.white,
      italic: true,
    });
    // Nine stacks listed and more items than they hold: more stacks than the frame tells.
    const full = {
      ...shulker,
      inside: {
        count: 1000,
        top: Array.from({ length: 9 }, () => ({
          id: "minecraft:dirt",
          count: 64,
          label: { tr: "Toprak", en: "Dirt" },
        })),
      },
    };
    expect(tooltipLines(full, "tr", tr).at(-1)?.text).toBe("ve daha fazlası...");
    expect(tooltipLines(full, "en", INVENTORY_TEXTS.en).at(-1)?.text).toBe("and more...");
    // A newer machine counts the stacks: the exact number, as the game says it.
    const counted = { ...full, inside: { ...full.inside, stacks: 16 } };
    expect(tooltipLines(counted, "tr", tr).at(-1)).toEqual({
      text: "ve 11 tane daha...",
      color: TEXT_COLOR.white,
      italic: true,
    });
    expect(tooltipLines(counted, "en", INVENTORY_TEXTS.en).at(-1)?.text).toBe("and 11 more...");
    // Five stacks or fewer: nothing more to say.
    const five = {
      ...shulker,
      inside: { count: 320, top: full.inside.top.slice(0, 5), stacks: 5 },
    };
    expect(tooltipLines(five, "tr", tr).at(-1)?.text).toBe("Toprak x 64");
    // A count below the stacks listed, or not a whole number, is not trusted: as before.
    for (const stacks of [3, 12.5, Number.NaN]) {
      const odd = { ...full, inside: { ...full.inside, stacks } };
      expect(tooltipLines(odd, "tr", tr).at(-1)?.text).toBe("ve daha fazlası...");
    }
    // The checked item (the site's live inventory, a console hover) keeps the count when it is a
    // whole number from the stacks listed up to 256, and drops that field alone otherwise.
    expect(safeInvItem(counted)?.inside).toEqual({ count: 1000, top: full.inside.top, stacks: 16 });
    for (const stacks of [3, 12.5, 257, "16", null]) {
      const odd = safeInvItem({ ...full, inside: { ...full.inside, stacks } });
      expect(odd?.inside).toEqual({ count: 1000, top: full.inside.top });
    }
    // The launcher's own frames (pl_cloud_afk reads the count up to 256): toInvItem keeps it when
    // it is at least the stacks listed, so the desktop says the same.
    const cloud = { ...full, inside: { ...full.inside, stacks: 16 } };
    expect(tooltipLines(toInvItem(cloud), "tr", tr).at(-1)?.text).toBe("ve 11 tane daha...");
    for (const stacks of [3, null, undefined]) {
      const odd = toInvItem({ ...cloud, inside: { ...cloud.inside, stacks } });
      expect(odd.inside).toEqual({ count: 1000, top: full.inside.top });
    }
    const custom = SAMPLE_INVENTORY.items.find((i) => i.slot === 24) as InvItem;
    expect(tooltipLines(custom, "en", INVENTORY_TEXTS.en)[0]?.text).toBe("Ejderha Kılıcı");
  });

  it("keeps every tooltip colour at 4.5:1 or more on the tooltip", () => {
    const lum = (hex: string) => {
      const c = [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16) / 255);
      const [r, g, b] = c.map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
      return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);
    };
    const background = lum("#1e0f1e"); // the 94 % tooltip over a white page, the worst case
    for (const color of [
      "white",
      "yellow",
      "aqua",
      "lightPurple",
      "gray",
      "red",
      "lore",
    ] as const) {
      const ratio = (lum(TEXT_COLOR[color]) + 0.05) / (background + 0.05);
      expect(ratio, color).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("draws the wear bar and shadows with the game's formulas", () => {
    expect(durabilityWidth(1561, 1561)).toBe(13);
    expect(durabilityWidth(1, 1561)).toBe(0);
    expect(durabilityColor(1561, 1561)).toBe("#00ff00");
    expect(durabilityColor(0, 100)).toBe("#ff0000");
    expect(shadowOf("#ffffff")).toBe("#3f3f3f");
    expect(fill(tr.durability, { left: 3, max: 9 })).toBe("Dayanıklılık: 3 / 9");
  });
});

const stack = (id: string, count: number, extra: Partial<InvItem> = {}): rules.Stack => ({
  id: `minecraft:${id}`,
  count,
  label: { tr: id, en: id },
  ...extra,
});

function state(
  slots: Record<number, rules.Stack>,
  cursor: rules.Stack | null = null,
): rules.InvState {
  return { held: 0, cursor, slots: new Map(Object.entries(slots).map(([k, v]) => [Number(k), v])) };
}

describe("the game's click rules (player's screen)", () => {
  it("takes all, takes half, places all and places one", () => {
    const s = state({ 9: stack("dirt", 33) });
    expect(rules.pickup(s, 9, 0).cursor?.count).toBe(33);
    const half = rules.pickup(s, 9, 1);
    expect(half.cursor?.count).toBe(17);
    expect(half.slots.get(9)?.count).toBe(16);
    expect(rules.pickup(state({}, stack("dirt", 5)), 10, 0).slots.get(10)?.count).toBe(5);
    const one = rules.pickup(state({}, stack("dirt", 5)), 10, 1);
    expect([one.slots.get(10)?.count, one.cursor?.count]).toEqual([1, 4]);
  });

  it("merges equal stacks up to 64 and swaps different ones", () => {
    const merged = rules.pickup(state({ 9: stack("dirt", 60) }, stack("dirt", 10)), 9, 0);
    expect([merged.slots.get(9)?.count, merged.cursor?.count]).toEqual([64, 6]);
    const swapped = rules.pickup(state({ 9: stack("dirt", 3) }, stack("stone", 2)), 9, 0);
    expect([swapped.slots.get(9)?.id, swapped.cursor?.id]).toEqual([
      "minecraft:stone",
      "minecraft:dirt",
    ]);
  });

  it("keeps armour slots to their armour and cursed armour on", () => {
    expect(rules.pickup(state({}, stack("dirt", 1)), 5, 0).slots.has(5)).toBe(false);
    expect(rules.pickup(state({}, stack("iron_helmet", 1)), 5, 0).slots.get(5)?.id).toBe(
      "minecraft:iron_helmet",
    );
    const cursed = stack("iron_helmet", 1, {
      ench: [{ id: "minecraft:binding_curse", lvl: 1, label: { tr: "", en: "" } }],
    });
    expect(rules.pickup(state({ 5: cursed }), 5, 0).cursor).toBeNull();
  });

  it("names where a shift-click goes, as InventoryMenu sends it", () => {
    const s = state({
      5: stack("iron_helmet", 1),
      20: stack("iron_boots", 1),
      21: stack("iron_helmet", 1),
      22: stack("shield", 1),
      23: stack("dirt", 5),
      40: stack("dirt", 5),
    });
    expect(rules.quickMoveTarget(s, 5)).toBe("inventory");
    expect(rules.quickMoveTarget(s, 20)).toBe("equipment");
    // The head slot is taken: the second helmet goes to the hotbar.
    expect(rules.quickMoveTarget(s, 21)).toBe("hotbar");
    expect(rules.quickMoveTarget(s, 22)).toBe("offhand");
    expect(rules.quickMoveTarget(s, 23)).toBe("hotbar");
    expect(rules.quickMoveTarget(s, 40)).toBe("inventory");
    expect(rules.quickMoveTarget(s, 9)).toBeNull();
  });

  it("shift-clicks like InventoryMenu: armour on, main to hotbar, hotbar to main", () => {
    expect(rules.quickMove(state({ 20: stack("iron_boots", 1) }), 20).slots.get(8)?.id).toBe(
      "minecraft:iron_boots",
    );
    expect([...rules.quickMove(state({ 20: stack("dirt", 5) }), 20).slots.keys()]).toEqual([36]);
    expect([...rules.quickMove(state({ 40: stack("dirt", 5) }), 40).slots.keys()]).toEqual([9]);
    const merge = rules.quickMove(state({ 20: stack("dirt", 10), 38: stack("dirt", 60) }), 20);
    expect([merge.slots.get(38)?.count, merge.slots.get(36)?.count]).toEqual([64, 6]);
  });
});

function planState(inv: rules.InvState, window: typeof SAMPLE_KITS | null = null): PlanState {
  return {
    screen: screenFor(window?.open ?? null),
    inv,
    win: new Map((window?.items ?? []).map(({ slot, ...s }) => [slot, s])),
    wseq: window?.wseq ?? 0,
    cursorOrigin: null,
    lang: "tr",
  };
}

const spotOf = (p: PlanState, key: string) => {
  const spot = p.screen.slots.find((s) => s.key === key);
  if (!spot) throw new Error(key);
  return spot;
};

describe("what each input sends", () => {
  it("sends a click with the window the player sees, and guesses only on the player's screen", () => {
    const player = planState(state({ 9: stack("dirt", 4) }));
    const [click] = planClick(player, spotOf(player, "inv:9"), 0);
    expect(click?.op).toEqual({
      action: "click",
      area: "inv",
      slot: 9,
      button: 0,
      mode: "pickup",
      wseq: 0,
    });
    expect(click?.predict).toBeDefined();
    const menu = planState(state({}), SAMPLE_KITS);
    const [kit] = planClick(menu, spotOf(menu, "win:11"), 0);
    expect(kit?.op).toMatchObject({ area: "win", slot: 11, wseq: SAMPLE_KITS.wseq });
    expect(kit?.predict).toBeUndefined();
    expect(planClick(player, spotOf(player, "inv:10"), 0)).toEqual([]);
  });

  it("never drops with an item on the cursor, and shift-clicks only a full slot", () => {
    const p = planState(state({ 9: stack("dirt", 4) }, stack("stone", 1)));
    expect(planDrop(p, spotOf(p, "inv:9"), true)).toEqual([]);
    const q = planState(state({ 9: stack("dirt", 4) }));
    expect(planDrop(q, spotOf(q, "inv:9"), false)[0]?.op).toEqual({
      action: "drop",
      area: "inv",
      slot: 9,
      stack: false,
      wseq: 0,
    });
    expect(planQuickMove(q, spotOf(q, "inv:10"))).toEqual([]);
  });

  it("builds a number-key swap from pickup clicks, only on the player's own screen", () => {
    const p = planState(state({ 20: stack("dirt", 4), 37: stack("stone", 2) }));
    const steps = planHotbarSwap(p, spotOf(p, "inv:20"), 1);
    expect(steps.map((s) => (s.op as { slot: number }).slot)).toEqual([20, 37, 20]);
    expect(steps.map((s) => s.expectCursor)).toEqual([
      { id: "minecraft:dirt", count: 4 },
      { id: "minecraft:stone", count: 2 },
      null,
    ]);
    expect(
      planHotbarSwap(planState(state({ 20: stack("dirt", 4) })), spotOf(p, "inv:20"), 0),
    ).toHaveLength(2);
    const menu = planState(state({ 9: stack("dirt", 1) }), SAMPLE_KITS);
    expect(planHotbarSwap(menu, spotOf(menu, "inv:9"), 0)).toEqual([]);
  });

  it("puts the cursor's item back where it came from, else into the first free slot", () => {
    const p = {
      ...planState(state({ 9: stack("dirt", 1) }, stack("stone", 3))),
      cursorOrigin: slotKey("inv", 12),
    };
    expect(planPutBack(p)[0]?.op).toMatchObject({ slot: 12 });
    const q = planState(state({ 9: stack("dirt", 1) }, stack("stone", 3)));
    expect(planPutBack(q)[0]?.op).toMatchObject({ slot: 10 });
  });
});

describe("moves on their way", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  function queue(answers: InvResult[]) {
    const messages: string[] = [];
    const spoken: string[] = [];
    let truth: rules.InvState | null = state({ 9: stack("dirt", 4) });
    const sent: unknown[] = [];
    const q = new MoveQueue({
      send: (op) => {
        sent.push(op);
        return Promise.resolve(answers.shift() ?? { ok: true });
      },
      truth: () => truth,
      changed: () => {},
      message: (m) => messages.push(m),
      speak: (text, item) => spoken.push(`${text}:${item}`),
      origin: () => {},
    });
    return { q, messages, spoken, sent, setTruth: (s: rules.InvState) => (truth = s) };
  }
  const p = planState(state({ 9: stack("dirt", 4) }));

  it("shows a guess until a frame follows the answer, then speaks", async () => {
    const { q, spoken } = queue([{ ok: true }]);
    q.run(planClick(p, spotOf(p, "inv:9"), 0));
    expect(q.shown(p.inv).state?.cursor?.count).toBe(4);
    expect(q.shown(p.inv).keys.has("inv:9")).toBe(true);
    await vi.advanceTimersByTimeAsync(10);
    q.frame();
    expect(q.waiting).toHaveLength(0);
    expect(spoken).toEqual(["pickedUp:dirt"]);
  });

  it("puts the view back on a refusal and says why, never for stale", async () => {
    const { q, messages } = queue([
      { error: { code: "busy_trade" } },
      { error: { code: "stale" } },
    ]);
    q.run(planClick(p, spotOf(p, "inv:9"), 0));
    await vi.advanceTimersByTimeAsync(10);
    expect(q.shown(p.inv).state?.cursor).toBeNull();
    q.run(planClick(p, spotOf(p, "inv:9"), 0));
    await vi.advanceTimersByTimeAsync(10);
    expect(messages).toEqual(["tradeLocked"]);
  });

  it("waits out 'too fast' without sending again", async () => {
    const { q, messages, sent } = queue([{ error: { code: "rate_limited", retryAfter: 2 } }]);
    q.run(planClick(p, spotOf(p, "inv:9"), 0));
    await vi.advanceTimersByTimeAsync(10);
    expect(q.run(planClick(p, spotOf(p, "inv:9"), 0))).toBe(false);
    expect(sent).toHaveLength(1);
    expect(messages).toEqual(["slowDown", "slowDown"]);
    await vi.advanceTimersByTimeAsync(2100);
    expect(q.run(planClick(p, spotOf(p, "inv:9"), 0))).toBe(true);
  });

  it("says the server refused when nothing changed after an ok", async () => {
    const { q, messages } = queue([{ ok: true }]);
    q.run(planClick(p, spotOf(p, "inv:9"), 0));
    await vi.advanceTimersByTimeAsync(2000);
    expect(messages).toEqual(["refused"]);
  });

  it("sends a chain's next click only when the frame shows the cursor it needs", async () => {
    const swap = planState(state({ 20: stack("dirt", 4), 37: stack("stone", 2) }));
    const steps = planHotbarSwap(swap, spotOf(swap, "inv:20"), 1);
    const { q, sent, setTruth } = queue([]);
    q.run(steps);
    await vi.advanceTimersByTimeAsync(10);
    setTruth(state({ 37: stack("stone", 2) }, stack("dirt", 4)));
    q.frame();
    expect(sent).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(10);
    setTruth(state({ 37: stack("stone", 2) }, stack("dirt", 4))); // the server did nothing
    q.frame();
    expect(sent).toHaveLength(2);
  });
});

describe("where the tooltip goes", () => {
  const viewport = { width: 1280, height: 760 };
  const words = { width: 120, height: 40 };
  // A slot 54 CSS pixels wide at 3 per GUI pixel.
  const slot = { left: 400, top: 300, right: 454, bottom: 354 };

  it("follows the pointer with the game's offsets, to its left near the edge", () => {
    expect(tooltipPlace({ pointer: { x: 500, y: 400 } }, words, viewport, 3)).toEqual({
      x: 536,
      y: 364,
    });
    expect(tooltipPlace({ pointer: { x: 1200, y: 400 } }, words, viewport, 3)).toEqual({
      x: 1200 - 72 - 120,
      y: 364,
    });
  });

  it("sits beside a focused slot, clear of its brackets, else on its other side", () => {
    // 7 GUI pixels past the slot's edge; the frame (4 more) starts past the brackets (2 out).
    expect(tooltipPlace({ slot, by: "keyboard" }, words, viewport, 3)).toEqual({
      x: 454 + 21,
      y: 306,
    });
    const atEdge = { left: 1180, top: 300, right: 1234, bottom: 354 };
    expect(tooltipPlace({ slot: atEdge, by: "keyboard" }, words, viewport, 3)).toEqual({
      x: 1180 - 21 - 120,
      y: 306,
    });
  });

  it("goes above a tapped slot, centred, or below it near the top", () => {
    expect(tooltipPlace({ slot, by: "touch" }, words, viewport, 3)).toEqual({
      x: 427 - 60,
      y: 300 - 12 - 40 - 3,
    });
    const nearTop = { left: 400, top: 20, right: 454, bottom: 74 };
    expect(tooltipPlace({ slot: nearTop, by: "touch" }, words, viewport, 3).y).toBe(74 + 12 + 3);
  });
});

describe("the stand-in machine", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("answers like the gate: stale windows, the cursor rule, the rate limit", async () => {
    let t = 0;
    const machine = createMockMachine({
      inventory: withCursor(SAMPLE_INVENTORY, 13),
      latency: 0,
      frameDelay: 0,
      now: () => t,
    });
    const answers: InvResult[] = [];
    void machine
      .op({ action: "drop", area: "inv", slot: 9, stack: true, wseq: 0 })
      .then((r) => answers.push(r));
    void machine
      .op({ action: "click", area: "inv", slot: 9, button: 0, mode: "pickup", wseq: 5 })
      .then((r) => answers.push(r));
    for (let i = 0; i < 13; i++) {
      void machine.op({ action: "hold", slot: i % 9 }).then((r) => answers.push(r));
    }
    await vi.advanceTimersByTimeAsync(10);
    expect(answers[0]).toEqual({ error: { code: "invalid", reason: "cursor" } });
    expect(answers[1]).toEqual({ error: { code: "stale" } });
    expect(answers.filter((a) => "error" in a && a.error.code === "rate_limited")).toHaveLength(1);
    t += 1000;
    machine.dispose();
  });
});
