// The same cases as pl-content's `a_short_event_overlays_a_long_one_while_it_runs`.
import { describe, expect, it } from "vitest";
import type { EventItem } from "@/lib/api";
import { cardEvents, nextBoundary } from "./event-card";

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
