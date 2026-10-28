// 7/24 Bulut while a machine cannot be reached (CLOUD-STALE, 28 Oct 2026; QA-FIX-L's second look
// found the cards, the console's title and their counters reading live then). A machine is out of
// reach when a read could not ask it (`live.state` "unknown") or a live socket said so (its machine
// went away, the socket itself stayed down past a moment, a refusal named it). Each account keeps
// its last live state known to be true and when, so the page can say what it knew, dim and still,
// never as live. Nothing asks for any of it: the overview reads (the page opening, an action,
// "Yenile") and the sockets that already run (a console, AFK+) bring it, and a machine a socket
// sees again reads the overview once (use-cloud.ts), so the page turns live again by itself.
import type { TFunction } from "i18next";
import { create } from "zustand";
import { timeLocative } from "@/lib/event-card";
import { dateTimeFormat } from "@/lib/format";
import type { CloudAccount, CloudPageData } from "./api";

type LiveState = CloudAccount["live"];

/** A live state known to be true, and when (this computer's clock, ms). */
export interface KnownLive {
  live: LiveState;
  at: number;
}

export interface ReachState {
  /** Machines out of reach now, and since when (ms). */
  away: Readonly<Record<string, number>>;
  /** Each account's last live state known to be true. */
  known: Readonly<Record<string, KnownLive>>;
}

export const NO_REACH: ReachState = { away: {}, known: {} };

/** What may be said of an account's live state now: its own, or the last one known (null: none). */
export type AccountReach = { stale: false } | { stale: true; known: KnownLive | null };

export const LIVE_REACH: AccountReach = { stale: false };

/** A machine that could not be asked says "unknown". */
const isKnown = (live: LiveState) => live.state !== "unknown";

function without(record: Readonly<Record<string, number>>, key: string) {
  const { [key]: _gone, ...rest } = record;
  return rest;
}

/**
 * An overview read that started at `at`: the live states it knows are the latest, and a machine it
 * could ask is back, unless a socket said it went away once the read was under way. Signed out,
 * nothing of before stays; a service that did not answer changes nothing.
 */
export function withOverview(state: ReachState, data: CloudPageData, at: number): ReachState {
  const { overview } = data;
  if (!overview.available || overview.session === "failed") return state;
  if (overview.session === "signedOut") {
    return state.known === NO_REACH.known && state.away === NO_REACH.away ? state : NO_REACH;
  }
  const answered = new Set<string>();
  const fresh: Record<string, KnownLive> = {};
  for (const account of overview.accounts) {
    if (!isKnown(account.live)) continue;
    if (account.host) answered.add(account.host);
    fresh[account.id] = { live: account.live, at };
  }
  let away = state.away;
  for (const host of answered) {
    const since = away[host];
    if (since !== undefined && since < at) away = without(away, host);
  }
  const known = Object.keys(fresh).length ? { ...state.known, ...fresh } : state.known;
  return known === state.known && away === state.away ? state : { away, known };
}

/** A live state the service gave at `at` (a socket's stream, an action's answer). */
export function withLive(state: ReachState, id: string, live: LiveState, at: number): ReachState {
  if (!isKnown(live)) return state;
  return { ...state, known: { ...state.known, [id]: { live, at } } };
}

/**
 * A machine out of reach from `at` (the first time is kept). `watched`: the account a socket
 * streamed until then, whose state was known up to that moment.
 */
export function withAway(
  state: ReachState,
  host: string,
  at: number,
  watched: { id: string; live: LiveState } | null = null,
): ReachState {
  const away = state.away[host] === undefined ? { ...state.away, [host]: at } : state.away;
  const known =
    watched && isKnown(watched.live)
      ? { ...state.known, [watched.id]: { live: watched.live, at } }
      : state.known;
  return away === state.away && known === state.known ? state : { away, known };
}

/** A socket sees the machine again. */
export function withBack(state: ReachState, host: string): ReachState {
  return state.away[host] === undefined ? state : { ...state, away: without(state.away, host) };
}

/**
 * What may be said of the account's live state now. A hold or an expired account says the
 * service's own words, which never need the machine, and so does one the player stopped that was
 * stopped (nothing runs it, whatever the machine does).
 */
export function reachOf(account: CloudAccount, state: ReachState): AccountReach {
  if (account.hold || account.expired) return LIVE_REACH;
  if (account.desired !== "run" && account.live.state === "stopped") return LIVE_REACH;
  const away = account.host !== null && state.away[account.host] !== undefined;
  if (isKnown(account.live) && !away) return LIVE_REACH;
  return { stale: true, known: state.known[account.id] ?? null };
}

/** The live states a last known line can name; the waits between attempts read as connecting. */
const LAST_KIND: Record<string, "running" | "connecting" | "queued" | "stopped"> = {
  running: "running",
  connecting: "connecting",
  waiting: "connecting",
  queued: "queued",
  stopped: "stopped",
};

/**
 * A last known state in words, in the past with its clock time ("En son 14:05'te oyundaydı";
 * another day's with its date, "27 Ekim 14:05'te"); with nothing known, "Durumu bilinmiyor".
 */
export function lastKnownText(known: KnownLive | null, t: TFunction, nowMs = Date.now()): string {
  const kind = known ? LAST_KIND[known.live.state] : undefined;
  if (!known || !kind) return t("cloudStale.unknown");
  const at = new Date(known.at);
  const clock = dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(at);
  const today = at.toDateString() === new Date(nowMs).toDateString();
  const time = today
    ? clock
    : `${dateTimeFormat({ day: "numeric", month: "long" }).format(at)} ${clock}`;
  return t(`cloudStale.last.${kind}`, {
    time,
    ending: timeLocative(at.getHours(), at.getMinutes()),
  });
}

/** The session's reach: written by the reads and the sockets, read by the page's lines. */
export const useCloudReach = create<ReachState>()(() => NO_REACH);

export function updateReach(update: (state: ReachState) => ReachState) {
  useCloudReach.setState((state) => update(state), true);
}

/** The account's reach (re-renders only when its machine's or its own entry changes). */
export function useAccountReach(account: CloudAccount | undefined): AccountReach {
  const host = account?.host ?? null;
  const away = useCloudReach((s) => (host === null ? undefined : s.away[host]));
  const known = useCloudReach((s) => (account ? s.known[account.id] : undefined));
  if (!account) return LIVE_REACH;
  return reachOf(account, {
    away: host === null || away === undefined ? {} : { [host]: away },
    known: known ? { [account.id]: known } : {},
  });
}
