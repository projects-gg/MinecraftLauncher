// The game's click rules for the player's own screen (AbstractContainerMenu.doClick and
// InventoryMenu.quickMoveStack), used to show a move at once while the machine carries it out.
// Only the player's screen is predicted: a server's menu decides its clicks itself (a shop takes
// the click and sells), so a click there waits for the next frame instead.
import { bareId } from "./text";
import type { InvCursor, InvFrame, InvItem } from "./types";

export type Stack = NonNullable<InvCursor>;

/** The player inventory as the view holds it: the slot numbers 5–45 of the contract. */
export interface InvState {
  held: number;
  cursor: Stack | null;
  slots: Map<number, Stack>;
}

export function stateOf(frame: InvFrame): InvState {
  const slots = new Map<number, Stack>();
  for (const item of frame.items) {
    const { slot, ...stack } = item;
    if (stack.count > 0) slots.set(slot, stack);
  }
  return {
    held: frame.held,
    cursor: frame.cursor && frame.cursor.count > 0 ? frame.cursor : null,
    slots,
  };
}

export function frameOf(state: InvState): InvFrame {
  const items: InvItem[] = [...state.slots.entries()]
    .sort(([a], [b]) => a - b)
    .map(([slot, stack]) => ({ slot, ...stack }));
  return { held: state.held, cursor: state.cursor, items };
}

const ONE = new Set([
  "potion",
  "splash_potion",
  "lingering_potion",
  "enchanted_book",
  "written_book",
  "writable_book",
  "cake",
  "saddle",
  "minecart",
  "totem_of_undying",
  "elytra",
  "shield",
  "bow",
  "crossbow",
  "trident",
  "mace",
  "fishing_rod",
  "carrot_on_a_stick",
  "warped_fungus_on_a_stick",
  "flint_and_steel",
  "shears",
  "brush",
  "spyglass",
  "bundle",
  "goat_horn",
  "knowledge_book",
  "debug_stick",
  "mushroom_stew",
  "rabbit_stew",
  "beetroot_soup",
  "suspicious_stew",
  "shulker_box",
]);
const ONE_SUFFIX = [
  "_sword",
  "_pickaxe",
  "_axe",
  "_shovel",
  "_hoe",
  "_spear",
  "_helmet",
  "_chestplate",
  "_leggings",
  "_boots",
  "_horse_armor",
  "_shulker_box",
  "_bed",
  "_boat",
  "_raft",
  "_minecart",
  "_bundle",
  "_banner_pattern",
];
const SIXTEEN = new Set([
  "ender_pearl",
  "snowball",
  "egg",
  "blue_egg",
  "brown_egg",
  "bucket",
  "honey_bottle",
  "armor_stand",
]);
const SIXTEEN_SUFFIX = ["_sign", "_hanging_sign", "_banner"];

/** How many of an item stack, as the game counts it for the common items (else 64). */
export function maxStack(item: Stack): number {
  const path = bareId(item.id);
  let max = 64;
  if (item.durability || ONE.has(path) || path.startsWith("music_disc_")) max = 1;
  else if (path.endsWith("_bucket")) max = 1;
  else if (ONE_SUFFIX.some((suffix) => path.endsWith(suffix))) max = 1;
  else if (SIXTEEN.has(path) || SIXTEEN_SUFFIX.some((suffix) => path.endsWith(suffix))) max = 16;
  // A server may stack beyond the usual; what is there already is possible.
  return Math.max(max, item.count);
}

export type ArmorPart = "head" | "chest" | "legs" | "feet";

const HEADWEAR = new Set([
  "turtle_helmet",
  "carved_pumpkin",
  "player_head",
  "zombie_head",
  "creeper_head",
  "skeleton_skull",
  "wither_skeleton_skull",
  "piglin_head",
  "dragon_head",
]);

/** The armour slot an item goes into, if any. */
export function armorPart(item: Stack): ArmorPart | null {
  const path = bareId(item.id);
  if (path.endsWith("_helmet") || HEADWEAR.has(path)) return "head";
  if (path.endsWith("_chestplate") || path === "elytra") return "chest";
  if (path.endsWith("_leggings")) return "legs";
  if (path.endsWith("_boots")) return "feet";
  return null;
}

