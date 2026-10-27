// AFK+ order cards: what an order's draft still asks, a chip's effect on it, the order it saves
// as, and an order's status (projects-studio services/cloud-afk/README.md "AFK+ trade orders" →
// The card, Status and progress). The service reads the player's words once ("Kontrol et") and
// answers with a draft; the questions are answered here, without a second model call. A port of
// the contract's reference (test-vectors/afkplus-intake.mjs `questionsOf`, `answer`, `orderOf`,
// `fill`; afkplus-order.mjs `statusOf`); vectors/afkplus-intake-v1.json `drafts` and `answers`
// and vectors/afkplus-order-v1.json `status` run in order-draft.test.ts.
import type { ItemData } from "@pl/ui/items";

export type ItemRef = string;
export type Side = "buy" | "sell";

export interface DraftReq {
  ench: Record<string, number>;
  /** null: the default (90 for a received item that wears out, else 0). */
  dur: number | null;
  named: boolean;
  cursed: boolean;
}

export type DraftOpen = { options: ItemRef[] } | { mention: string } | { either: ItemRef };

/** How a custom item is drawn (IA-ART's overlay): either part may be missing. */
export interface Look {
  cmd?: number | null;
  model?: string | null;
}

/**
 * A custom item as an order names it (a new one has no `at` yet: the service sets it). `from`:
 * `inv`, `text` or `seen` (a stored one's as the service wrote it).
 */
export interface CustomItemLike {
  k: string;
  name: string;
  base: string | null;
  fp: string | null;
  look: Look | null;
  from: string;
  at?: number;
}

/** An order still being made: the fields the text gave, the defaults, `null` where a question is open. */
export interface OrderDraft {
  side: Side | null;
  item: ItemRef | null;
  qty: number | null;
  pay: "money" | ItemRef | null;
  unit: "each" | "total" | null;
  target: number | null;
  limit: number | null;
  style: "patient" | "balanced" | "quick";
  req: DraftReq | null;
  trade: "auto" | "owner";
  until: number;
  note: string;
  src: "text" | "form" | "inv";
  /** What target and limit count (they start again when it changes). */
  amounts: "money" | "count";
  open: { item?: DraftOpen; pay?: DraftOpen };
  /** Questions answered without a field ("far"). */
  ok: string[];
  /** New custom items the draft names (a first-sight one). */
  customItems?: CustomItemLike[];
}

export type OrderQuestion =
  | { kind: "which"; field: "item" | "pay"; options: ItemRef[] }
  | { kind: "side" }
  | {
      kind: "custom";
      field: "item" | "pay";
      mention: string;
      chips: ("pick" | "first_seen" | "fix")[];
    }
  | { kind: "limit"; side: Side; amounts: number[] }
  | { kind: "pay"; item: ItemRef }
  | { kind: "pay_off"; want: "item" | "money" }
  | { kind: "per"; price: number }
  | { kind: "qty"; amounts: number[] }
  | { kind: "far"; limit: number; target: number }
  | { kind: "over_cap"; limit: number; cap: number };

/** "Kontrol et"'s answer. */
export interface CheckAnswer {
  draft: OrderDraft;
  questions: OrderQuestion[];
  /** `approval_pref`, `named_player`, `notify_only`, `item_pay_off`. */
  notes: string[];
  /** The checks left today. */
  left: number;
}

/** What the questions are asked against: the account's settings, its server and its item data. */
export interface DraftContext {
  /** The account version's item data (undefined while it could not be loaded). */
  data: ItemData | undefined;
  /** The account's custom items. */
  custom: readonly CustomItemLike[];
  /** The money's cap and rounding steps (`[[from, step]…]`; null: two significant digits). */
  money: { maxDeal: number; round: readonly (readonly [number, number])[] | null };
  /** The account's payment setting. */
  pay: "money" | "items" | "both";
  features: { itemPay: boolean; customItems: boolean };
  limits: { perDealMax: number | null; totalBudget: number | null };
  /** Who makes the account's trades (`GET /plus` → `server.trade`). */
  trades: "bot" | "owner";
}

