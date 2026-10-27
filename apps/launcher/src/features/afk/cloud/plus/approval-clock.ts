// What waits for the player and what closed lately ("Seni bekleyenler", "Kapananlar"), and the
// moment both are read at: read again only when one of them changes (an approval's time runs out,
// a closed one leaves its ten minutes), with one timer for the soonest, never a tick.
import { useEffect, useState } from "react";
import type { PlusApproval } from "./api";

/** How long a closed approval stays under "Kapananlar" (seconds). */
const RECENT = 600;

/** When the player answered an approval in this launcher (ms): its ten minutes start then. */
const answeredHere = new Map<string, number>();

/** An approval the player answered here a moment ago. */
export function markAnswered(id: string): void {
  answeredHere.set(id, Date.now());
}

/** Still waiting: pending and its time not run out. */
export function waitsNow(approval: PlusApproval, nowSeconds: number): boolean {
  return approval.state === "pending" && approval.expiresAt > nowSeconds;
}

/**
 * Until when a closed approval shows under "Kapananlar" (Unix seconds); null while it waits. The
 * service does not say when one was answered, so: ten minutes from an answer made here, from its
 * question while that is new, from its time running out; one the service closed with time left (the
 * assistant stopped, maintenance, the offer gone) until ten minutes after that time. One answered
 * elsewhere with time left shows only while its question is new.
 */
export function closedUntil(approval: PlusApproval, nowSeconds: number): number | null {
  if (waitsNow(approval, nowSeconds)) return null;
  const here = answeredHere.get(approval.id);
  if (here !== undefined) return Math.floor(here / 1000) + RECENT;
  const asked = approval.createdAt + RECENT;
  if (approval.expiresAt <= nowSeconds || approval.state === "closed") {
    return Math.max(asked, approval.expiresAt + RECENT);
  }
  return asked;
}

/** Closed in the last ten minutes (as far as this launcher can tell). */
export function closedLately(approval: PlusApproval, nowSeconds: number): boolean {
  const until = closedUntil(approval, nowSeconds);
  return until !== null && until > nowSeconds;
}

/**
 * Now (Unix seconds), read again at the next moment an approval's time runs out or a closed one
 * leaves "Kapananlar": the feed, the rows and the rail count what waits with it.
 */
export function useApprovalNow(approvals: readonly PlusApproval[]): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    let next: number | null = null;
    for (const approval of approvals) {
      const at = waitsNow(approval, now) ? approval.expiresAt : closedUntil(approval, now);
      if (at !== null && at > now && (next === null || at < next)) next = at;
    }
    if (next === null) return;
    const wait = Math.max(0, next * 1000 - Date.now()) + 50;
    const timer = window.setTimeout(() => setNow(Math.floor(Date.now() / 1000)), wait);
    return () => window.clearTimeout(timer);
  }, [approvals, now]);
  return now;
}
