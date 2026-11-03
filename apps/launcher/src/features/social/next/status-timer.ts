// SOC-NEXT §3.3, pure rules (status-timer.test.ts): "Rahatsız etmeyin" and "Görünmez" for a while,
// with the crew mute's choices ("1 saat", "8 saat", "Yarın sabaha kadar", "Ben açana kadar"). When
// the time is up the launcher puts the player back to "Çevrimiçi": one timer, nothing polled, and a
// launcher that was closed meanwhile does it when it starts. A status changed meanwhile (here by
// hand, or on another device) drops the timer and changes nothing.
import type { StatusMode } from "@/lib/api/social-types";
import { MUTE_PRESETS, type MutePreset, muteUntilFor } from "../mute";
import type { StatusTimer } from "./prefs";

/** The choices, in the menu's order (one vocabulary with "Sessize al"). */
export const STATUS_PRESETS: readonly MutePreset[] = MUTE_PRESETS;

/** When a choice ends (Unix ms), or null for "Ben açana kadar". */
export function statusUntil(preset: MutePreset, nowMs: number): number | null {
  const until = muteUntilFor(preset, nowMs);
  return until === null ? null : until * 1000;
}

/**
 * What a kept timer means now, for the status shown (the one just chosen, while the service has
 * not confirmed it yet): nothing to do, wait until its end, it is over (back to "Çevrimiçi"), or
 * the status changed meanwhile (the timer goes).
 */
export function timerVerdict(
  timer: StatusTimer | null,
  status: StatusMode | undefined,
  nowMs: number,
): "none" | "wait" | "end" | "drop" {
  if (!timer) return "none";
  // Not signed in yet (or the first snapshot is on its way): it waits for the status.
  if (status === undefined) return "wait";
  if (status !== timer.status) return "drop";
  return nowMs >= timer.until ? "end" : "wait";
}

/** How long until the timer's end (ms; at least one second), for the one timeout it needs. */
export function timerDelay(timer: StatusTimer, nowMs: number): number {
  return Math.min(Math.max(timer.until - nowMs, 1_000), 2_000_000_000);
}
