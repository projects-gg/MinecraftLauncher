// Event reminders ("Hatırlat"), the pure part: when a reminder rings, which ones are due now and
// what they say, and which stored ones can go. The store is stores/reminders.ts; the bell and the
// scheduler that tells the player are in event-reminders.tsx. Reminders stay on this computer.
import type { EventItem } from "@/lib/api";

/** A reminder rings this long before its event starts. */
export const REMINDER_LEAD_MS = 5 * 60_000;

/**
 * The scheduler sleeps at most this long between checks. A computer that slept past a reminder
 * gets it within a minute of waking (timers do not count the time asleep), and `setTimeout`
 * never sees a delay it cannot hold (beyond ~24.8 days it fires at once).
 */
export const CHECK_EVERY_MS = 60_000;

/**
 * A reminder the player set, by event id. The event's times are copied from the last content
 * that listed it: while the event is missing (a feed outage), the reminder waits instead of
 * being lost, and it is dropped once that end has passed.
 */
export interface Reminder {
  startsAt: string;
  endsAt: string;
  /** When it was told (or passed over while the game ran). */
  firedAt?: number;
}

export type Reminders = Record<string, Reminder>;

/** A reminder whose moment has come: its event starts in a few minutes, or already runs. */
export type DueReminder =
  | { event: EventItem; kind: "soon"; minutes: number }
  | { event: EventItem; kind: "running" };

/** When the reminder for an event starting at `startsAt` rings. */
export function ringsAt(startsAt: string): number {
  return Date.parse(startsAt) - REMINDER_LEAD_MS;
}

/** Whole minutes until the start, rounded up: "1 dakika" in the last minute, never 0. */
export function minutesUntil(startsAt: string, now: number): number {
  return Math.max(1, Math.ceil((Date.parse(startsAt) - now) / 60_000));
}

/**
 * Brings the stored reminders up to date with the content. A moved event moves its reminder,
 * which rings again if it already rang and its new moment is still ahead; reminders whose event
 * has ended go. A reminder whose event is not listed keeps its last known times. Returns the
 * same object when nothing changed.
 */
export function reconcile(stored: Reminders, events: EventItem[], now: number): Reminders {
  const listed = new Map(events.map((event) => [event.id, event]));
  const next: Reminders = {};
  let changed = false;
  for (const [id, reminder] of Object.entries(stored)) {
    let current = reminder;
    const event = listed.get(id);
    if (event && (event.startsAt !== reminder.startsAt || event.endsAt !== reminder.endsAt)) {
      const moved = Date.parse(event.startsAt) !== Date.parse(reminder.startsAt);
      const rearm = moved && ringsAt(event.startsAt) > now;
      current = { startsAt: event.startsAt, endsAt: event.endsAt };
      if (reminder.firedAt !== undefined && !rearm) current.firedAt = reminder.firedAt;
      changed = true;
    }
    const ends = Date.parse(current.endsAt);
    if (Number.isNaN(ends) || ends <= now) {
      changed = true;
      continue;
    }
    next[id] = current;
  }
  return changed ? next : stored;
}

/**
 * The reminders that should be told now, soonest start first: they have not rung, their moment
 * has passed and their event is listed and has not ended. One set less than five minutes before
 * the start is due at once; one missed while the launcher was closed or the computer slept says
 * that the event already runs.
 */
export function dueReminders(stored: Reminders, events: EventItem[], now: number): DueReminder[] {
  const due: DueReminder[] = [];
  for (const event of events) {
    const reminder = stored[event.id];
    if (!reminder || reminder.firedAt !== undefined || ringsAt(event.startsAt) > now) continue;
    if (now >= Date.parse(event.endsAt)) continue;
    due.push(
      now < Date.parse(event.startsAt)
        ? { event, kind: "soon", minutes: minutesUntil(event.startsAt, now) }
        : { event, kind: "running" },
    );
  }
  return due.sort((a, b) => Date.parse(a.event.startsAt) - Date.parse(b.event.startsAt));
}

/** The next moment a stored reminder of a listed event rings, if any is still ahead. */
export function nextRing(stored: Reminders, events: EventItem[], now: number): number | undefined {
  let next: number | undefined;
  for (const event of events) {
    const reminder = stored[event.id];
    if (!reminder || reminder.firedAt !== undefined) continue;
    const at = ringsAt(event.startsAt);
    if (at > now && (next === undefined || at < next)) next = at;
  }
  return next;
}

/**
 * How long the scheduler waits before it looks again: until the next ring (a little after it,
 * so the clock has surely passed it), at most `CHECK_EVERY_MS`.
 */
export function wakeDelay(next: number, now: number): number {
  const wait = next - now;
  return wait > CHECK_EVERY_MS ? CHECK_EVERY_MS : Math.max(0, wait) + 250;
}