/** A chip's choice, by the question's kind. */
export type Choice =
  | { option: ItemRef }
  | { amount: number }
  | Side
  | "first_seen"
  | "pick"
  | "fix"
  | "money"
  | "item"
  | "turn_on"
  | "switch"
  | "each"
  | "total"
  | "yes"
  | "raise"
  | "use_cap";

export type AnswerResult =
  | { draft: OrderDraft; settings?: { pay?: "both"; limits?: { perDealMax: number } } }
  | { form: "item" | "pay" | "limit" }
  | { pick: "item" | "pay" };

/** An order as `POST …/plus/orders` takes it (the service sets `id` and `at`). */
export interface NewOrder {
  on: boolean;
  side: Side;
  item: ItemRef;
  qty: number;
  req: { ench: Record<string, number>; dur: number; named: boolean; cursed: boolean } | null;
  pay: "money" | ItemRef;
  unit: "each" | "total";
  target: number;
  limit: number;
  style: "patient" | "balanced" | "quick";
  trade: "auto" | "owner";
  until: number;
  note: string;
  src: "text" | "form" | "inv";
}

export const MAX_COUNT = 9_999;
export const CUSTOM_NAME_MAX = 32;

interface Facts {
  stack: number;
  maxDamage: number;
  /** The enchantments that fit, by id; null for a custom item (its data is the server's). */
  ench: string[] | null;
}

function itemFacts(data: ItemData | undefined, id: string | null): Facts | null {
  if (!data || id === null) return null;
  const item = data.byId.get(id);
  if (!item) return null;
  return {
    stack: item.stack,
    maxDamage: item.maxDamage,
    ench: item.enchantments.flatMap((i) => data.enchantments[i]?.id ?? []),
  };
}

/** The facts of an item reference: vanilla ones from the data, custom ones from their `base`. */
function refFacts(ref: ItemRef | null, ctx: DraftContext): Facts | null {
  if (ref === null) return null;
  if (ref.startsWith("custom:")) {
    const k = ref.slice("custom:".length);
    const custom = ctx.custom.find((item) => item.k === k);
    const base = custom?.base ? itemFacts(ctx.data, custom.base) : null;
    return { stack: base?.stack ?? 64, maxDamage: base?.maxDamage ?? 0, ench: null };
  }
  return itemFacts(ctx.data, ref);
}

/** The step to round `value` to: the profile's `money.round`, else 2 significant digits. */
function stepOf(value: number, round: DraftContext["money"]["round"]): number {
  if (Array.isArray(round)) {
    for (const [from, step] of round) {
      if (value >= from) return step;
    }
    return 1;
  }
  const digits = Math.floor(Math.log10(Math.max(1, value)));
  return digits < 2 ? 1 : 10 ** (digits - 1);
}

/**
 * The target a limit gives: buying 80 % of it rounded down, selling 120 % rounded up, by the
 * profile's rounding; counts of a paying item to whole items. Never 0, never past `maxDeal`.
 */
export function targetFromLimit(
  side: Side,
  limit: number,
  money: DraftContext["money"],
  counts: boolean,
): number {
  if (counts) {
    return side === "buy"
      ? Math.max(1, Math.floor(limit * 0.8))
      : Math.min(MAX_COUNT, Math.ceil(limit * 1.2));
  }
  if (side === "buy") {
    const raw = Math.floor(limit * 0.8);
    const step = stepOf(raw, money.round);
    const rounded = Math.floor(raw / step) * step;
    return rounded >= 1 ? rounded : Math.max(1, raw);
  }
  const raw = Math.ceil(limit * 1.2);
  const step = stepOf(raw, money.round);
  return Math.min(money.maxDeal, Math.ceil(raw / step) * step);
}

/** A chip amount: to 2 significant digits, the nearer way (counts below 100 stay whole). */
function chipRound(value: number): number {
  const digits = Math.floor(Math.log10(Math.max(1, value)));
  const step = digits < 2 ? 1 : 10 ** (digits - 1);
  return Math.max(1, Math.round(value / step) * step);
}

/** A custom item's name from what the player wrote: at most 32 code points, cut at a word. */
export function cutName(text: string): string {
  const chars = [...text.replaceAll("\n", " ")];
  if (chars.length <= CUSTOM_NAME_MAX) return chars.join("").trim();
  let cut = chars.slice(0, CUSTOM_NAME_MAX).join("");
  const space = cut.lastIndexOf(" ");
  if (chars[CUSTOM_NAME_MAX] !== " " && space > 0) cut = cut.slice(0, space);
  return cut.trim();
}

