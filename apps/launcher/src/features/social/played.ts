// How long a friend has been playing (CREW-PLUS, a player's suggestion): the line under their
// name says it coarsely ("Projects'te · 25 dk", "Oyunda · 1 sa"), as "son görülme" is coarse.
// The service sends when the game session began (down to five minutes) only from a friend who
// shares "last seen". Nothing under five minutes; under an hour in five-minute steps; then whole
// hours, always rounded down. The clock that redraws it (`usePlayedTicker`) wakes only at the next
// change, at most once a minute, and only while the friends panel is open and the window shows.
import type { TFunction } from "i18next";
import { useEffect } from "react";
import { create } from "zustand";
import { useShallow } from "zustand/react/shallow";
import type { Friend } from "@/lib/api/social-types";
import { useSocialUi } from "./store";

/** Under this long nothing is said (seconds). */
export const PLAYED_FROM = 300;
/** Under an hour the minutes go in steps of this (seconds). */
const STEP = 300;
const HOUR = 3_600;
/** The clock wakes at most this often (ms). */
export const PLAYED_TICK_MIN_MS = 60_000;

export type Played = { unit: "minutes" | "hours"; value: number };

/** How long a session that began at `since` (Unix seconds) has run at `nowMs`, coarsely. */
export function playedFor(since: number | undefined, nowMs: number): Played | null {
  if (since === undefined) return null;
  const elapsed = Math.floor(nowMs / 1000) - since;
  if (elapsed < PLAYED_FROM) return null;
  if (elapsed < HOUR) return { unit: "minutes", value: Math.floor(elapsed / STEP) * 5 };
  return { unit: "hours", value: Math.floor(elapsed / HOUR) };
}

/** The friend's session as the line shows it: only while they play. */
export function friendPlayed(
  friend: Pick<Friend, "state" | "playingSince">,
  nowMs: number,
): Played | null {
  return friend.state === "playing" ? playedFor(friend.playingSince, nowMs) : null;
}

/**
 * A crewmate's row says what their crew presence says; a crewmate who is a friend also carries how
 * long they have been playing (only friends get it from the service), so the same person reads the
 * same in the crew and in the friend list.
 */
export function withPlayed<T extends object>(
  member: T,
  friend: Pick<Friend, "playingSince"> | null | undefined,
): T & { playingSince?: number } {
  return friend?.playingSince === undefined
    ? member
    : { ...member, playingSince: friend.playingSince };
}

/** "25 dk", "1 sa". */
export function playedText(played: Played, t: TFunction): string {
  return t(`crewPlus.played.${played.unit}`, { count: played.value });
}

/** "25 dakikadır oyunda", for screen readers. */
export function playedSpoken(played: Played, t: TFunction): string {
  return t(`crewPlus.played.spoken.${played.unit}`, { count: played.value });
}

/** When (ms) the text of a session that began at `since` next changes after `nowMs`. */
export function nextPlayedChange(since: number, nowMs: number): number {
  const elapsed = Math.floor(nowMs / 1000) - since;
  if (elapsed < PLAYED_FROM) return (since + PLAYED_FROM) * 1000;
  const step = elapsed < HOUR ? STEP : HOUR;
  return (since + (Math.floor(elapsed / step) + 1) * step) * 1000;
}

/** How long until any of these sessions' texts changes (ms): at least a minute; null for none. */
export function nextPlayedTick(sinces: readonly number[], nowMs: number): number | null {
  if (!sinces.length) return null;
  const next = Math.min(...sinces.map((since) => nextPlayedChange(since, nowMs)));
  return Math.max(PLAYED_TICK_MIN_MS, next - nowMs);
}

/** The time the play lines are drawn at. */
export const usePlayedClock = create<{ now: number }>(() => ({ now: Date.now() }));

/** A little after the moment, so the minutes have really changed when the line redraws. */
const LATE_MS = 40;

/**
 * Mounted once, by SocialHost: one timer to the next change of any friend's line, only while the
 * friends panel is open and the window is on screen (none otherwise: no idle wake-ups).
 */
export function usePlayedTicker() {
  const open = useSocialUi((s) => s.open);
  const sinces = useSocialUi(
    useShallow((s) =>
      s.view.friends
        .filter((f) => f.state === "playing" && f.playingSince !== undefined)
        .map((f) => f.playingSince as number),
    ),
  );
  const key = sinces.join(",");
  useEffect(() => {
    if (!open || !key) return;
    const starts = key.split(",").map(Number);
    let timer: number | undefined;
    const tick = () => {
      window.clearTimeout(timer);
      timer = undefined;
      if (document.visibilityState !== "visible") return;
      const now = Date.now();
      usePlayedClock.setState({ now });
      const delay = nextPlayedTick(starts, now);
      if (delay !== null) timer = window.setTimeout(tick, delay + LATE_MS);
    };
    const onVisibility = () => {
      if (document.visibilityState === "visible") tick();
      else window.clearTimeout(timer);
    };
    tick();
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [open, key]);
}
