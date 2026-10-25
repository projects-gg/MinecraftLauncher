// Where everything sits, in GUI pixels (the game's own units: a slot is 18, an item 16), from the
// game's container screens: ChestMenu (generic_9xN), HopperMenu, DispenserMenu (3 × 3),
// ShulkerBoxMenu. Without a window the player's own screen has the hopper's height: the game's
// crafting grid and player model are not part of the cloud inventory (their slots are not in the
// contract), so armour and the off hand take one row under an "Equipment" title, on the grid's
// left edge, the off hand a step apart.
import type { InvArea, WinOpen } from "./types";

export type SlotKind =
  | "window"
  | "main"
  | "hotbar"
  | "head"
  | "chest"
  | "legs"
  | "feet"
  | "offhand";

export type SectionId = "window" | "equipment" | "main" | "hotbar";

export interface SlotSpot {
  /** "inv:9", "win:0". */
  key: string;
  area: InvArea;
  slot: number;
  kind: SlotKind;
  /** The item's top-left; the slot square is (x - 1, y - 1) and 18 wide. */
  x: number;
  y: number;
  section: SectionId;
}

export interface ScreenSection {
  id: SectionId;
  rows: SlotSpot[][];
}

export type ScreenKind = "player" | "generic" | "hopper" | "dispenser" | "shulker" | "single";

export interface Screen {
  kind: ScreenKind;
  width: number;
  height: number;
  /**
   * The title's place (where the game's text starts, the top of its letters; `center` centres it on
   * the panel): the window's title, or "Equipment" on the player's own screen.
   */
  title: { x: number; y: number; align: "start" | "center" } | null;
  /** The "Inventory" label above the player's rows. */
  inventoryLabel: { x: number; y: number } | null;
  slots: SlotSpot[];
  sections: ScreenSection[];
  /** Rows the window's own slots take (for unknown windows drawn as rows of 9). */
  windowRows: number;
}

export const SLOT = 18;
export const PANEL_WIDTH = 176;

/** inv slots: 5–8 armour (head, chest, legs, feet), 9–35 main, 36–44 hotbar, 45 off hand. */
export const ARMOR_SLOTS = [5, 6, 7, 8] as const;
export const MAIN_FIRST = 9;
export const HOTBAR_FIRST = 36;
export const OFFHAND_SLOT = 45;

const ARMOR_KINDS: SlotKind[] = ["head", "chest", "legs", "feet"];

/** Windows that do not carry the player inventory: clicks there are refused (the contract). */
const WITHOUT_PLAYER_INVENTORY = new Set(["lectern"]);

export function slotKey(area: InvArea, slot: number): string {
  return `${area}:${slot}`;
}

function spot(
  area: InvArea,
  slot: number,
  kind: SlotKind,
  x: number,
  y: number,
  section: SectionId,
): SlotSpot {
  return { key: slotKey(area, slot), area, slot, kind, x, y, section };
}

function playerRows(mainTop: number, hotbarTop: number): SlotSpot[] {
  const slots: SlotSpot[] = [];
  for (let row = 0; row < 3; row++) {
    for (let col = 0; col < 9; col++) {
      const slot = MAIN_FIRST + row * 9 + col;
      slots.push(spot("inv", slot, "main", 8 + col * SLOT, mainTop + row * SLOT, "main"));
    }
  }
  for (let col = 0; col < 9; col++) {
    slots.push(spot("inv", HOTBAR_FIRST + col, "hotbar", 8 + col * SLOT, hotbarTop, "hotbar"));
  }
  return slots;
}

function sectionsOf(slots: SlotSpot[]): ScreenSection[] {
  const order: SectionId[] = ["window", "equipment", "main", "hotbar"];
  const sections: ScreenSection[] = [];
  for (const id of order) {
    const own = slots.filter((s) => s.section === id);
    if (own.length === 0) continue;
    const byRow = new Map<number, SlotSpot[]>();
    for (const s of own) byRow.set(s.y, [...(byRow.get(s.y) ?? []), s]);
    const rows = [...byRow.entries()]
      .sort(([a], [b]) => a - b)
      .map(([, row]) => row.sort((a, b) => a.x - b.x));
    sections.push({ id, rows });
  }
  return sections;
}

/** The game's rows for a generic window: its type's N, else enough rows of 9 for its size. */
export function windowRows(open: WinOpen): number {
  const match = /^generic_9x([1-6])$/.exec(open.type);
  if (match) return Number(match[1]);
  return Math.max(1, Math.min(6, Math.ceil(Math.max(1, open.size) / 9)));
}

/** The gap the game leaves between groups of slots (the hotbar under the inventory). */
const GROUP_GAP = 4;

/**
 * The player's own screen (no window), as tall as the hopper's: "Equipment" where a window's title
 * goes, then head, chest, legs and feet from the grid's left edge and the off hand a group's gap
 * after them, then the inventory as under any window.
 */
function playerScreen(): Screen {
  const equipment: SlotSpot[] = ARMOR_SLOTS.map((slot, i) =>
    spot("inv", slot, ARMOR_KINDS[i] as SlotKind, 8 + i * SLOT, 20, "equipment"),
  );
  equipment.push(spot("inv", OFFHAND_SLOT, "offhand", 8 + 4 * SLOT + GROUP_GAP, 20, "equipment"));
  const slots = [...equipment, ...playerRows(51, 109)];
  return {
    kind: "player",
    width: PANEL_WIDTH,
    height: 133,
    title: { x: 8, y: 6, align: "start" },
    inventoryLabel: { x: 8, y: 39 },
    slots,
    sections: sectionsOf(slots),
    windowRows: 0,
  };
}