/** The item whose conditions `req` holds: the item when buying, the paying item when selling. */
export function receivedOf(draft: Pick<OrderDraft, "side" | "item" | "pay">): ItemRef | null {
  if (draft.side === "buy") return draft.item;
  if (draft.side === "sell" && draft.pay !== null && draft.pay !== "money") return draft.pay;
  return null;
}

/**
 * The defaults a draft's known fields give (run after every change): one of an item that does
 * not stack; `total` for one item and for every buy; the target from the limit; conditions that
 * fit what the bot receives.
 */
export function fill(draft: OrderDraft, ctx: DraftContext): OrderDraft {
  const facts = refFacts(draft.item, ctx);
  if (draft.qty === null && facts !== null && facts.stack === 1) draft.qty = 1;
  if (draft.unit === null && (draft.qty === 1 || draft.side === "buy")) draft.unit = "total";
  if (draft.target === null && draft.limit !== null && draft.side !== null) {
    draft.target = targetFromLimit(draft.side, draft.limit, ctx.money, draft.amounts === "count");
  }
  if (draft.req !== null) {
    const received = receivedOf(draft);
    if (draft.side === "sell" && draft.pay === "money") {
      draft.req = null;
    } else if (received !== null) {
      draft.req.ench = fittingEnch(draft.req.ench, received, ctx.data);
    }
  }
  return draft;
}

/**
 * The enchantments that fit the item the bot receives: its group, no curse, none excluding one
 * kept before it, each within its most. A custom item's (the server's data) and any without the
 * item data stay as they are.
 */
export function fittingEnch(
  ench: Record<string, number>,
  received: ItemRef | null,
  data: ItemData | undefined,
): Record<string, number> {
  if (received === null || received.startsWith("custom:") || !data) return ench;
  const facts = itemFacts(data, received);
  if (facts === null || facts.ench === null) return ench;
  const kept: Record<string, number> = {};
  for (const [id, level] of Object.entries(ench)) {
    const info = data.enchantmentById.get(id);
    if (!info || info.curse || !facts.ench.includes(id)) continue;
    const excludes = info.excludes.map((i) => data.enchantments[i]?.id);
    if (Object.keys(kept).some((other) => excludes.includes(other))) continue;
    kept[id] = Math.min(level, info.maxLevel);
  }
  return kept;
}

/**
 * Whether the item the bot receives wears out (a custom item by its material); null while that
 * cannot be told (no item data, an item it lacks, a custom item without a known material).
 */
export function receivedWears(ref: ItemRef | null, ctx: DraftContext): boolean | null {
  if (ref === null || !ctx.data) return null;
  const id = ref.startsWith("custom:")
    ? (ctx.custom.find((item) => `custom:${item.k}` === ref)?.base ?? null)
    : ref;
  const item = id === null ? undefined : ctx.data.byId.get(id);
  return item ? item.maxDamage > 0 : null;
}

function ownerTrades(draft: OrderDraft, ctx: DraftContext): boolean {
  return ctx.trades === "owner" || draft.trade === "owner";
}

function customChips(
  draft: OrderDraft,
  field: "item" | "pay",
  ctx: DraftContext,
): ("pick" | "first_seen" | "fix")[] {
  if (!ctx.features.customItems) return ["fix"];
  const receives =
    (field === "item" && draft.side === "buy") || (field === "pay" && draft.side === "sell");
  return receives ? ["pick", "first_seen", "fix"] : ["pick", "fix"];
}

function limitChips(draft: OrderDraft, ctx: DraftContext, counts: boolean): number[] {
  if (draft.target === null || draft.side === null) return [];
  const target = draft.target;
  const factors = draft.side === "buy" ? [1.1, 1.25, 1.5] : [0.9, 0.8, 0.7];
  const top = counts ? MAX_COUNT : ctx.money.maxDeal;
  const out: number[] = [];
  for (const factor of factors) {
    const value = counts ? Math.round(target * factor) : chipRound(target * factor);
    const fits = draft.side === "buy" ? value > target : value < target;
    if (fits && value >= 1 && value <= top && !out.includes(value)) out.push(value);
  }
  return out;
}

