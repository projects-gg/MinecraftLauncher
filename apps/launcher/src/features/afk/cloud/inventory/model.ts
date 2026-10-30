// The cloud inventory's frames as the live socket brings them (pl_cloud_afk's `inventory`,
// `window`, `invState` events), turned into `@pl/ui/inventory`'s own types, and the state the view
// shows. Nothing is stored: the frames live while the view is on screen.
import type { InvCursor, InvError, InvFrame, InvItem, InvResult, WinFrame } from "@pl/ui/inventory";
import type { CloudInvError, CloudInvItem, CloudLiveEvent } from "@/bindings";

type Label = { tr?: string; en?: string } | null | undefined;

const label = (value: Label) => ({ tr: value?.tr ?? "", en: value?.en ?? "" });

export function toInvItem(item: CloudInvItem): InvItem {
  const out: InvItem = {
    slot: item.slot ?? 0,
    id: item.id || "minecraft:air",
    count: item.count ?? 1,
    label: label(item.label),
  };
  if (item.name) out.name = item.name;
  // Machine 0.3.2: the looks of the name and lore, a head's skin (@pl/ui checks them again).
  if (item.name && item.nameStyle?.length) out.nameStyle = item.nameStyle;
  if (item.lore?.length) out.lore = item.lore;
  if (item.lore?.length && item.loreStyle?.some(Boolean)) out.loreStyle = item.loreStyle;
  if (item.skin) out.skin = item.skin;
  if (item.durability) {
    out.durability = { left: item.durability.left ?? 0, max: item.durability.max ?? 0 };
  }
  if (item.ench?.length) {
    out.ench = item.ench.map((e) => ({ id: e.id ?? "", lvl: e.lvl ?? 1, label: label(e.label) }));
  }
  if (item.enchanted) out.enchanted = true;
  if (item.inside) {
    const top = (item.inside.top ?? []).map((s) => ({
      id: s.id ?? "",
      count: s.count ?? 1,
      label: label(s.label),
    }));
    out.inside = { count: item.inside.count ?? 0, top };
    // Newer machines count the stacks (pl_cloud_afk reads a whole number up to 256): kept when it
    // is at least the stacks listed, as @pl/ui wants it.
    const stacks = item.inside.stacks;
    if (typeof stacks === "number" && stacks >= top.length) out.inside.stacks = stacks;
  }
  if (item.potion) out.potion = item.potion;
  if (typeof item.color === "number") out.color = item.color;
  if (item.partial) out.partial = true;
  if (typeof item.cmd === "number") out.cmd = item.cmd;
  if (item.model) out.model = item.model;
  return out;
}

export function toCursor(item: CloudInvItem | null | undefined): InvCursor {
  if (!item) return null;
  const { slot: _slot, ...rest } = toInvItem(item);
  return rest;
}

export function toInvError(error: CloudInvError | null | undefined): InvError {
  const out: InvError = { code: error?.code || "host_offline" };
  if (error?.reason) out.reason = error.reason;
  if (error?.field) out.field = error.field;
  if (typeof error?.retryAfter === "number") out.retryAfter = error.retryAfter;
  return out;
}

export function toInvResult(ok: boolean, error: CloudInvError | null | undefined): InvResult {
  return ok ? { ok: true } : { ok: false, error: toInvError(error) };
}

/**
 * What the view shows: `waiting` until the machine said whether the inventory shows, `ok` with its
 * frames, `off` with the machine's reason (`not_running`, `setting`, `restart`, `version`,
 * `service`).
 */
export interface InventoryState {
  status: "waiting" | "ok" | "off";
  reason: string | null;
  /** An AFK+ trade holds it: the view stays live, moves wait. */
  locked: boolean;
  inventory: InvFrame | null;
  window: WinFrame | null;
}

export const WAITING: InventoryState = {
  status: "waiting",
  reason: null,
  locked: false,
  inventory: null,
  window: null,
};

/** The next state after one of the account's events (others leave it as it is). */
export function reduce(state: InventoryState, event: CloudLiveEvent, acct: string): InventoryState {
  switch (event.kind) {
    case "invState":
      if (event.acct !== acct) return state;
      return event.state === "ok"
        ? { ...state, status: "ok", reason: null, locked: event.lock === "trade" }
        : { ...WAITING, status: "off", reason: event.reason ?? "service" };
    case "inventory":
      if (event.acct !== acct) return state;
      return {
        ...state,
        status: state.status === "off" ? state.status : "ok",
        inventory: {
          held: event.held ?? 0,
          cursor: toCursor(event.cursor),
          items: (event.items ?? []).map(toInvItem),
        },
      };
    case "window":
      if (event.acct !== acct) return state;
      if (event.closed || !event.open) {
        // A close for an opening the view no longer shows changes nothing.
        return state.window && state.window.wseq !== event.wseq
          ? state
          : { ...state, window: null };
      }
      return {
        ...state,
        window: {
          wseq: event.wseq,
          open: {
            type: event.open.type ?? "unknown",
            title: event.open.title ?? "",
            size: event.open.size ?? 0,
          },
          items: (event.items ?? []).map(toInvItem),
        },
      };
    default:
      return state;
  }
}

let next = 0;

/** A move's id: unique among this run's moves, 1–32 of `[A-Za-z0-9_-]`. */
export function moveId(): string {
  next = (next + 1) % 1_000_000;
  return `m${Date.now().toString(36)}${next.toString(36)}`;
}
