// "Ekibi çağır", pure rules (calls.test.ts): which calls are still on, the minutes they have left,
// when the next minute (or end) comes, how the answers add up and when the player may call again.
// Calls end by themselves at `endsAt` and no message says so: the launcher hides them on its own
// clock, with one timer to the next change while a call is on and none otherwise.
import type { CallAnswer, CrewCall, SocialTeam } from "@/lib/api/social-types";

/** A player calls at most every 5 minutes (the service says so too). */
export const CALL_GAP_MS = 5 * 60_000;

/** Crews of the player's a call names, in the player's order. A call from a service with one
 * crew per player names none: it is the (only) primary crew's. */
export function crewsOfCall<T extends Pick<SocialTeam, "id">>(
  call: Pick<CrewCall, "crews">,
  crews: readonly T[],
): T[] {
  if (!call.crews.length) return crews.slice(0, 1);
  return crews.filter((crew) => call.crews.includes(crew.id));
}

/** A call that names this crew (see `crewsOfCall`). */
export function callNames(
  call: Pick<CrewCall, "crews">,
  crew: Pick<SocialTeam, "id">,
  crews: readonly Pick<SocialTeam, "id">[],
): boolean {
  return crewsOfCall(call, crews).some((named) => named.id === crew.id);
}

/** Calls still on at `nowMs`. */
export function liveCalls(calls: readonly CrewCall[], nowMs: number): CrewCall[] {
  return calls.filter((call) => call.endsAt * 1000 > nowMs);
}

/** "17 dk": whole minutes left, rounded up (never 0 while the call is on). */
export function minutesLeft(call: Pick<CrewCall, "endsAt">, nowMs: number): number {
  return Math.max(1, Math.ceil((call.endsAt * 1000 - nowMs) / 60_000));
}

/**
 * How long until the minutes of some call change or a call ends (ms), or null without a call: the
 * only timer the calls need, and only while one is on.
 */
export function nextCallTick(calls: readonly CrewCall[], nowMs: number): number | null {
  let next: number | null = null;
  for (const call of calls) {
    const left = call.endsAt * 1000 - nowMs;
    if (left <= 0) continue;
    const step = left % 60_000 || 60_000;
    next = next === null ? step : Math.min(next, step);
  }
  return next;
}

/** The player's own call, if one is on. */
export function myCall(calls: readonly CrewCall[], nowMs: number): CrewCall | undefined {
  return liveCalls(calls, nowMs).find((call) => call.mine);
}

/** Teammates' calls that are on, newest first (blocks can leave two open at once). */
export function theirCalls(calls: readonly CrewCall[], nowMs: number): CrewCall[] {
  return liveCalls(calls, nowMs)
    .filter((call) => !call.mine)
    .sort((a, b) => b.startedAt - a.startedAt);
}

/** A teammate's call the player has not answered (nor closed here). */
export function unansweredCall(
  calls: readonly CrewCall[],
  nowMs: number,
  closed: readonly string[] = [],
): CrewCall | undefined {
  return theirCalls(calls, nowMs).find((call) => !call.myAnswer && !closed.includes(call.id));
}

/** Who said what, in the crew's order. */
export function answersBy(call: Pick<CrewCall, "answers">): Record<CallAnswer, string[]> {
  const out: Record<CallAnswer, string[]> = { yes: [], soon: [], no: [] };
  for (const { id, answer } of call.answers) out[answer].push(id);
  return out;
}

/**
 * Until when "Ekibi çağır" waits (ms): five minutes after the player's last call started here,
 * or what the service said after a refused one, whichever is later. 0: it does not wait.
 */
export function callWaitUntil(
  calls: readonly CrewCall[],
  refusedUntil: number,
  lastCallAt: number | null,
): number {
  const own = calls.find((call) => call.mine);
  const started = Math.max(own ? own.startedAt * 1000 : 0, lastCallAt ?? 0);
  return Math.max(refusedUntil, started ? started + CALL_GAP_MS : 0);
}

/** "4 dk sonra yeniden çağırabilirsin": whole minutes, rounded up. */
export function waitMinutes(untilMs: number, nowMs: number): number {
  return Math.max(1, Math.ceil((untilMs - nowMs) / 60_000));
}
