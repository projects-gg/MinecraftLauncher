// A stand-in for the account's machine, for the preview, the tests and apps' mock modes: it
// answers moves like the machine's gate (the shape it can see, `stale`, the cursor rule, the rate
// limits) and sends the frames that follow after a delay, the way the live socket would. A
// storage window follows the game's rules; a server menu takes its clicks itself (a kit closes the
// menu and lands in the inventory; a pane does nothing).
import * as rules from "./rules";
import type { InvFrame, InvItem, InvOp, InvResult, WinFrame } from "./types";

export type MockWindowKind = "storage" | "menu";

export interface MockMachineOptions {
  inventory: InvFrame;
  window?: WinFrame | null;
  windowKind?: MockWindowKind;
  /** Until the answer (`invres`), in milliseconds (default 180). */
  latency?: number;
  /** From the answer to the frame that shows the result (default 260). */
  frameDelay?: number;
  /** The machine's bucket: a burst, then so many a second (defaults 12 and 4). */
  burst?: number;
  perSecond?: number;
  /** The view's "Studio switch": `view` refuses every move but a snapshot. */
  mode?: "full" | "view";
  /** An AFK+ trade holds the inventory: moves are refused with `busy_trade`. */
  trade?: boolean;
  now?: () => number;
}

export interface MockMachine {
  inventory(): InvFrame;
  window(): WinFrame | null;
  op(op: InvOp): Promise<InvResult>;
  subscribe(listener: (inventory: InvFrame, window: WinFrame | null) => void): () => void;
  /** Opens (or with null closes) a window, as the server would. */
  open(window: WinFrame | null, kind?: MockWindowKind): void;
  setTrade(trade: boolean): void;
  dispose(): void;
}

const WIN_BASE = 100;

