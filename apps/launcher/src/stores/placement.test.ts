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

const NOW = Date.parse("2026-10-19T18:00:00Z");

/** Starts the launcher (a fresh import of the store) at `now`. */
async function launch(now = NOW) {
  vi.resetModules();
  vi.useFakeTimers({ now, toFake: ["Date"] });
  return import("./placement");
}

afterEach(() => {
  storage.clear();
  vi.useRealTimers();
});

describe("placeOf", () => {
  it("keeps an entry at home until the player moves it", async () => {
    const { placeOf } = await launch();
    expect(placeOf("friends", {})).toBe("titlebar");
    expect(placeOf("friends", { friends: "rail" })).toBe("rail");
  });

  it("ignores a stored place the entry cannot take", async () => {
    const { placeOf } = await launch();
    expect(placeOf("friends", { friends: "dock" })).toBe("titlebar");
    expect(placeOf("friends", { friends: 42 })).toBe("titlebar");
  });

  it("offers the other place in the right-click menu", async () => {
    const { nextPlace } = await launch();
    expect(nextPlace("friends", "titlebar")).toBe("rail");
    expect(nextPlace("friends", "rail")).toBe("titlebar");
  });
});

describe("usePlacement", () => {
  it("remembers the move across restarts, but not the move itself", async () => {
    const first = await launch();
    first.usePlacement.getState().move("friends", "rail");
    expect(first.placeOf("friends", first.usePlacement.getState().chosen)).toBe("rail");
    expect(first.justMoved("friends")).toBe(true);

    const second = await launch(NOW + 60_000);
    expect(second.placeOf("friends", second.usePlacement.getState().chosen)).toBe("rail");
    // The next start shows the entry where it is, without a move animation.
    expect(second.usePlacement.getState().lastMove).toBeNull();
    expect(second.justMoved("friends")).toBe(false);
  });

  it("counts the arrival as the move only for a moment", async () => {
    const { usePlacement, justMoved, MOVE_WINDOW_MS } = await launch();
    usePlacement.getState().move("friends", "rail");
    expect(justMoved("friends", NOW + MOVE_WINDOW_MS - 1)).toBe(true);
    expect(justMoved("friends", NOW + MOVE_WINDOW_MS)).toBe(false);
  });

  it("does nothing when the entry is already there", async () => {
    const { usePlacement } = await launch();
    usePlacement.getState().move("friends", "titlebar");
    expect(usePlacement.getState().lastMove).toBeNull();
  });

  it("finds the entry's button at one place only", async () => {
    const { entrySelector } = await launch();
    expect(entrySelector("friends", "rail")).toBe('[data-movable="friends"][data-place="rail"]');
  });
});
