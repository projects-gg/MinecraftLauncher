// What a player's click, key or menu row sends: one move, or for a number key a short chain of
// pickup clicks (the contract has no number-key swap; the game's own swap is two or three clicks).

import { HOTBAR_FIRST, MAIN_FIRST, type Screen, type SlotSpot, slotKey } from "./layout";
import * as rules from "./rules";
import { itemName } from "./text";
import type { InventoryLang, InventoryTexts } from "./texts";
import type { InvOp } from "./types";

/** What the view holds when a move is planned. */
export interface PlanState {
  screen: Screen;
  /** The player inventory as shown (with moves on their way). */
  inv: rules.InvState;
  /** The open window's items by slot. */
  win: Map<number, rules.Stack>;
  /** The window the player sees, 0 for none. */
  wseq: number;
  /** Where the cursor's item came from, when this view picked it up. */
  cursorOrigin: string | null;
  lang: InventoryLang;
}

export interface Step {
  op: InvOp;
  /** Slots that show the move waiting ("inv:9", "win:0", "cursor"). */
  keys: string[];
  /** The player's screen changes at once (never a server's menu). */
  predict?: (state: rules.InvState) => rules.InvState;
  /** In a chain: the cursor the next step needs (id and count), checked against the next frame. */
  expectCursor?: { id: string; count: number } | null;
  /** A move that must change something: no change after it means the server refused it. */
  mustChange?: boolean;
  /** What a screen reader hears when it went through. */
  speech?: { text: keyof InventoryTexts; item: string };
  /** The slot the cursor's item comes from (a pickup into an empty cursor). */
  origin?: string;
}

function stackAt(state: PlanState, spot: SlotSpot): rules.Stack | null {
  return (spot.area === "inv" ? state.inv.slots.get(spot.slot) : state.win.get(spot.slot)) ?? null;
}

function predicts(state: PlanState): boolean {
  return state.screen.kind === "player";
}

function click(state: PlanState, spot: SlotSpot, button: 0 | 1): InvOp {
  return {
    action: "click",
    area: spot.area,
    slot: spot.slot,
    button,
    mode: "pickup",
    wseq: state.wseq,
  };
}

/** A left (0) or right (1) click: take, place, swap, half, one. */
export function planClick(state: PlanState, spot: SlotSpot, button: 0 | 1): Step[] {
  const here = stackAt(state, spot);
  const carried = state.inv.cursor;
  if (!here && !carried) return [];
  const step: Step = {
    op: click(state, spot, button),
    keys: [spot.key, "cursor"],
    mustChange: true,
  };
  if (predicts(state)) step.predict = (s) => rules.pickup(s, spot.slot, button);
  if (here && !carried) {
    step.speech = { text: "pickedUp", item: itemName(here, state.lang) };
    step.origin = spot.key;
  } else if (!here && carried) {
    step.speech = { text: "placed", item: itemName(carried, state.lang) };
  } else if (here && carried) {
    step.speech = rules.sameItem(here, carried)
      ? { text: "placed", item: itemName(carried, state.lang) }
      : { text: "swapped", item: itemName(here, state.lang) };
  }
  return [step];
}

/** A shift-click: the stack moves to the other part (window and player, main and hotbar). */
export function planQuickMove(state: PlanState, spot: SlotSpot): Step[] {
  const here = stackAt(state, spot);
  if (!here) return [];
  const step: Step = {
    op: {
      action: "click",
      area: spot.area,
      slot: spot.slot,
      button: 0,
      mode: "quick_move",
      wseq: state.wseq,
    },
    keys: [spot.key],
    mustChange: true,
    speech: { text: "moved", item: itemName(here, state.lang) },
  };
  if (predicts(state)) step.predict = (s) => rules.quickMove(s, spot.slot);
  return [step];
}

/** Q (one) and Ctrl+Q (the stack). Never with an item on the cursor (the contract refuses). */
export function planDrop(state: PlanState, spot: SlotSpot, stack: boolean): Step[] {
  const here = stackAt(state, spot);
  if (!here || state.inv.cursor) return [];
  const step: Step = {
    op: { action: "drop", area: spot.area, slot: spot.slot, stack, wseq: state.wseq },
    keys: [spot.key],
    mustChange: true,
    speech: { text: "dropped", item: itemName(here, state.lang) },
  };
  if (predicts(state)) step.predict = (s) => rules.drop(s, spot.slot, stack);
  return [step];
}

/** "Kullan": the hotbar slot's item, as a right click into the air (never with a window open). */
export function planUse(state: PlanState, hotbar: number): Step[] {
  if (state.wseq !== 0 || state.screen.kind !== "player") return [];
  const item = state.inv.slots.get(HOTBAR_FIRST + hotbar);
  if (!item) return [];
  return [
    {
      op: { action: "use", slot: hotbar },
      keys: [slotKey("inv", HOTBAR_FIRST + hotbar)],
      speech: { text: "used", item: itemName(item, state.lang) },
    },
  ];
}

