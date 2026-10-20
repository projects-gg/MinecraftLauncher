// "Sessize al", pure rules (mute.test.ts): when each preset ends, whether a mute is on at a moment
// (a timed one ends by itself and nothing says so: everyone compares the time), which crew
// notifications are heard, and the pieces of "Sessizde · 14:30'a kadar". The service keeps the
// settings (`teamFeatures: mute`); without it the launcher's own switches decide, as before.
import type { CrewNotifications } from "@/lib/api/social-types";

export type MutePreset = "hour" | "eightHours" | "morning" | "forever";

/** The menu's order: "1 saat", "8 saat", "Yarın sabaha kadar", "Ben açana kadar". */
export const MUTE_PRESETS: readonly MutePreset[] = ["hour", "eightHours", "morning", "forever"];

/** "Yarın sabaha kadar" lasts until this local hour. */
export const MORNING_HOUR = 8;

/** The next local 08:00 after `nowMs`, on the local calendar (so a clock change between does not
 * move it). */
export function nextMorning(nowMs: number): number {
  const at = new Date(nowMs);
  at.setHours(MORNING_HOUR, 0, 0, 0);
  if (at.getTime() <= nowMs) {
    at.setDate(at.getDate() + 1);
    at.setHours(MORNING_HOUR, 0, 0, 0);
  }
  return at.getTime();
}

/**
 * When a preset's mute ends (Unix seconds), or null for "Ben açana kadar". Hours count from the
 * next whole minute, so the end reads as the time it really is ("14:18'e kadar").
 */
export function muteUntilFor(preset: MutePreset, nowMs: number): number | null {
  const minute = Math.ceil(nowMs / 60_000) * 60;
  switch (preset) {
    case "hour":
      return minute + 3_600;
    case "eightHours":
      return minute + 8 * 3_600;
    case "morning":
      return Math.round(nextMorning(nowMs) / 1000);
    case "forever":
      return null;
  }
}

/** Muted at `nowMs`. */
export function mutedNow(settings: CrewNotifications | undefined, nowMs: number): boolean {
  if (!settings?.muted) return false;
  return settings.muteUntil === null || settings.muteUntil * 1000 > nowMs;
}

/**
 * Whether a crew call is announced (a toast, a notification, the gold dot): the service's switch
 * and no mute where the service keeps them, else this computer's "Ekip çağrıları".
 */
export function crewCallsHeard(
  settings: CrewNotifications | undefined,
  localSwitch: boolean,
  nowMs: number,
): boolean {
  if (!settings) return localSwitch;
  return settings.calls && !mutedNow(settings, nowMs);
}

/** How long until a timed mute ends (ms), or null: the only timer a mute needs. */
export function nextMuteTick(
  settings: CrewNotifications | undefined,
  nowMs: number,
): number | null {
  if (!settings?.muted || settings.muteUntil === null) return null;
  const left = settings.muteUntil * 1000 - nowMs;
  return left > 0 ? left : null;
}

/** Which day a mute ends on, as the player reads it: today, tomorrow or a date. */
export function muteEndDay(untilMs: number, nowMs: number): "today" | "tomorrow" | "later" {
  const day = (ms: number) => {
    const date = new Date(ms);
    return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  };
  const today = day(nowMs);
  const end = day(untilMs);
  if (end <= today) return "today";
  const tomorrow = new Date(today);
  tomorrow.setDate(tomorrow.getDate() + 1);
  return end === tomorrow.getTime() ? "tomorrow" : "later";
}

/** Bir, iki, üç… dokuz: their dative endings ("bire", "ikiye", "üçe"). */
const ONES = ["a", "e", "ye", "e", "e", "e", "ya", "ye", "e", "a"] as const;
/** On, yirmi, otuz, kırk, elli. */
const TENS = ["a", "a", "ye", "a", "a", "ye"] as const;

/**
 * The Turkish dative ending a clock time takes after its apostrophe, by how it is read: the
 * minutes' last word, or the hour's on the hour ("14:30'a", "14:00'e", "16:00'ya", "12:00'ye").
 */
export function timeDative(hour: number, minute: number): "a" | "e" | "ya" | "ye" {
  const read = minute === 0 ? hour : minute;
  const ones = read % 10;
  // Sıfır (00:00) reads like dokuz: "sıfıra".
  const ending = read === 0 ? "a" : ones ? ONES[ones] : TENS[Math.floor(read / 10) % 10];
  return ending ?? "a";
}
