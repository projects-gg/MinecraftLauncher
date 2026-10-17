import { afterEach, describe, expect, it, vi } from "vitest";
import type { EventItem } from "@/lib/api";
import {
  CHECK_EVERY_MS,
  dueReminders,
  minutesUntil,
  nextRing,
  REMINDER_LEAD_MS,
  type Reminders,
  reconcile,
  ringsAt,
  wakeDelay,
} from "./reminders";

// The store persists to localStorage; Node has none, so the tests bring a shared one (a restart
// re-imports the store over the same storage).
const storage = vi.hoisted(() => {
  const map = new Map<string, string>();
  const memory = {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => {
      map.set(key, String(value));
    },
    removeItem: (key: string) => {
      map.delete(key);
    },
    clear: () => map.clear(),
    key: (index: number) => [...map.keys()][index] ?? null,
    get length() {
      return map.size;
    },
  };
  Object.defineProperty(globalThis, "localStorage", { value: memory, configurable: true });
  return memory;
});

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
/** Saturday 24 October 2026, 19:00 in Türkiye. */
const T0 = Date.parse("2026-10-24T16:00:00Z");

function event(id: string, startsIn: number, length = 3 * HOUR): EventItem {
  return {
    id,
    title: `Etkinlik ${id}`,
    startsAt: new Date(T0 + startsIn).toISOString(),
    endsAt: new Date(T0 + startsIn + length).toISOString(),
  };
}

function remind(e: EventItem, firedAt?: number): Reminders[string] {
  return firedAt === undefined
    ? { startsAt: e.startsAt, endsAt: e.endsAt }
    : { startsAt: e.startsAt, endsAt: e.endsAt, firedAt };
}

describe("when a reminder rings", () => {
  const pvp = event("pvp", HOUR);

  it("rings five minutes before the start", () => {
    expect(REMINDER_LEAD_MS).toBe(5 * MINUTE);
    expect(ringsAt(pvp.startsAt)).toBe(T0 + HOUR - 5 * MINUTE);
    expect(nextRing({ pvp: remind(pvp) }, [pvp], T0)).toBe(T0 + 55 * MINUTE);
  });

  it("wakes for the soonest reminder and skips events without one or already told", () => {
    const later = event("later", 3 * HOUR);
    const plain = event("plain", 30 * MINUTE);
    const told = event("told", 20 * MINUTE);
    const stored = { later: remind(later), pvp: remind(pvp), told: remind(told, T0) };
    expect(nextRing(stored, [later, plain, pvp, told], T0)).toBe(T0 + 55 * MINUTE);
    expect(nextRing({ told: remind(told, T0) }, [told], T0)).toBeUndefined();
  });

  it("never waits on a moment already past", () => {
    expect(nextRing({ pvp: remind(pvp) }, [pvp], T0 + 56 * MINUTE)).toBeUndefined();
  });

  it("counts whole minutes up to the start, never zero", () => {
    expect(minutesUntil(pvp.startsAt, T0 + 55 * MINUTE)).toBe(5);
    expect(minutesUntil(pvp.startsAt, T0 + 55 * MINUTE + 250)).toBe(5);
    expect(minutesUntil(pvp.startsAt, T0 + 57.5 * MINUTE)).toBe(3);
    expect(minutesUntil(pvp.startsAt, T0 + HOUR - 20_000)).toBe(1);
  });
});

describe("dueReminders", () => {
  const pvp = event("pvp", HOUR);
  const stored = { pvp: remind(pvp) };

  it("tells nothing before the moment and says how soon at the moment", () => {
    expect(dueReminders(stored, [pvp], T0)).toEqual([]);
    expect(dueReminders(stored, [pvp], T0 + 55 * MINUTE)).toEqual([
      { event: pvp, kind: "soon", minutes: 5 },
    ]);
  });

  it("tells a reminder set less than five minutes before the start at once", () => {
    const raid = event("raid", 3 * MINUTE);
    expect(dueReminders({ raid: remind(raid) }, [raid], T0)).toEqual([
      { event: raid, kind: "soon", minutes: 3 },
    ]);
  });

  it("says the event is on when the reminder was missed (closed launcher, sleep)", () => {
    // The computer slept from before the moment until half an hour into the event.
    expect(dueReminders(stored, [pvp], T0 + HOUR + 30 * MINUTE)).toEqual([
      { event: pvp, kind: "running" },
    ]);
  });

  it("never tells a reminder whose event has ended", () => {
    expect(dueReminders(stored, [pvp], T0 + 4 * HOUR)).toEqual([]);
  });

  it("tells a reminder once", () => {
    expect(dueReminders({ pvp: remind(pvp, T0 + 55 * MINUTE) }, [pvp], T0 + 56 * MINUTE)).toEqual(
      [],
    );
  });

  it("waits while the event is not listed (a feed outage)", () => {
    expect(dueReminders(stored, [], T0 + 55 * MINUTE)).toEqual([]);
  });

  it("puts the soonest start first", () => {
    const early = event("early", 2 * MINUTE);
    const late = event("late", 4 * MINUTE);
    const due = dueReminders({ late: remind(late), early: remind(early) }, [late, early], T0);
    expect(due.map((d) => d.event.id)).toEqual(["early", "late"]);
  });
});

