// Game accounts and AFK sessions share player names: the server accepts one connection per
// name, so a game account whose name has an AFK session running would knock it off (or be
// knocked off). Names match ignoring case, as on the server.

import type { AfkAccount, AfkState } from "@/lib/api";

/** Connected to the server right now. */
export function isAfkOnline(state: AfkState): boolean {
  return state.kind === "connected";
}

/** Holds its name on the server or is about to: connected, connecting, queued, reconnecting. */
export function isAfkRunning(state: AfkState): boolean {
  return state.kind !== "stopped" && state.kind !== "stopping" && state.kind !== "error";
}

export function sameName(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

/** The AFK session with this player name that passes `test` (running, by default). */
export function findAfkSession(
  username: string,
  sessions: readonly AfkAccount[],
  test: (state: AfkState) => boolean = isAfkRunning,
): AfkAccount | null {
  return sessions.find((s) => sameName(s.nickname, username) && test(s.state)) ?? null;
}

/** For the "AFK'da" chip: the session of this name that is connected right now. */
export function onlineAfkSession(
  username: string,
  sessions: readonly AfkAccount[],
): AfkAccount | null {
  return findAfkSession(username, sessions, isAfkOnline);
}

/** For the account center row: how many AFK accounts there are and how many are online. */
export function afkSummary(sessions: readonly AfkAccount[]): { total: number; online: number } {
  return {
    total: sessions.length,
    online: sessions.filter((s) => isAfkOnline(s.state)).length,
  };
}
