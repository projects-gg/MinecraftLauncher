// Which AFK accounts just stopped for good, and the notice that says so. An account in `error`
// stopped and will not try again by itself (a reconnect is `reconnecting`, a stop the player asked
// for is `stopped`), so only the step into `error` is news: the first read, an account that stays
// in error and every reconnect say nothing (docs/tasks/q2.md §3, "Yapma listesi").
import type { TFunction } from "i18next";
import type { AfkAccount, AfkState } from "@/lib/api";

/** An account that just stopped for good. */
export interface AfkStop {
  id: string;
  name: string;
  /** The short Turkish reason, without the server's own message. */
  reason: string;
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

/** Accounts in `error` now that were not at the last read (`before` is null on the first read). */
export function findStops(before: AfkSeen | null, accounts: readonly AfkAccount[]): AfkStop[] {
  if (!before) return [];
  const stops: AfkStop[] = [];
  for (const account of accounts) {
    if (account.state.kind === "error" && before.get(account.id) !== "error") {
      stops.push({
        id: account.id,
        name: account.nickname,
        reason: shortReason(account.state.reason),
      });
    }
  }
  return stops;
}

/**
 * One notice for the stops found together: the name and the reason, or how many and who (with
 * the reason when they share it, as when a server turns on its whitelist).
 */
export function stopNotice(
  stops: readonly [AfkStop, ...AfkStop[]],
  t: TFunction,
): { title: string; body: string } {
  const [first] = stops;
  if (stops.length === 1) {
    return { title: t("afkAlerts.stopped", { name: first.name }), body: first.reason };
  }
  const names = stops.map((stop) => stop.name).join(", ");
  const shared = stops.every((stop) => stop.reason === first.reason);
  return {
    title: t("afkAlerts.stoppedMany", { count: stops.length }),
    body: shared ? t("afkAlerts.sameReason", { reason: first.reason, names }) : names,
  };
}