const ARMOR_SLOT: Record<ArmorPart, number> = { head: 5, chest: 6, legs: 7, feet: 8 };
const ARMOR_OF_SLOT: Record<number, ArmorPart> = { 5: "head", 6: "chest", 7: "legs", 8: "feet" };

function isArmorSlot(slot: number): boolean {
  return slot >= 5 && slot <= 8;
}

/** Whether a slot takes an item: an armour slot only its own armour; the rest anything. */
export function mayPlace(slot: number, item: Stack): boolean {
  return isArmorSlot(slot) ? armorPart(item) === ARMOR_OF_SLOT[slot] : true;
}

/** Whether an item may leave its slot: armour cursed with binding stays on. */
export function mayPickup(slot: number, item: Stack): boolean {
  return !(isArmorSlot(slot) && item.ench?.some((e) => bareId(e.id) === "binding_curse"));
}

/** How many of this item the slot holds. */
export function slotLimit(slot: number, item: Stack): number {
  return isArmorSlot(slot) ? 1 : maxStack(item);
}

/** The game's "same item, same components", from what the frame shows of them. */
export function sameItem(a: Stack, b: Stack): boolean {
  return (
    a.id === b.id &&
    (a.name ?? "") === (b.name ?? "") &&
    JSON.stringify(a.lore ?? []) === JSON.stringify(b.lore ?? []) &&
    JSON.stringify((a.ench ?? []).map((e) => [e.id, e.lvl])) ===
      JSON.stringify((b.ench ?? []).map((e) => [e.id, e.lvl])) &&
    (a.potion ?? "") === (b.potion ?? "") &&
    (a.color ?? -1) === (b.color ?? -1) &&
    (a.durability?.left ?? -1) === (b.durability?.left ?? -1) &&
    (a.durability?.max ?? -1) === (b.durability?.max ?? -1) &&
    (a.cmd ?? -1) === (b.cmd ?? -1) &&
    (a.model ?? "") === (b.model ?? "") &&
    Boolean(a.enchanted) === Boolean(b.enchanted) &&
    // A head's profile and the text's look are components too.
    (a.skin ?? "") === (b.skin ?? "") &&
    JSON.stringify(a.nameStyle ?? null) === JSON.stringify(b.nameStyle ?? null) &&
    JSON.stringify(a.loreStyle ?? null) === JSON.stringify(b.loreStyle ?? null)
  );
}

function withCount(stack: Stack, count: number): Stack | null {
  return count > 0 ? { ...stack, count } : null;
}

function clone(state: InvState): InvState {
  return { held: state.held, cursor: state.cursor, slots: new Map(state.slots) };
}

function put(state: InvState, slot: number, stack: Stack | null) {
  if (stack) state.slots.set(slot, stack);
  else state.slots.delete(slot);
}

/** Slot.safeInsert: up to `n` of the carried stack into the slot; returns what stays carried. */
function safeInsert(state: InvState, slot: number, carried: Stack, n: number): Stack | null {
  if (!mayPlace(slot, carried)) return carried;
  const existing = state.slots.get(slot) ?? null;
  const room = slotLimit(slot, carried) - (existing?.count ?? 0);
  const moved = Math.min(n, carried.count, room);
  if (moved <= 0) return carried;
  if (!existing) put(state, slot, withCount(carried, moved));
  else if (sameItem(existing, carried))
    put(state, slot, withCount(existing, existing.count + moved));
  else return carried;
  return withCount(carried, carried.count - moved);
}

/** A pickup click (button 0 all, 1 half or one) on the player's screen. */
export function pickup(state: InvState, slot: number, button: 0 | 1): InvState {
  const next = clone(state);
  const here = next.slots.get(slot) ?? null;
  const carried = next.cursor;
  if (!here) {
    if (carried) next.cursor = safeInsert(next, slot, carried, button === 0 ? carried.count : 1);
    return next;
  }
  if (!mayPickup(slot, here)) {
    return next;
  }
  if (!carried) {
    const take = button === 0 ? here.count : Math.ceil(here.count / 2);
    next.cursor = withCount(here, take);
    put(next, slot, withCount(here, here.count - take));
    return next;
  }
  if (mayPlace(slot, carried)) {
    if (sameItem(here, carried)) {
      next.cursor = safeInsert(next, slot, carried, button === 0 ? carried.count : 1);
    } else if (carried.count <= slotLimit(slot, carried)) {
      next.cursor = here;
      put(next, slot, carried);
    }
    return next;
  }
  if (sameItem(here, carried)) {
    const take = Math.min(here.count, maxStack(carried) - carried.count);
    if (take > 0) {
      next.cursor = withCount(carried, carried.count + take);
      put(next, slot, withCount(here, here.count - take));
    }
  }
  return next;
}