/** The questions a draft still has, in order; the card shows the first two. */
export function questionsOf(draft: OrderDraft, ctx: DraftContext): OrderQuestion[] {
  const out: OrderQuestion[] = [];
  for (const field of ["item", "pay"] as const) {
    const open = draft.open[field];
    if (draft[field] === null && open && "options" in open) {
      out.push({ kind: "which", field, options: open.options });
    }
  }
  if (draft.side === null) out.push({ kind: "side" });
  if (draft.side !== null) {
    for (const field of ["item", "pay"] as const) {
      const open = draft.open[field];
      if (draft[field] === null && open && "mention" in open) {
        out.push({
          kind: "custom",
          field,
          mention: open.mention,
          chips: customChips(draft, field, ctx),
        });
      }
    }
  }
  if (draft.limit === null && draft.side !== null) {
    out.push({
      kind: "limit",
      side: draft.side,
      amounts: limitChips(draft, ctx, draft.amounts === "count"),
    });
  }
  const either = draft.open.pay && "either" in draft.open.pay ? draft.open.pay.either : undefined;
  if (draft.pay === null && either !== undefined) out.push({ kind: "pay", item: either });
  const setting = ctx.pay ?? "money";
  if (draft.pay !== null && draft.pay !== "money" && setting === "money") {
    out.push({ kind: "pay_off", want: "item" });
  } else if (draft.pay === "money" && setting === "items") {
    out.push({ kind: "pay_off", want: "money" });
  }
  const price = draft.target ?? draft.limit;
  if (
    draft.side === "sell" &&
    draft.qty !== null &&
    draft.qty > 1 &&
    draft.unit === null &&
    price !== null
  ) {
    out.push({ kind: "per", price });
  }
  const facts = refFacts(draft.item, ctx);
  if (draft.qty === null && facts !== null && facts.stack > 1) {
    out.push({
      kind: "qty",
      amounts: [...new Set([1, Math.floor(facts.stack / 2), facts.stack])],
    });
  }
  if (
    draft.side === "buy" &&
    draft.target !== null &&
    draft.limit !== null &&
    draft.limit > 10 * draft.target &&
    !draft.ok.includes("far")
  ) {
    out.push({ kind: "far", limit: draft.limit, target: draft.target });
  }
  const cap = ctx.limits.perDealMax ?? null;
  if (
    draft.side === "buy" &&
    draft.pay === "money" &&
    cap !== null &&
    draft.limit !== null &&
    draft.limit > cap &&
    !ownerTrades(draft, ctx)
  ) {
    out.push({ kind: "over_cap", limit: draft.limit, cap });
  }
  return out;
}

function withSettings(
  ctx: DraftContext,
  settings: { pay?: "both"; limits?: { perDealMax: number } },
): DraftContext {
  const next = { ...ctx };
  if (settings.pay !== undefined) next.pay = settings.pay;
  if (settings.limits !== undefined) next.limits = { ...ctx.limits, ...settings.limits };
  return next;
}

/**
 * A chip's effect: the new draft (and a settings change saved with the order in the same write),
 * or the form at a field, or the inventory picker. `newKey`: the key for a new custom item.
 */
