// A machine's item checked field by field before any view draws it (TASTE-SWEEP, CONSOLE-COLOR): the
// site's live inventory and a console hover's item take nothing on trust, as the launcher's typed
// model (pl_cloud_afk) does not either. A required field out of shape drops the item; an optional
// one drops alone. Strings are plain text of bounded length without the forbidden characters (the
// console's one list), parts are the frame's [length, code] covering their text exactly.
import { clean } from "../console/looks";
import { STYLE_CODE } from "./text";
import type { InvCursor, InvEnchant, InvInsideStack, InvItem, InvLabel, TextSpan } from "./types";

const ID = /^[a-z0-9_.-]{1,64}:[a-z0-9_./-]{1,128}$/;
const SKIN = /^[0-9a-f]{16,64}$/;
const LIMIT = {
  text: 256,
  label: 128,
  lore: 32,
  spans: 64,
  ench: 32,
  inside: 9,
  /** A container's stack count (afk-host's cap). */
  stacks: 256,
  count: 1_000_000,
} as const;

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function int(value: unknown, min: number, max: number): number | undefined {
  return typeof value === "number" && Number.isInteger(value) && value >= min && value <= max
    ? value
    : undefined;
}

function text(value: unknown, max: number): string | undefined {
  return typeof value === "string" && value.length <= max && clean(value, false)
    ? value
    : undefined;
}

function labelOf(value: unknown): InvLabel | undefined {
  if (!isObject(value)) return undefined;
  const tr = text(value.tr, LIMIT.label);
  const en = text(value.en, LIMIT.label);
  return tr === undefined || en === undefined ? undefined : { tr, en };
}

/** Parts over `over`: at most 64 of [n >= 1, code], covering it exactly, never inside a character. */
function spansOf(value: unknown, over: string): TextSpan[] | undefined {
  if (!Array.isArray(value) || value.length === 0 || value.length > LIMIT.spans) return undefined;
  const out: TextSpan[] = [];
  let at = 0;
  for (const span of value) {
    if (!Array.isArray(span) || span.length !== 2) return undefined;
    const [n, code] = span as unknown[];
    if (int(n, 1, LIMIT.text) === undefined || typeof code !== "string" || !STYLE_CODE.test(code)) {
      return undefined;
    }
    at += n as number;
    if (at > over.length) return undefined;
    const high = over.charCodeAt(at - 1);
    if (high >= 0xd800 && high <= 0xdbff && at < over.length) return undefined;
    out.push([n as number, code]);
  }
  return at === over.length ? out : undefined;
}

function stackOf(value: unknown): InvInsideStack | undefined {
  if (!isObject(value) || typeof value.id !== "string" || !ID.test(value.id)) return undefined;
  const label = labelOf(value.label);
  const count = int(value.count, 1, LIMIT.count);
  return label && count !== undefined ? { id: value.id, count, label } : undefined;
}

function enchantOf(value: unknown): InvEnchant | undefined {
  if (!isObject(value) || typeof value.id !== "string" || !ID.test(value.id)) return undefined;
  const label = labelOf(value.label);
  const lvl = int(value.lvl, 0, 255);
  return label && lvl !== undefined ? { id: value.id, lvl, label } : undefined;
}

/** The item as a view may draw it, or null when its id, label, slot or count is out of shape. */
export function safeInvItem(value: unknown): InvItem | null {
  if (!isObject(value) || typeof value.id !== "string" || !ID.test(value.id)) return null;
  const label = labelOf(value.label);
  if (!label) return null;
  const slot = value.slot === undefined ? 0 : int(value.slot, 0, 1_000);
  const count = value.count === undefined ? 1 : int(value.count, 1, LIMIT.count);
  if (slot === undefined || count === undefined) return null;
  const item: InvItem = { slot, id: value.id, count, label };
  const name = text(value.name, LIMIT.text);
  if (name) {
    item.name = name;
    const style = spansOf(value.nameStyle, name);
    if (style) item.nameStyle = style;
  }
  if (Array.isArray(value.lore) && value.lore.length <= LIMIT.lore) {
    const lore = value.lore.map((line) => text(line, LIMIT.text));
    if (lore.length && lore.every((line) => line !== undefined)) {
      item.lore = lore as string[];
      if (Array.isArray(value.loreStyle) && value.loreStyle.length === lore.length) {
        const styles = value.loreStyle.map((spans, i) =>
          spans === null ? null : (spansOf(spans, lore[i] as string) ?? null),
        );
        if (styles.some(Boolean)) item.loreStyle = styles;
      }
    }
  }
  if (typeof value.skin === "string" && SKIN.test(value.skin)) item.skin = value.skin;
  if (isObject(value.durability)) {
    const max = int(value.durability.max, 1, LIMIT.count);
    const left = int(value.durability.left, 0, LIMIT.count);
    if (max !== undefined && left !== undefined && left <= max) item.durability = { left, max };
  }
  if (Array.isArray(value.ench) && value.ench.length <= LIMIT.ench) {
    const ench = value.ench.map(enchantOf);
    if (ench.length && ench.every(Boolean)) item.ench = ench as InvEnchant[];
  }
  if (value.enchanted === true) item.enchanted = true;
  if (
    isObject(value.inside) &&
    Array.isArray(value.inside.top) &&
    value.inside.top.length <= LIMIT.inside
  ) {
    const insideCount = int(value.inside.count, 0, LIMIT.count);
    const top = value.inside.top.map(stackOf);
    if (insideCount !== undefined && top.every(Boolean)) {
      item.inside = { count: insideCount, top: top as InvInsideStack[] };
      // A newer machine counts the stacks: at least the ones listed; otherwise that field drops alone.
      const stacks = int(value.inside.stacks, top.length, LIMIT.stacks);
      if (stacks !== undefined) item.inside.stacks = stacks;
    }
  }
  if (typeof value.potion === "string" && ID.test(value.potion)) item.potion = value.potion;
  const color = int(value.color, 0, 0xffffff);
  if (color !== undefined) item.color = color;
  if (value.partial === true) item.partial = true;
  const cmd = int(value.cmd, -2_147_483_648, 2_147_483_647);
  if (cmd !== undefined) item.cmd = cmd;
  if (typeof value.model === "string" && ID.test(value.model)) item.model = value.model;
  return item;
}

/** A frame's items checked one by one (an item out of shape stays out). */
export function safeInvItems(value: unknown): InvItem[] {
  return Array.isArray(value)
    ? value.map(safeInvItem).filter((item): item is InvItem => item !== null)
    : [];
}

/** The carried stack: an item without its slot, or none. */
export function safeInvCursor(value: unknown): InvCursor {
  if (value === null || value === undefined) return null;
  const item = safeInvItem(value);
  if (!item) return null;
  const { slot: _slot, ...cursor } = item;
  return cursor;
}