/**
 * AbstractContainerMenu.moveItemStackTo over the slots in `order` (the game walks a range, either
 * way): merge into equal stacks first, then into the first empty slot. Returns what is left.
 */
export function moveInto(state: InvState, stack: Stack, order: number[]): Stack | null {
  let rest: Stack | null = stack;
  if (maxStack(stack) > 1) {
    for (const slot of order) {
      if (!rest) break;
      const existing = state.slots.get(slot);
      if (!existing || !sameItem(existing, rest)) continue;
      const limit = slotLimit(slot, existing);
      const total = existing.count + rest.count;
      if (total <= limit) {
        put(state, slot, withCount(existing, total));
        rest = null;
      } else if (existing.count < limit) {
        rest = withCount(rest, total - limit);
        put(state, slot, withCount(existing, limit));
      }
    }
  }
  if (rest) {
    for (const slot of order) {
      if (state.slots.has(slot) || !mayPlace(slot, rest)) continue;
      const moved = Math.min(slotLimit(slot, rest), rest.count);
      put(state, slot, withCount(rest, moved));
      rest = withCount(rest, rest.count - moved);
      break;
    }
  }
  return rest;
}

function range(start: number, end: number): number[] {
  return Array.from({ length: end - start }, (_, i) => start + i);
}

function moveTo(state: InvState, stack: Stack, start: number, end: number): Stack | null {
  return moveInto(state, stack, range(start, end));
}

/** Where a shift-click on the player's screen sends a stack (the menu row names it so). */
export type QuickMoveTarget = "inventory" | "hotbar" | "equipment" | "offhand";

/** InventoryMenu.quickMoveStack's choice for the slot's stack: the target and its slot range. */
function quickMoveRange(
  state: InvState,
  slot: number,
  stack: Stack,
): [QuickMoveTarget, number, number] {
  const part = armorPart(stack);
  if (isArmorSlot(slot) || slot === 45) return ["inventory", 9, 45];
  if (part && !state.slots.has(ARMOR_SLOT[part])) {
    return ["equipment", ARMOR_SLOT[part], ARMOR_SLOT[part] + 1];
  }
  if (bareId(stack.id) === "shield" && !state.slots.has(45)) return ["offhand", 45, 46];
  if (slot >= 9 && slot < 36) return ["hotbar", 36, 45];
  return ["inventory", 9, 36];
}

/** Where a shift-click on the player's screen would send the slot's stack, or null when empty. */
export function quickMoveTarget(state: InvState, slot: number): QuickMoveTarget | null {
  const stack = state.slots.get(slot);
  return stack ? quickMoveRange(state, slot, stack)[0] : null;
}

/** One InventoryMenu.quickMoveStack; returns whether anything moved. */
function quickMoveOnce(state: InvState, slot: number): boolean {
  const stack = state.slots.get(slot);
  if (!stack) return false;
  const [, start, end] = quickMoveRange(state, slot, stack);
  const rest = moveTo(state, stack, start, end);
  const moved = (rest?.count ?? 0) !== stack.count;
  if (moved) put(state, slot, rest);
  return moved;
}

/** A shift-click on the player's screen: the game repeats it while anything still moves. */
export function quickMove(state: InvState, slot: number): InvState {
  const next = clone(state);
  const stack = next.slots.get(slot);
  if (!stack || !mayPickup(slot, stack)) return next;
  for (let guard = 0; guard < 64 && quickMoveOnce(next, slot); guard++) {
    const left = next.slots.get(slot);
    if (!left || !sameItem(left, stack)) break;
  }
  return next;
}

/** Q and Ctrl+Q: one item, or the stack, leaves the slot. */
export function drop(state: InvState, slot: number, stack: boolean): InvState {
  const next = clone(state);
  const here = next.slots.get(slot);
  if (!here || next.cursor || !mayPickup(slot, here)) return next;
  put(next, slot, stack ? null : withCount(here, here.count - 1));
  return next;
}

export function hold(state: InvState, hotbar: number): InvState {
  return { ...clone(state), held: hotbar };
}