export function answer(
  draft: OrderDraft,
  question: OrderQuestion,
  choice: Choice,
  ctx: DraftContext,
  newKey = "c1",
): AnswerResult {
  const next: OrderDraft = structuredClone(draft);
  let settings: { pay?: "both"; limits?: { perDealMax: number } } | undefined;
  switch (question.kind) {
    case "which":
      next[question.field] = (choice as { option: ItemRef }).option;
      delete next.open[question.field];
      break;
    case "side":
      next.side = choice as Side;
      break;
    case "custom":
      if (choice === "fix") return { form: question.field };
      if (choice === "pick") return { pick: question.field };
      next[question.field] = `custom:${newKey}`;
      delete next.open[question.field];
      next.customItems = [
        ...(next.customItems ?? []),
        {
          k: newKey,
          name: cutName(question.mention),
          base: null,
          fp: null,
          look: null,
          from: "text",
        },
      ];
      break;
    case "limit":
      next.limit = (choice as { amount: number }).amount;
      if (
        next.target !== null &&
        ((next.side === "buy" && next.target > next.limit) ||
          (next.side === "sell" && next.target < next.limit))
      ) {
        next.target = null;
      }
      break;
    case "pay":
      next.pay = choice === "money" ? "money" : question.item;
      delete next.open.pay;
      break;
    case "pay_off":
      if (choice === "turn_on") settings = { pay: "both" };
      else if (question.want === "item") next.pay = "money";
      else return { form: "pay" };
      break;
    case "per":
      next.unit = choice as "each" | "total";
      break;
    case "qty":
      next.qty = (choice as { amount: number }).amount;
      break;
    case "far":
      if (choice === "fix") return { form: "limit" };
      next.ok = [...next.ok, "far"];
      break;
    case "over_cap":
      if (choice === "raise") {
        settings = { limits: { perDealMax: question.limit } };
      } else {
        next.limit = question.cap;
        if (next.target !== null && next.target > question.cap) next.target = question.cap;
      }
      break;
  }
  if (next.pay !== null) {
    const amounts = next.pay === "money" ? "money" : "count";
    if (amounts !== next.amounts) {
      // Money and a paying item's count are different units: the amounts start again.
      next.target = null;
      next.limit = null;
      next.amounts = amounts;
    }
  }
  const after = settings === undefined ? ctx : withSettings(ctx, settings);
  fill(next, after);
  return settings === undefined ? { draft: next } : { draft: next, settings };
}

/**
 * The order a finished draft saves as (without `id` and `at`), or null while questions remain.
 * `req.dur` null becomes the default: 90 for an item that wears out, else 0. Without the item data
 * (or a custom item's material) it stays as the draft has it, 90 when none: a durability lost on
 * the way would buy a worn item, while one that does not fit is dropped by the agent.
 */
export function orderOf(draft: OrderDraft, ctx: DraftContext): NewOrder | null {
  if (questionsOf(draft, ctx).length > 0) return null;
  if (
    draft.side === null ||
    draft.item === null ||
    draft.qty === null ||
    draft.pay === null ||
    draft.unit === null ||
    draft.target === null ||
    draft.limit === null
  ) {
    return null;
  }
  let req: NewOrder["req"] = null;
  if (draft.req !== null) {
    const wears = receivedWears(receivedOf(draft), ctx);
    req = {
      ench: draft.req.ench,
      dur: wears === false ? 0 : (draft.req.dur ?? 90),
      named: draft.req.named,
      cursed: draft.req.cursed,
    };
  }
  return {
    on: true,
    side: draft.side,
    item: draft.item,
    qty: draft.qty,
    req,
    pay: draft.pay,
    unit: draft.unit,
    target: draft.target,
    limit: draft.limit,
    style: draft.style,
    trade: draft.trade,
    until: draft.until,
    note: draft.note,
    src: draft.src,
  };
}

// --- status -----------------------------------------------------------------------------------------

export type OrderStatus =
  | "waiting_ok"
  | "your_trade"
  | "trading"
  | "done"
  | "expired"
  | "paused"
  | "side_off"
  | "haggling"
  | "looking";

/**
 * An order's status: what needs the player first (an approval or a first sight, a trade the owner
 * makes), then a trade in its window, then finished (done before expired), then stopped (paused,
 * the side switched off), then the bargaining or looking.
 */
export function statusOf(
  order: { side: Side; qty: number; until: number; on: boolean },
  state: { now: number; got: number; live: string | null; allow: { buy: boolean; sell: boolean } },
): OrderStatus {
  if (state.live === "waiting_ok" || state.live === "your_trade" || state.live === "trading") {
    return state.live;
  }
  if (state.got >= order.qty) return "done";
  if (state.now >= order.until) return "expired";
  if (!order.on) return "paused";
  if (!state.allow[order.side]) return "side_off";
  return state.live === "haggling" ? "haggling" : "looking";
}

/** Finished orders go under "Biten". */
export function isFinished(status: OrderStatus): boolean {
  return status === "done" || status === "expired";
}