export function createMockMachine(options: MockMachineOptions): MockMachine {
  const latency = options.latency ?? 180;
  const frameDelay = options.frameDelay ?? 260;
  const burst = options.burst ?? 12;
  const perSecond = options.perSecond ?? 4;
  const now = options.now ?? (() => Date.now());
  let inv = rules.stateOf(options.inventory);
  let win: WinFrame | null = options.window ?? null;
  let kind: MockWindowKind = options.windowKind ?? "storage";
  let trade = options.trade ?? false;
  let tokens = burst;
  let refilledAt = now();
  let nextWseq = 2_000_000;
  const listeners = new Set<(inventory: InvFrame, window: WinFrame | null) => void>();
  const timers = new Set<ReturnType<typeof setTimeout>>();

  const later = (ms: number, fn: () => void) => {
    const timer = setTimeout(() => {
      timers.delete(timer);
      fn();
    }, ms);
    timers.add(timer);
  };
  const emit = () => {
    const frame = rules.frameOf(inv);
    for (const listener of listeners) listener(frame, win);
  };

  /** The window's items and the player's main and hotbar as one state (window slots 100+). */
  const combined = (): rules.InvState => {
    const slots = new Map<number, rules.Stack>();
    for (const [slot, stack] of inv.slots) if (slot >= 9 && slot <= 44) slots.set(slot, stack);
    for (const { slot, ...stack } of win?.items ?? []) slots.set(WIN_BASE + slot, stack);
    return { held: inv.held, cursor: inv.cursor, slots };
  };
  const split = (state: rules.InvState) => {
    const slots = new Map<number, rules.Stack>();
    for (const [slot, stack] of inv.slots) if (slot < 9 || slot > 44) slots.set(slot, stack);
    const items: InvItem[] = [];
    for (const [slot, stack] of state.slots) {
      if (slot >= WIN_BASE) items.push({ slot: slot - WIN_BASE, ...stack });
      else slots.set(slot, stack);
    }
    inv = { held: state.held, cursor: state.cursor, slots };
    if (win) win = { ...win, items: items.sort((a, b) => a.slot - b.slot) };
  };

  const take = (): boolean => {
    const t = now();
    tokens = Math.min(burst, tokens + ((t - refilledAt) / 1000) * perSecond);
    refilledAt = t;
    if (tokens < 1) return false;
    tokens -= 1;
    return true;
  };

  const apply = (op: InvOp) => {
    if (op.action === "hold") {
      inv = rules.hold(inv, op.slot);
      return true;
    }
    if (op.action === "close") {
      win = null;
      return true;
    }
    if (op.action === "use") {
      if (op.slot !== undefined) inv = rules.hold(inv, op.slot);
      const held = inv.slots.get(36 + inv.held);
      if (held && /(beef|bread|apple|carrot|potato|porkchop|chicken|mutton)$/.test(held.id)) {
        inv = rules.drop(inv, 36 + inv.held, false);
        return true;
      }
      return op.slot !== undefined;
    }
    if (op.action === "snapshot") return true;
    if (!win) {
      if (op.action === "drop") inv = rules.drop(inv, op.slot, op.stack);
      else if (op.mode === "quick_move") inv = rules.quickMove(inv, op.slot);
      else inv = rules.pickup(inv, op.slot, op.button);
      return true;
    }
    const id = op.area === "win" ? WIN_BASE + op.slot : op.slot;
    if (kind === "menu" && op.area === "win") {
      const clicked = win.items.find((it) => it.slot === op.slot);
      if (!clicked?.lore || op.action !== "click") return false;
      // A kit: the menu closes and the kit's item lands in the inventory.
      win = null;
      const { slot: _slot, lore: _lore, ...stack } = clicked;
      const rest = rules.moveInto(inv, { ...stack, name: undefined, count: 1 }, range(9, 45));
      return rest === null || true;
    }
    const state = combined();
    if (op.action === "drop") {
      const here = state.slots.get(id);
      if (!here) return false;
      if (op.stack || here.count === 1) state.slots.delete(id);
      else state.slots.set(id, { ...here, count: here.count - 1 });
    } else if (op.mode === "quick_move") {
      const stack = state.slots.get(id);
      if (!stack) return false;
      const size = win.open.size;
      // ChestMenu.quickMoveStack: from the window into the player's rows from the end, else in.
      const order =
        op.area === "win"
          ? [...range(36, 45).reverse(), ...range(9, 36).reverse()]
          : range(WIN_BASE, WIN_BASE + size);
      const rest = rules.moveInto(state, stack, order);
      if (rest) state.slots.set(id, rest);
      else state.slots.delete(id);
    } else {
      const next = rules.pickup(state, id, op.button);
      state.slots = next.slots;
      state.cursor = next.cursor;
    }
    split(state);
    return true;
  };

  return {
    inventory: () => rules.frameOf(inv),
    window: () => win,
    op(op) {
      return new Promise((resolve) => {
        const answer = (result: InvResult) => later(latency, () => resolve(result));
        if (options.mode === "view" && op.action !== "snapshot") {
          answer({ error: { code: "inventory_off", reason: "view" } });
          return;
        }
        if (trade && op.action !== "snapshot") {
          answer({ error: { code: "busy_trade" } });
          return;
        }
        const wseq = win?.wseq ?? 0;
        if ("wseq" in op && op.wseq !== wseq) {
          answer({ error: { code: "stale" } });
          return;
        }
        if ((op.action === "drop" || op.action === "close") && inv.cursor) {
          answer({ error: { code: "invalid", reason: "cursor" } });
          return;
        }
        if (op.action === "use" && win) {
          answer({ error: { code: "invalid", reason: "window" } });
          return;
        }
        if (op.action !== "snapshot" && !take()) {
          answer({ error: { code: "rate_limited", retryAfter: 1 } });
          return;
        }
        answer({ ok: true });
        later(latency + frameDelay, () => {
          if (apply(op)) emit();
        });
      });
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    open(window, windowKind = "storage") {
      win = window ? { ...window, wseq: nextWseq++ } : null;
      kind = windowKind;
      emit();
    },
    setTrade(value) {
      trade = value;
    },
    dispose() {
      for (const timer of timers) clearTimeout(timer);
      timers.clear();
      listeners.clear();
    },
  };
}

function range(start: number, end: number): number[] {
  return Array.from({ length: end - start }, (_, i) => start + i);
}
