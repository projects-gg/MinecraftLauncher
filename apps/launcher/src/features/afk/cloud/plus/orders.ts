// AFK+ orders as the panel shows them: the context a card's questions are asked against, amounts
// in the game's money (its own name after them when the server's profile gives one, else bare),
// an order's state and the keys new custom items get. Pure: orders.test.ts covers it.
import type { ItemData } from "@pl/ui/items";
import type {
  PlusConfig,
  PlusCustomItem,
  PlusOrder,
  PlusOrderView,
  PlusRules,
  PlusServer,
} from "./api";
import { type DraftContext, isFinished, type OrderStatus, statusOf } from "./order-draft";
import { formatAmount, type Lang } from "./words";

/** What a card's questions are asked against: the account's settings, server, rules and item data. */
export function draftContextOf(
  config: PlusConfig,
  server: PlusServer,
  rules: PlusRules,
  data: ItemData | undefined,
): DraftContext {
  return {
    data,
    custom: config.customItems,
    // The profile's rounding steps are the service's; the card rounds a target to 2 digits.
    money: { maxDeal: server.money.maxDeal, round: null },
    pay: (["money", "items", "both"] as const).find((p) => p === config.pay) ?? "money",
    features: { itemPay: rules.features.itemPay, customItems: rules.features.customItems },
    limits: { perDealMax: config.limits.perDealMax, totalBudget: config.limits.totalBudget },
    trades: server.trade === "owner" ? "owner" : "bot",
  };
}

/** The money's own name after an amount ("32.000 Coin"), or the bare amount. */
export function moneyText(value: number, server: Pick<PlusServer, "money">, lang: Lang): string {
  const label = server.money.label;
  const name = label ? (lang === "en" ? label.en : label.tr) : "";
  return name ? `${formatAmount(value, lang)} ${name}` : formatAmount(value, lang);
}

/** Whether amounts need "Oyundaki parayla" said once (the profile names no money). */
export function bareMoney(server: Pick<PlusServer, "money">): boolean {
  return !server.money.label;
}

/** Who makes an order's trade: the bot only where the server's profile lets it and the order allows. */
export function tradesItself(order: { trade: string }, server: Pick<PlusServer, "trade">): boolean {
  return order.trade !== "owner" && server.trade === "bot";
}

/** An order's state now: its progress, its live state and its status. */
export interface OrderState {
  got: number;
  at: number;
  live: string | null;
  status: OrderStatus;
}

export function orderState(
  order: PlusOrder,
  view: PlusOrderView | undefined,
  allow: { buy: boolean; sell: boolean },
  nowSeconds: number,
): OrderState {
  const got = view?.got ?? 0;
  const live = view?.live ?? null;
  const side = order.side === "sell" ? "sell" : "buy";
  return {
    got,
    at: view?.at ?? 0,
    live,
    status: statusOf(
      { side, qty: order.qty, until: order.until, on: order.on },
      { now: nowSeconds, got, live, allow },
    ),
  };
}

/**
 * Active orders first (what waits for the player first, then the newest), finished ones after.
 * `waiting`: the orders with an approval waiting under their row; the row says its state from that
 * (the machine's word can be older), so the order follows it too.
 */
export function sortedOrders(
  orders: readonly PlusOrder[],
  states: ReadonlyMap<string, OrderState>,
  waiting?: ReadonlySet<string | null>,
): { active: PlusOrder[]; finished: PlusOrder[] } {
  const rank: Record<OrderStatus, number> = {
    waiting_ok: 0,
    your_trade: 0,
    trading: 1,
    haggling: 2,
    looking: 3,
    side_off: 4,
    paused: 5,
    done: 6,
    expired: 6,
  };
  const status = (order: PlusOrder) => states.get(order.id)?.status ?? "looking";
  const rankOf = (order: PlusOrder) => {
    const own = status(order);
    if (!waiting || isFinished(own)) return rank[own];
    if (waiting.has(order.id)) return 0;
    return own === "waiting_ok" || own === "your_trade" ? rank.looking : rank[own];
  };
  const sorted = [...orders].sort(
    (a, b) => rankOf(a) - rankOf(b) || b.at - a.at || (a.id < b.id ? -1 : 1),
  );
  return {
    active: sorted.filter((order) => !isFinished(status(order))),
    finished: sorted.filter((order) => isFinished(status(order))),
  };
}

/** The active orders counted against Studio's cap (on, not finished). */
export function activeCount(
  orders: readonly PlusOrder[],
  states: ReadonlyMap<string, OrderState>,
): number {
  return orders.filter((order) => {
    const status = states.get(order.id)?.status ?? "looking";
    return order.on && !isFinished(status);
  }).length;
}

/** A key no custom item of the account (nor of the card) uses: `c1`, `c2`, … (`^c[0-9a-z]{1,4}$`). */
export function newCustomKey(taken: readonly Pick<PlusCustomItem, "k">[]): string {
  const used = new Set(taken.map((item) => item.k));
  for (let n = 1; n < 36 ** 4; n++) {
    const key = `c${n.toString(36)}`;
    if (!used.has(key)) return key;
  }
  return "c0";
}

/** Whole days from now until `until` (Unix seconds), rounded up; 0 once it passed. */
export function daysLeft(until: number, nowSeconds: number): number {
  return Math.max(0, Math.ceil((until - nowSeconds) / 86_400));
}
