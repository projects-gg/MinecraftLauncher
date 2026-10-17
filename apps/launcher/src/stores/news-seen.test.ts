import { afterEach, describe, expect, it, vi } from "vitest";

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

const DAY = 86_400_000;
const NOW = Date.parse("2026-10-17T12:00:00Z");
const at = (ms: number) => new Date(ms).toISOString();

/** Starts the launcher (a fresh import of the store) at `now`. */
async function launch(now = NOW) {
  vi.resetModules();
  vi.useFakeTimers({ now, toFake: ["Date"] });
  return import("./news-seen");
}

afterEach(() => {
  storage.clear();
  vi.useRealTimers();
});

describe("isNewsNew", () => {
  it("is new when published after the watermark, recently, and not opened", async () => {
    const { isNewsNew } = await launch();
    const seen = { watermark: NOW - DAY, read: {} };
    expect(isNewsNew({ id: "a", publishedAt: at(NOW - 2 * 3_600_000) }, seen, NOW)).toBe(true);
    expect(isNewsNew({ id: "b", publishedAt: at(NOW - 2 * DAY) }, seen, NOW)).toBe(false);
    expect(
      isNewsNew({ id: "a", publishedAt: at(NOW - 3_600_000) }, { ...seen, read: { a: NOW } }, NOW),
    ).toBe(false);
  });

  it("is never new after 14 days, even past an old watermark", async () => {
    const { isNewsNew, NEW_FOR_MS } = await launch();
    const seen = { watermark: NOW - 60 * DAY, read: {} };
    expect(NEW_FOR_MS).toBe(14 * DAY);
    expect(isNewsNew({ id: "a", publishedAt: at(NOW - 13 * DAY) }, seen, NOW)).toBe(true);
    expect(isNewsNew({ id: "b", publishedAt: at(NOW - 15 * DAY) }, seen, NOW)).toBe(false);
  });

  it("treats a pinned article like any other: old means not new", async () => {
    const { isNewsNew } = await launch();
    const pinned = { id: "pinned", publishedAt: at(NOW - 3 * DAY), pinned: true };
    expect(isNewsNew(pinned, { watermark: NOW - DAY, read: {} }, NOW)).toBe(false);
  });

  it("shows nothing as new before the first start set a watermark", async () => {
    const { isNewsNew } = await launch();
    expect(isNewsNew({ id: "a", publishedAt: at(NOW) }, { watermark: null, read: {} }, NOW)).toBe(
      false,
    );
  });
});

describe("useNewsSeen", () => {
  it("starts with the watermark at the first start, so a fresh install lights up nothing", async () => {
    const { isNewsNew, useNewsSeen } = await launch();
    const seen = useNewsSeen.getState();
    expect(seen.watermark).toBe(NOW);
    expect(isNewsNew({ id: "old", publishedAt: at(NOW - 3_600_000) }, seen, NOW)).toBe(false);
    expect(isNewsNew({ id: "fresh", publishedAt: at(NOW + 3_600_000) }, seen, NOW + DAY)).toBe(
      true,
    );
  });

  it("keeps the watermark and opened articles across a restart", async () => {
    const first = await launch();
    first.useNewsSeen.getState().markRead("a");

    const second = await launch(NOW + DAY);
    expect(second.useNewsSeen.getState().watermark).toBe(NOW);
    expect(second.useNewsSeen.getState().read).toEqual({ a: NOW });
  });

  it("moves the watermark forward only", async () => {
    const { useNewsSeen } = await launch();
    useNewsSeen.getState().advance(NOW + 3_600_000);
    expect(useNewsSeen.getState().watermark).toBe(NOW + 3_600_000);
    useNewsSeen.getState().advance(NOW - DAY);
    expect(useNewsSeen.getState().watermark).toBe(NOW + 3_600_000);
  });

  it("forgets opened articles after 30 days", async () => {
    const { useNewsSeen } = await launch();
    useNewsSeen.getState().markRead("old", NOW - 31 * DAY);
    useNewsSeen.getState().markRead("recent", NOW - DAY);
    useNewsSeen.getState().markRead("now", NOW);
    expect(Object.keys(useNewsSeen.getState().read).sort()).toEqual(["now", "recent"]);
  });

  it("finds the newest article a visit showed", async () => {
    const { newestPublished } = await launch();
    expect(newestPublished([])).toBe(0);
    expect(
      newestPublished([{ publishedAt: at(NOW - DAY) }, { publishedAt: at(NOW - 3_600_000) }]),
    ).toBe(NOW - 3_600_000);
  });
});