/** A screen for the window the server opened, with the player's rows under it. */
export function screenFor(open: WinOpen | null): Screen {
  if (!open) return playerScreen();
  const size = Math.max(0, Math.min(54, Math.floor(open.size)));

  if (WITHOUT_PLAYER_INVENTORY.has(open.type)) {
    const slots: SlotSpot[] = [];
    for (let i = 0; i < Math.max(1, size); i++) {
      slots.push(
        spot("win", i, "window", 8 + (i % 9) * SLOT, 18 + Math.floor(i / 9) * SLOT, "window"),
      );
    }
    const rows = Math.ceil(Math.max(1, size) / 9);
    return {
      kind: "single",
      width: PANEL_WIDTH,
      height: 18 + rows * SLOT + 7,
      title: { x: 8, y: 6, align: "start" },
      inventoryLabel: null,
      slots,
      sections: sectionsOf(slots),
      windowRows: rows,
    };
  }

  if (open.type === "hopper") {
    const slots: SlotSpot[] = [];
    for (let i = 0; i < Math.min(5, size || 5); i++) {
      slots.push(spot("win", i, "window", 44 + i * SLOT, 20, "window"));
    }
    slots.push(...playerRows(51, 109));
    return {
      kind: "hopper",
      width: PANEL_WIDTH,
      height: 133,
      title: { x: 8, y: 6, align: "start" },
      inventoryLabel: { x: 8, y: 39 },
      slots,
      sections: sectionsOf(slots),
      windowRows: 1,
    };
  }

  if (open.type === "generic_3x3" || open.type === "crafter_3x3") {
    const slots: SlotSpot[] = [];
    for (let i = 0; i < Math.min(9, size || 9); i++) {
      slots.push(
        spot("win", i, "window", 62 + (i % 3) * SLOT, 17 + Math.floor(i / 3) * SLOT, "window"),
      );
    }
    slots.push(...playerRows(84, 142));
    return {
      kind: "dispenser",
      width: PANEL_WIDTH,
      height: 166,
      title: { x: 0, y: 6, align: "center" },
      inventoryLabel: { x: 8, y: 72 },
      slots,
      sections: sectionsOf(slots),
      windowRows: 3,
    };
  }

  if (open.type === "shulker_box") {
    const slots: SlotSpot[] = [];
    for (let i = 0; i < Math.min(27, size || 27); i++) {
      slots.push(
        spot("win", i, "window", 8 + (i % 9) * SLOT, 18 + Math.floor(i / 9) * SLOT, "window"),
      );
    }
    slots.push(...playerRows(84, 142));
    return {
      kind: "shulker",
      width: PANEL_WIDTH,
      height: 167,
      title: { x: 8, y: 6, align: "start" },
      inventoryLabel: { x: 8, y: 73 },
      slots,
      sections: sectionsOf(slots),
      windowRows: 3,
    };
  }

  // generic_9xN, and any other window as rows of 9 (the contract's fallback).
  const rows = windowRows(open);
  const height = 114 + rows * SLOT;
  const count = /^generic_9x[1-6]$/.test(open.type) ? rows * 9 : Math.min(size, rows * 9);
  const slots: SlotSpot[] = [];
  for (let i = 0; i < count; i++) {
    slots.push(
      spot("win", i, "window", 8 + (i % 9) * SLOT, 18 + Math.floor(i / 9) * SLOT, "window"),
    );
  }
  slots.push(...playerRows(height - 83, height - 25));
  return {
    kind: "generic",
    width: PANEL_WIDTH,
    height,
    title: { x: 8, y: 6, align: "start" },
    inventoryLabel: { x: 8, y: height - 94 },
    slots,
    sections: sectionsOf(slots),
    windowRows: rows,
  };
}

export type Direction = "left" | "right" | "up" | "down" | "home" | "end" | "first" | "last";

/**
 * The slot the arrows lead to: left and right along the row, up and down to the nearest row in
 * that direction and the slot closest beneath; Home and End to the row's ends; first and last
 * (Ctrl+Home, Ctrl+End) to the screen's.
 */
export function neighbor(slots: SlotSpot[], from: SlotSpot, direction: Direction): SlotSpot {
  const sameRow = slots.filter((s) => s.y === from.y).sort((a, b) => a.x - b.x);
  switch (direction) {
    case "left":
      return [...sameRow].reverse().find((s) => s.x < from.x) ?? from;
    case "right":
      return sameRow.find((s) => s.x > from.x) ?? from;
    case "home":
      return sameRow[0] ?? from;
    case "end":
      return sameRow[sameRow.length - 1] ?? from;
    case "first":
    case "last": {
      const all = [...slots].sort((a, b) => a.y - b.y || a.x - b.x);
      return (direction === "first" ? all[0] : all[all.length - 1]) ?? from;
    }
    case "up":
    case "down": {
      const ahead = slots.filter((s) => (direction === "up" ? s.y < from.y : s.y > from.y));
      if (ahead.length === 0) return from;
      const nearestY = ahead.reduce(
        (best, s) => (Math.abs(s.y - from.y) < Math.abs(best - from.y) ? s.y : best),
        ahead[0]?.y ?? from.y,
      );
      const row = ahead.filter((s) => s.y === nearestY);
      return row.reduce((best, s) =>
        Math.abs(s.x - from.x) < Math.abs(best.x - from.x) ? s : best,
      );
    }
  }
}

/** Whether moves may name this slot while the window is open (armour and off hand may not). */
export function slotUsable(screen: Screen, spot: SlotSpot): boolean {
  if (spot.area === "win") return true;
  if (screen.kind === "player") return true;
  if (screen.kind === "single") return false;
  return spot.kind === "main" || spot.kind === "hotbar";
}
