// The event card's choice, as pl-content's `card_order` makes it (docs/content-format.md
// "Event card"): live events lead, the highest priority first, then the one that started last
// (a short event overlays a long one while it runs); without a live one, the soonest upcoming.
// The backend sends the events in this order for the moment it built the home content; the
// card applies the rule again whenever an event starts or ends, so it changes on time.
import type { EventItem } from "@/lib/api";
import i18n from "@/lib/i18n";

type SpanUnit = "days" | "hours" | "minutes" | "seconds";

const spanUnit = (name: SpanUnit, count: number) => i18n.t(`newsColumn.span.${name}`, { count });

/**
 * Time left on the event card and in the other events list, in whole words like the column's
 * "5 saat önce": "4 gün 23 saat", "2 saat 14 dakika", "38 dakika", "45 saniye" (the app's short
 * "sa" and "dk" have no match for "gün"). Two units at most, the larger first, a zero second unit
 * left out. It rounds down, so it changes 1 ms past each boundary as `countdownTickMs` expects,
 * and says "1 saniye" to the end rather than "0 saniye".
 */
export function timeLeftText(ms: number): string {
  const secs = Math.max(1, Math.floor(ms / 1000));
  const days = Math.floor(secs / 86_400);
  const hours = Math.floor((secs % 86_400) / 3600);
  const minutes = Math.floor((secs % 3600) / 60);
  const parts = days
    ? [spanUnit("days", days), hours ? spanUnit("hours", hours) : ""]
    : hours
      ? [spanUnit("hours", hours), minutes ? spanUnit("minutes", minutes) : ""]
      : [minutes ? spanUnit("minutes", minutes) : spanUnit("seconds", secs)];
  return parts.filter(Boolean).join(" ");
}

export type EventPhase = "upcoming" | "live" | "ended";

export function phaseOf(event: EventItem, now: number): EventPhase {
  if (now < Date.parse(event.startsAt)) return "upcoming";
  if (now < Date.parse(event.endsAt)) return "live";
  return "ended";
}

export function cardOrder(a: EventItem, b: EventItem, now: number): number {
  const aLive = phaseOf(a, now) === "live";
  const bLive = phaseOf(b, now) === "live";
  if (aLive !== bLive) return aLive ? -1 : 1;
  const priority = (b.priority ?? 0) - (a.priority ?? 0);
  const startA = Date.parse(a.startsAt);
  const startB = Date.parse(b.startsAt);
  const ordered = aLive ? priority || startB - startA : startA - startB || priority;
  if (ordered !== 0) return ordered;
  if (Boolean(a.fromFeed) !== Boolean(b.fromFeed)) return a.fromFeed ? 1 : -1;
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}

/** The events that have not ended at `now`, in card order (the card shows the first). */
export function cardEvents(events: EventItem[], now: number): EventItem[] {
  return events.filter((e) => phaseOf(e, now) !== "ended").sort((a, b) => cardOrder(a, b, now));
}

/** The next moment the card may change: the soonest start or end after `now`. */
export function nextBoundary(events: EventItem[], now: number): number | undefined {
  let next: number | undefined;
  for (const event of events) {
    for (const t of [Date.parse(event.startsAt), Date.parse(event.endsAt)]) {
      if (t > now && (next === undefined || t < next)) next = t;
    }
  }
  return next;
}
