// The same cases as pl-content's `a_short_event_overlays_a_long_one_while_it_runs`.
import { countdownTickMs } from "@pl/ui";
import { afterEach, describe, expect, it } from "vitest";
import type { EventItem } from "@/lib/api";
import { cardEvents, nextBoundary, startsText, timeLeftText, timeLocative } from "./event-card";
import i18n, { loadEnglish } from "./i18n";

function event(
  id: string,
  startsAt: string,
  endsAt: string,
  extra: Partial<EventItem> = {},
): EventItem {
  return { id, title: id, startsAt, endsAt, ...extra };
}

const beginner = event("beginner", "2026-10-16T00:00:00Z", "2026-10-23T00:00:00Z");
const cup = event("cup", "2026-10-16T14:00:00Z", "2026-10-16T16:00:00Z", { fromFeed: true });
const card = (events: EventItem[], at: string) => cardEvents(events, Date.parse(at))[0]?.id;

describe("event card", () => {
  it("lets a short event overlay a long one while it runs", () => {
    expect(card([beginner, cup], "2026-10-16T13:59:59Z")).toBe("beginner");
    expect(card([beginner, cup], "2026-10-16T14:00:00Z")).toBe("cup");
    expect(card([beginner, cup], "2026-10-16T16:00:00Z")).toBe("beginner");
    const both = cardEvents([beginner, cup], Date.parse("2026-10-16T15:00:00Z"));
    expect(both.map((e) => e.id)).toEqual(["cup", "beginner"]);
  });

  it("puts priority before the start time and the soonest upcoming without a live one", () => {
    expect(card([{ ...beginner, priority: 1 }, cup], "2026-10-16T15:00:00Z")).toBe("beginner");
    expect(card([beginner, { ...cup, priority: -1 }], "2026-10-16T15:00:00Z")).toBe("beginner");
    const far = event("far", "2026-10-26T20:00:00Z", "2026-10-26T22:00:00Z", { priority: 5 });
    const night = event("night", "2026-10-25T20:00:00Z", "2026-10-25T22:00:00Z");
    expect(card([far, night], "2026-10-24T00:00:00Z")).toBe("night");
    // Same start: config before feed, then id.
    const a = event("a", "2026-10-25T20:00:00Z", "2026-10-25T22:00:00Z", { fromFeed: true });
    const b = event("b", "2026-10-25T20:00:00Z", "2026-10-25T22:00:00Z");
    expect(card([a, b], "2026-10-24T00:00:00Z")).toBe("b");
  });

  it("wakes up at the next start or end", () => {
    const at = Date.parse("2026-10-16T13:00:00Z");
    expect(nextBoundary([beginner, cup], at)).toBe(Date.parse("2026-10-16T14:00:00Z"));
    expect(nextBoundary([beginner], Date.parse("2026-10-24T00:00:00Z"))).toBeUndefined();
  });
});

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

describe("time left", () => {
  afterEach(async () => {
    await i18n.changeLanguage("tr");
  });

  it("says two units at most in whole words, the larger first", () => {
    expect(timeLeftText(4 * DAY + 23 * HOUR + 40 * MINUTE)).toBe("4 gün 23 saat");
    expect(timeLeftText(3 * DAY + 20 * MINUTE)).toBe("3 gün");
    expect(timeLeftText(2 * HOUR + 14 * MINUTE + 57 * SECOND)).toBe("2 saat 14 dakika");
    expect(timeLeftText(2 * HOUR + 30 * SECOND)).toBe("2 saat");
    expect(timeLeftText(38 * MINUTE + 20 * SECOND)).toBe("38 dakika");
    expect(timeLeftText(45 * SECOND + 500)).toBe("45 saniye");
  });

  it("rounds down, so the words change just past each boundary the tick wakes at", () => {
    const left = 2 * HOUR + 14 * MINUTE + 57 * SECOND;
    const next = left - (countdownTickMs(left) ?? 0);
    expect(timeLeftText(next)).toBe("2 saat 13 dakika");
    expect(timeLeftText(MINUTE)).toBe("1 dakika");
    expect(timeLeftText(MINUTE - 1)).toBe("59 saniye");
  });

  it("says one second to the end, never zero", () => {
    expect(timeLeftText(400)).toBe("1 saniye");
    expect(timeLeftText(0)).toBe("1 saniye");
    expect(timeLeftText(-5 * SECOND)).toBe("1 saniye");
  });

  it("counts in English with singular units", async () => {
    await loadEnglish();
    await i18n.changeLanguage("en");
    expect(timeLeftText(4 * DAY + 23 * HOUR)).toBe("4 days 23 hours");
    expect(timeLeftText(DAY + HOUR)).toBe("1 day 1 hour");
    expect(timeLeftText(2 * HOUR + MINUTE)).toBe("2 hours 1 minute");
    expect(timeLeftText(SECOND)).toBe("1 second");
  });
});

describe("an upcoming event's start", () => {
  afterEach(async () => {
    await i18n.changeLanguage("tr");
  });

  it("gives a time the locative it takes as it is read", () => {
    const cases: [number, number, string][] = [
      [20, 0, "de"], // yirmide
      [19, 30, "da"], // otuzda
      [15, 0, "te"], // on beşte
      [21, 40, "ta"], // kırkta
      [16, 0, "da"], // on altıda
      [13, 0, "te"], // on üçte
      [10, 0, "da"], // onda
      [9, 0, "da"], // dokuzda
      [18, 50, "de"], // ellide
      [0, 0, "da"], // sıfırda
    ];
    for (const [hour, minute, ending] of cases) expect(timeLocative(hour, minute)).toBe(ending);
  });

  it("says when it starts in a sentence, by how far the day is", async () => {
    // Local times: Sunday 25 October 2026, 10:00.
    const now = new Date(2026, 9, 25, 10, 0).getTime();
    const at = (day: number, hour: number, minute = 0) =>
      new Date(2026, 9, day, hour, minute).toISOString();
    expect(startsText(at(25, 20), now)).toBe("Bugün 20:00'de başlıyor");
    expect(startsText(at(26, 19, 30), now)).toBe("Yarın 19:30'da başlıyor");
    expect(startsText(at(31, 15), now)).toBe("Cumartesi 15:00'te başlıyor");
    expect(startsText(at(2 + 31, 21, 40), now)).toBe("2 Kasım 21:40'ta başlıyor");
    await loadEnglish();
    await i18n.changeLanguage("en");
    expect(startsText(at(25, 20), now)).toBe("Starts today at 20:00");
    expect(startsText(at(31, 15), now)).toBe("Starts Saturday at 15:00");
  });
});
