import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RunningGame } from "@/stores/launch";
import {
  countLaunch,
  EMPTY,
  newShownRuns,
  parseStars,
  STAR_LAUNCHES,
  STAR_WAIT_MS,
  STORAGE_KEY,
  type StarState,
  starLineDue,
  starLineVisible,
  useStars,
} from "./stars";

// The store saves to localStorage; Node has none, so the tests bring one.
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

const DAY = 24 * 60 * 60 * 1000;
/** Sunday 1 November 2026, 18:00 in Türkiye. */
const T0 = Date.parse("2026-11-01T15:00:00Z");

/** Ten games over a week, the line not shown yet. */
const ready: StarState = { launches: STAR_LAUNCHES, firstAt: T0, shownAt: null, done: false };
const at = T0 + STAR_WAIT_MS;
const free = { now: at, busy: false, onboarded: true, shownThisSession: false };

function game(run: number, shown: boolean): RunningGame {
  return {
    run,
    game: run,
    instanceId: "projects",
    account: null,
    since: T0,
    shown,
    firstStart: false,
  };
}

describe("parseStars", () => {
  it("starts over on anything unreadable", () => {
    expect(parseStars(null)).toEqual(EMPTY);
    expect(parseStars("")).toEqual(EMPTY);
    expect(parseStars("{")).toEqual(EMPTY);
    expect(parseStars("[]")).toEqual({ ...EMPTY });
    expect(parseStars('"x"')).toEqual(EMPTY);
  });

  it("keeps valid fields and drops bad ones", () => {
    expect(parseStars(JSON.stringify(ready))).toEqual(ready);
    expect(
      parseStars(JSON.stringify({ launches: -3, firstAt: "soon", shownAt: 0, done: "yes" })),
    ).toEqual(EMPTY);
    expect(parseStars(JSON.stringify({ launches: 2.5, firstAt: T0 }))).toEqual({
      ...EMPTY,
      firstAt: T0,
    });
  });
});

describe("countLaunch", () => {
  it("counts and remembers the first game", () => {
    const one = countLaunch(EMPTY, T0);
    expect(one).toEqual({ ...EMPTY, launches: 1, firstAt: T0 });
    expect(countLaunch(one, T0 + DAY)).toEqual({ ...EMPTY, launches: 2, firstAt: T0 });
  });

  it("stops once the line has shown or been answered", () => {
    const shown = { ...ready, shownAt: at };
    expect(countLaunch(shown, at + DAY)).toBe(shown);
    const done = { ...ready, done: true };
    expect(countLaunch(done, at + DAY)).toBe(done);
  });
});

describe("starLineDue", () => {
  it("needs ten games over a week", () => {
    expect(starLineDue(ready, at)).toBe(true);
    expect(starLineDue(ready, at - 1)).toBe(false);
    expect(starLineDue({ ...ready, launches: STAR_LAUNCHES - 1 }, at + DAY)).toBe(false);
    expect(starLineDue({ ...ready, firstAt: null }, at)).toBe(false);
  });

  it("never comes back after it showed or was answered", () => {
    expect(starLineDue({ ...ready, shownAt: at }, at + DAY)).toBe(false);
    expect(starLineDue({ ...ready, done: true }, at + DAY)).toBe(false);
  });

  it("waits when the clock went back", () => {
    expect(starLineDue(ready, T0 - DAY)).toBe(false);
  });
});

describe("starLineVisible", () => {
  it("shows when due and the player is free", () => {
    expect(starLineVisible(ready, free)).toBe(true);
  });

  it("waits for the first-start setup and never covers a game", () => {
    expect(starLineVisible(ready, { ...free, onboarded: false })).toBe(false);
    expect(starLineVisible(ready, { ...free, busy: true })).toBe(false);
  });

  it("stays this session after its first appearance, until an answer", () => {
    const shown = { ...ready, shownAt: at };
    expect(starLineVisible(shown, { ...free, now: at + DAY, shownThisSession: true })).toBe(true);
    expect(starLineVisible(shown, { ...free, now: at + DAY })).toBe(false);
    expect(
      starLineVisible({ ...shown, done: true }, { ...free, now: at + DAY, shownThisSession: true }),
    ).toBe(false);
    expect(starLineVisible(shown, { ...free, busy: true, shownThisSession: true })).toBe(false);
  });
});

describe("newShownRuns", () => {
  it("takes this page's games on screen, each once", () => {
    const games = [game(1, true), game(2, false), game(-1, true), game(3, true)];
    expect(newShownRuns(games, new Set())).toEqual([1, 3]);
    expect(newShownRuns(games, new Set([1]))).toEqual([3]);
  });
});

describe("useStars", () => {
  beforeEach(() => {
    storage.clear();
    useStars.setState({ state: EMPTY, session: false });
  });

  it("saves the count", () => {
    useStars.getState().count(T0);
    useStars.getState().count(T0 + DAY);
    expect(parseStars(storage.getItem(STORAGE_KEY))).toEqual({
      ...EMPTY,
      launches: 2,
      firstAt: T0,
    });
  });

  it("remembers the first appearance at once and keeps it for the session", () => {
    useStars.setState({ state: ready });
    useStars.getState().show(at);
    expect(useStars.getState().session).toBe(true);
    expect(parseStars(storage.getItem(STORAGE_KEY)).shownAt).toBe(at);
    useStars.getState().show(at + DAY);
    expect(useStars.getState().state.shownAt).toBe(at);
  });

  it("closes for good on an answer", () => {
    useStars.setState({ state: { ...ready, shownAt: at }, session: true });
    useStars.getState().finish();
    expect(parseStars(storage.getItem(STORAGE_KEY))).toEqual({ ...ready, shownAt: at, done: true });
    expect(starLineVisible(useStars.getState().state, { ...free, shownThisSession: true })).toBe(
      false,
    );
  });

  it("does not throw when storage refuses", () => {
    const setItem = vi.spyOn(storage, "setItem").mockImplementation(() => {
      throw new Error("QuotaExceededError");
    });
    useStars.getState().count(T0);
    expect(useStars.getState().state.launches).toBe(1);
    setItem.mockRestore();
  });
});
