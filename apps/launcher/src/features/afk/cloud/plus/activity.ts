// "Etkinlik" in plain functions (PLUS-ACTIVITY; product.md → Denetim kaydı): the audit log's steps as
// rows (a player's day together, an event without a player alone), the step that says how a row
// went, the filters' groups, the search, who waits for the player's answer and today's trades. The
// site (afk-web audit-section.tsx) and the phone group the same entries the same way; no new read.
import { fold } from "@pl/ui";
import { dayGroup } from "./time";
import type { PlusAuditEntry, PlusInboxThread } from "./types";

export type AuditFilter = "all" | "deals" | "messages" | "warnings";

const DEALS = new Set([
  "offer",
  "counter",
  "agree",
  "ask_owner",
  "owner_ok",
  "owner_no",
  "owner_timeout",
  "trade_start",
  "trade_done",
  "trade_fail",
  "handoff",
  "handoff_done",
  "handoff_no",
  "cancel",
  "found",
  "confirm_item",
  "order_done",
  "order_expired",
  // DONUT-ORDER: a delivery to the server's order menu, and one that did not happen.
  "menu_done",
  "menu_fail",
]);
const MESSAGES = new Set(["pm_in", "reply", "owner_msg"]);
// "Retler": what was turned down, by a rule, the other player, the trade or the player (the contract's
// groups as the site filters them; an entry may be in two).
const WARNINGS = new Set([
  "refuse",
  "gate",
  "cancel",
  "trade_fail",
  "owner_no",
  "owner_timeout",
  "handoff_no",
  "menu_fail",
]);

/** Whether a step shows under a filter. */
export function inFilter(kind: string, filter: AuditFilter): boolean {
  if (filter === "deals") return DEALS.has(kind);
  if (filter === "messages") return MESSAGES.has(kind);
  if (filter === "warnings") return WARNINGS.has(kind);
  return true;
}

/** A conversation's line in the log: what a player wrote, ProBot AI's or the player's own answer. */
export function isMessage(kind: string): boolean {
  return MESSAGES.has(kind);
}

/** The kinds the log knows; a newer one reads as a plain step. */
export const KINDS: ReadonlySet<string> = new Set([
  ...DEALS,
  ...MESSAGES,
  ...WARNINGS,
  "quota",
  "paused",
  "resumed",
  // The chat ads stopped for the day (a spam warning, trade-any-4).
  "ads_stopped",
]);

/** A player's day in one row, or one event without a player alone: its entries newest first. */
export interface ActivityGroup {
  key: string;
  who: string | null;
  entries: PlusAuditEntry[];
}

/** A day's entries (newest first) as rows: a player's together (names as the game compares them). */
export function activityGroups(entries: readonly PlusAuditEntry[]): ActivityGroup[] {
  const byWho = new Map<string, ActivityGroup>();
  const out: ActivityGroup[] = [];
  for (const entry of entries) {
    if (!entry.w) {
      out.push({ key: `s${entry.s}`, who: null, entries: [entry] });
      continue;
    }
    const key = `w:${entry.w.toLowerCase()}`;
    let group = byWho.get(key);
    if (!group) {
      group = { key, who: entry.w, entries: [] };
      byWho.set(key, group);
      out.push(group);
    }
    group.entries.push(entry);
  }
  return out;
}

/**
 * The kinds that say how a player's day went, the most telling first; kinds on one rank are equal
 * and the newest of them wins. The bargain's last word is one rank, whoever said it (the site's,
 * TASTE-SWEEP r1: the seller's earlier ask outranked ProBot AI's later offer); a delivery to the
 * order menu ranks with a trade.
 */
const OUTCOME: ReadonlyArray<readonly string[]> = [
  ["trade_done", "menu_done"],
  ["trade_fail", "menu_fail"],
  ["handoff_done"],
  ["handoff_no"],
  ["owner_ok"],
  ["owner_no"],
  ["owner_timeout"],
  ["handoff"],
  ["ask_owner"],
  ["confirm_item"],
  ["agree"],
  ["gate"],
  ["refuse"],
  ["cancel"],
  ["found"],
  ["counter", "offer"],
  ["order_done"],
  ["order_expired"],
];
const OUTCOME_RANK = new Map(OUTCOME.flatMap((kinds, at) => kinds.map((k) => [k, at] as const)));

/** What a row says: the most telling step of its day (the newest of those), else its newest. */
export function outcomeOf(group: ActivityGroup): PlusAuditEntry {
  let best: PlusAuditEntry | null = null;
  let rank = OUTCOME.length;
  // Newest first: a later entry of the same rank never takes the place of a newer one.
  for (const entry of group.entries) {
    const at = OUTCOME_RANK.get(entry.k);
    if (at !== undefined && at < rank) {
      best = entry;
      rank = at;
    }
  }
  return best ?? (group.entries[0] as PlusAuditEntry);
}

/**
 * A row the search finds: by the player's name or an item's, in either language or by its aliases
 * ("elytra" finds the game's Turkish "Elitra"), folded as players type (Turkish lower case, "ı" as
 * "i", no marks: "zumrut" finds "Zümrüt"). An empty search finds every row.
 */
export function rowFound(
  group: ActivityGroup,
  query: string,
  itemNames: (ref: string) => readonly string[],
): boolean {
  const needle = fold(query.trim());
  if (!needle) return true;
  if (group.who && fold(group.who).includes(needle)) return true;
  return group.entries.some(
    (entry) => entry.i && itemNames(entry.i).some((name) => fold(name).includes(needle)),
  );
}

/**
 * The conversations that wait for the player's answer: a player wrote last and it is unread (a
 * blocked one never waits). "Seni bekleyenler" lists them beside the approvals; the rail counts them.
 */
export function waitingThreads(threads: readonly PlusInboxThread[]): PlusInboxThread[] {
  return threads.filter((th) => th.unread > 0 && th.state !== "blocked" && th.last.from === "them");
}

/** Trades finished today (the bot's or the player's handoff), for the slim line over the list. */
export function tradesToday(entries: readonly PlusAuditEntry[], nowMs: number): number {
  return entries.filter(
    (entry) =>
      (entry.k === "trade_done" || entry.k === "handoff_done") &&
      dayGroup(Math.floor(entry.t / 1000), nowMs) === "today",
  ).length;
}