/** "Eline al": the hotbar slot the account holds. */
export function planHold(state: PlanState, hotbar: number): Step[] {
  if (state.inv.held === hotbar) return [];
  const item = state.inv.slots.get(HOTBAR_FIRST + hotbar);
  return [
    {
      op: { action: "hold", slot: hotbar },
      keys: [slotKey("inv", HOTBAR_FIRST + hotbar)],
      predict: (s) => rules.hold(s, hotbar),
      speech: item ? { text: "held", item: itemName(item, state.lang) } : undefined,
    },
  ];
}

/**
 * A number key over a slot: the slot and hotbar slot trade places, as two or three pickup clicks,
 * each sent when the frame shows the cursor the next one needs. Only on the player's own screen:
 * in a server's menu a pickup click is the menu's button (a shop would sell twice).
 */
export function planHotbarSwap(state: PlanState, spot: SlotSpot, hotbar: number): Step[] {
  if (state.screen.kind !== "player" || spot.area !== "inv" || state.inv.cursor) return [];
  const target = HOTBAR_FIRST + hotbar;
  if (spot.slot === target) return [];
  const a = state.inv.slots.get(spot.slot) ?? null;
  const h = state.inv.slots.get(target) ?? null;
  if (!a && !h) return [];
  if (a && !rules.mayPickup(spot.slot, a)) return [];
  if (h && !rules.mayPlace(spot.slot, h)) return [];
  const hotbarSpot: SlotSpot = {
    ...spot,
    key: slotKey("inv", target),
    slot: target,
    kind: "hotbar",
  };
  const keys = [spot.key, hotbarSpot.key, "cursor"];
  const at = (s: SlotSpot, button: 0 | 1 = 0): Step => ({
    op: click(state, s, button),
    keys,
    predict: (inv) => rules.pickup(inv, s.slot, button),
  });
  const cursorOf = (stack: rules.Stack | null) =>
    stack ? { id: stack.id, count: stack.count } : null;

  if (a && !h) {
    return [
      { ...at(spot), expectCursor: cursorOf(a) },
      {
        ...at(hotbarSpot),
        expectCursor: null,
        speech: { text: "moved", item: itemName(a, state.lang) },
      },
    ];
  }
  if (!a && h) {
    return [
      { ...at(hotbarSpot), expectCursor: cursorOf(h) },
      { ...at(spot), expectCursor: null, speech: { text: "moved", item: itemName(h, state.lang) } },
    ];
  }
  if (a && h && rules.sameItem(a, h)) {
    const room = rules.slotLimit(target, h) - h.count;
    if (room <= 0) return [];
    const rest = Math.max(0, a.count - room);
    const steps: Step[] = [
      { ...at(spot), expectCursor: cursorOf(a) },
      { ...at(hotbarSpot), expectCursor: rest > 0 ? { id: a.id, count: rest } : null },
    ];
    if (rest > 0) steps.push({ ...at(spot), expectCursor: null });
    const last = steps[steps.length - 1];
    if (last) last.speech = { text: "moved", item: itemName(a, state.lang) };
    return steps;
  }
  if (a && h) {
    return [
      { ...at(spot), expectCursor: cursorOf(a) },
      { ...at(hotbarSpot), expectCursor: cursorOf(h) },
      {
        ...at(spot),
        expectCursor: null,
        speech: { text: "swapped", item: itemName(h, state.lang) },
      },
    ];
  }
  return [];
}

/**
 * The cursor's item goes back (Esc, or a click outside the grid): to the slot it came from when
 * that is free, else the first free slot of the main inventory and hotbar, else a stack it joins.
 * It never drops: the game would throw it out of the window.
 */
export function planPutBack(state: PlanState): Step[] {
  const carried = state.inv.cursor;
  if (!carried) return [];
  const spots = state.screen.slots;
  const origin = state.cursorOrigin ? spots.find((s) => s.key === state.cursorOrigin) : undefined;
  const free = (s: SlotSpot) =>
    !stackAt(state, s) && (s.area === "win" || rules.mayPlace(s.slot, carried));
  const main = spots.filter((s) => s.area === "inv" && s.slot >= MAIN_FIRST && s.slot < 45);
  const target =
    (origin && free(origin) ? origin : undefined) ??
    main.find(free) ??
    main.find((s) => {
      const here = stackAt(state, s);
      return here && rules.sameItem(here, carried) && here.count < rules.slotLimit(s.slot, here);
    });
  if (!target) return [];
  const step: Step = {
    op: click(state, target, 0),
    keys: [target.key, "cursor"],
    mustChange: true,
    speech: { text: "putBackDone", item: itemName(carried, state.lang) },
  };
  if (predicts(state)) step.predict = (s) => rules.pickup(s, target.slot, 0);
  return [step];
}

/** Closing the window (never with an item on the cursor: the contract refuses). */
export function planClose(state: PlanState): Step[] {
  if (state.wseq === 0 || state.inv.cursor) return [];
  return [{ op: { action: "close", wseq: state.wseq }, keys: [] }];
}