describe("reconcile", () => {
  const pvp = event("pvp", HOUR);

  it("returns the same reminders when nothing changed", () => {
    const stored = { pvp: remind(pvp) };
    expect(reconcile(stored, [pvp], T0)).toBe(stored);
  });

  it("follows an event Studio moved, and rings again when its new moment is ahead", () => {
    const told = { pvp: remind(pvp, T0 + 55 * MINUTE) };
    const moved = event("pvp", 2 * HOUR);
    const next = reconcile(told, [moved], T0 + 56 * MINUTE);
    expect(next.pvp).toEqual({ startsAt: moved.startsAt, endsAt: moved.endsAt });
    expect(nextRing(next, [moved], T0 + 56 * MINUTE)).toBe(T0 + 115 * MINUTE);
  });

  it("does not ring twice when the moved moment is already past", () => {
    const told = { pvp: remind(pvp, T0 + 55 * MINUTE) };
    const moved = event("pvp", 58 * MINUTE);
    const next = reconcile(told, [moved], T0 + 56 * MINUTE);
    expect(next.pvp?.firedAt).toBe(T0 + 55 * MINUTE);
    expect(dueReminders(next, [moved], T0 + 56 * MINUTE)).toEqual([]);
  });

  it("keeps the told state when only the end changed", () => {
    const told = { pvp: remind(pvp, T0 + 55 * MINUTE) };
    const longer = { ...pvp, endsAt: new Date(T0 + 6 * HOUR).toISOString() };
    expect(reconcile(told, [longer], T0 + HOUR)).toEqual({
      pvp: { startsAt: pvp.startsAt, endsAt: longer.endsAt, firedAt: T0 + 55 * MINUTE },
    });
  });

  it("drops reminders whose event has ended, listed or not", () => {
    const gone = event("gone", -5 * HOUR);
    const stored = { pvp: remind(pvp), gone: remind(gone) };
    expect(reconcile(stored, [pvp], T0)).toEqual({ pvp: remind(pvp) });
    expect(reconcile({ pvp: remind(pvp) }, [pvp], T0 + 4 * HOUR)).toEqual({});
  });

  it("keeps a reminder whose event is missing until its known end", () => {
    const stored = { pvp: remind(pvp) };
    expect(reconcile(stored, [], T0 + 2 * HOUR)).toBe(stored);
    expect(reconcile(stored, [], T0 + 4 * HOUR)).toEqual({});
  });
});

describe("wakeDelay", () => {
  it("waits until just after the ring", () => {
    expect(wakeDelay(T0 + 30_000, T0)).toBe(30_250);
    expect(wakeDelay(T0 - 5_000, T0)).toBe(250);
  });

  it("checks again within a minute when the ring is far (sleep, setTimeout's limit)", () => {
    expect(wakeDelay(T0 + 2 * MINUTE, T0)).toBe(CHECK_EVERY_MS);
    expect(wakeDelay(T0 + 30 * 24 * HOUR, T0)).toBe(CHECK_EVERY_MS);
    expect(CHECK_EVERY_MS).toBeLessThan(2 ** 31 - 1);
  });
});

describe("reminders across a restart", () => {
  afterEach(() => {
    storage.clear();
    vi.resetModules();
  });

  async function launch() {
    vi.resetModules();
    return (await import("@/stores/reminders")).useReminders;
  }

  it("keeps a reminder, and tells it as on when the launcher was closed at its moment", async () => {
    const pvp = event("pvp", HOUR);
    const first = await launch();
    expect(first.getState().toggle(pvp)).toBe(true);

    const second = await launch();
    const items = second.getState().items;
    expect(items).toEqual({ pvp: remind(pvp) });
    expect(dueReminders(items, [pvp], T0 + HOUR + 10 * MINUTE)).toEqual([
      { event: pvp, kind: "running" },
    ]);
  });

  it("remembers what was told and the first-time explanation", async () => {
    const pvp = event("pvp", HOUR);
    const first = await launch();
    first.getState().toggle(pvp);
    first.getState().setExplained();
    first.getState().markFired(["pvp"], T0 + 55 * MINUTE);

    const second = await launch();
    expect(second.getState().explained).toBe(true);
    expect(dueReminders(second.getState().items, [pvp], T0 + 56 * MINUTE)).toEqual([]);
  });

  it("removes a reminder pressed again", async () => {
    const pvp = event("pvp", HOUR);
    const first = await launch();
    first.getState().toggle(pvp);
    expect(first.getState().toggle(pvp)).toBe(false);

    const second = await launch();
    expect(second.getState().items).toEqual({});
  });
});
