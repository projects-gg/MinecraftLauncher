// Which AFK accounts just stopped for good, and the notice that says so. An account in `error`
// stopped and will not try again by itself (a reconnect is `reconnecting`, a stop the player asked
// for is `stopped`), so only the step into `error` is news: the first read, an account that stays
// in error and every reconnect say nothing (docs/tasks/q2.md §3, "Yapma listesi"). The step into
// `clientFailed` is news too: the account never got to connect, because its client could not be
// had (an auto-start in the tray, a connect whose download failed while the player was elsewhere).
import type { TFunction } from "i18next";
import type { AfkAccount, AfkClientProblem, AfkState } from "@/lib/api";

/** An account that just stopped for good, or whose client could not be had. */
export interface AfkStop {
  id: string;
  name: string;
  /** The short Turkish reason, without the server's own message (empty for `problem`). */
  reason: string;
  /** Set when the account never got to connect: its client could not be had. */
  problem?: AfkClientProblem;
}

/** Each account's state at the last read. */
export type AfkSeen = ReadonlyMap<string, AfkState["kind"]>;

export function seenOf(accounts: readonly AfkAccount[]): AfkSeen {
  return new Map(accounts.map((account) => [account.id, account.state.kind]));
}

/**
 * The reason without the server's message: the API layer writes "Sunucudan yasaklandı (You are
 * banned …)", a notice says "Sunucudan yasaklandı" (the AFK page shows the whole line).
 */
export function shortReason(reason: string): string {
  const cut = reason.indexOf(" (");
  return (cut > 0 ? reason.slice(0, cut) : reason).trim();
}

/**
 * Accounts in `error` or `clientFailed` now that were not at the last read (`before` is null on
 * the first read).
 */
export function findStops(before: AfkSeen | null, accounts: readonly AfkAccount[]): AfkStop[] {
  if (!before) return [];
  const stops: AfkStop[] = [];
  for (const account of accounts) {
    const { state } = account;
    if (before.get(account.id) === state.kind) continue;
    if (state.kind === "error") {
      stops.push({ id: account.id, name: account.nickname, reason: shortReason(state.reason) });
    } else if (state.kind === "clientFailed") {
      stops.push({ id: account.id, name: account.nickname, reason: "", problem: state.problem });
    }
  }
  return stops;
}

/** What happened to the client and what to do, in one body ("… indirilemedi. İnternet …"). */
function problemBody(problem: AfkClientProblem, t: TFunction): string {
  return `${t(`afkConnect.problem.${problem}.title`)}. ${t(`afkConnect.problem.${problem}.hint`)}`;
}

/**
 * One notice for the stops found together: the name and the reason, or how many and who (with
 * the reason when they share it, as when a server turns on its whitelist). Accounts whose client
 * could not be had say they could not connect, and what to do.
 */
export function stopNotice(
  stops: readonly [AfkStop, ...AfkStop[]],
  t: TFunction,
): { title: string; body: string } {
  const [first] = stops;
  const names = stops.map((stop) => stop.name).join(", ");
  if (stops.every((stop) => stop.problem)) {
    const title =
      stops.length === 1
        ? t("afkConnect.notice.failed", { name: first.name })
        : t("afkConnect.notice.failedMany", { count: stops.length });
    const shared = first.problem && stops.every((stop) => stop.problem === first.problem);
    return { title, body: shared && first.problem ? problemBody(first.problem, t) : names };
  }
  if (stops.length === 1) {
    return { title: t("afkAlerts.stopped", { name: first.name }), body: first.reason };
  }
  const shared = stops.every((stop) => stop.reason === first.reason && !stop.problem);
  return {
    title: t("afkAlerts.stoppedMany", { count: stops.length }),
    body: shared ? t("afkAlerts.sameReason", { reason: first.reason, names }) : names,
  };
}
