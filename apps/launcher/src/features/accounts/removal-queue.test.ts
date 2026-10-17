import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createRemovalQueue, pickNextActive, type RecencyAccount } from "./removal-queue";

describe("createRemovalQueue", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("undo brings the item back and nothing is finalized", () => {
    const finalize = vi.fn();
    const queue = createRemovalQueue<string>(finalize, { safetyMs: 1000 });
    queue.add("a", "Alex");
    expect(queue.has("a")).toBe(true);
    expect(queue.undo("a")).toBe("Alex");
    expect(queue.has("a")).toBe(false);
    vi.advanceTimersByTime(5000);
    expect(finalize).not.toHaveBeenCalled();
    // A second undo is too late.
    expect(queue.undo("a")).toBeNull();
  });

  it("settling makes the removal final once", () => {
    const finalize = vi.fn();
    const queue = createRemovalQueue<string>(finalize, { safetyMs: 1000 });
    queue.add("a", "Alex");
    queue.settle("a");
    queue.settle("a");
    vi.advanceTimersByTime(5000);
    expect(finalize).toHaveBeenCalledTimes(1);
    expect(finalize).toHaveBeenCalledWith("a", "Alex");
    expect(queue.undo("a")).toBeNull();
  });

  it("the safety timer finalizes removals the toast never reported", () => {
    const finalize = vi.fn();
    const queue = createRemovalQueue<string>(finalize, { safetyMs: 1000 });
    queue.add("a", "Alex");
    vi.advanceTimersByTime(999);
    expect(finalize).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(finalize).toHaveBeenCalledWith("a", "Alex");
    expect(queue.has("a")).toBe(false);
  });

  it("knows the latest removal for Ctrl+Z and flushes everything", () => {
    const finalize = vi.fn();
    const queue = createRemovalQueue<string>(finalize);
    expect(queue.latest()).toBeNull();
    queue.add("a", "Alex");
    queue.add("b", "Bora");
    expect(queue.latest()).toBe("b");
    queue.undo("b");
    expect(queue.latest()).toBe("a");
    queue.flush();
    expect(finalize).toHaveBeenCalledTimes(1);
    expect(queue.latest()).toBeNull();
  });

  it("removing the same id again finalizes the older window first", () => {
    const finalize = vi.fn();
    const queue = createRemovalQueue<number>(finalize);
    queue.add("a", 1);
    queue.add("a", 2);
    expect(finalize).toHaveBeenCalledWith("a", 1);
    expect(queue.undo("a")).toBe(2);
  });
});

describe("pickNextActive", () => {
  const hoursAgo = (h: number) => new Date(Date.UTC(2026, 9, 16, 12) - h * 3_600_000).toISOString();
  const list: RecencyAccount[] = [
    { id: "old", active: false, addedAt: hoursAgo(500), lastUsedAt: hoursAgo(200) },
    { id: "recent", active: false, addedAt: hoursAgo(400), lastUsedAt: hoursAgo(5) },
    { id: "fresh", active: false, addedAt: hoursAgo(1) },
    { id: "current", active: true, addedAt: hoursAgo(600), lastUsedAt: hoursAgo(1) },
  ];

  it("keeps the selection when an inactive account goes", () => {
    expect(pickNextActive(list, "old")).toBe("current");
  });

  it("picks the most recently played when the active account goes", () => {
    expect(pickNextActive(list, "current")).toBe("recent");
  });

  it("falls back to the most recently added, then to nothing", () => {
    const unplayed: RecencyAccount[] = [
      { id: "a", active: false, addedAt: hoursAgo(10) },
      { id: "b", active: false, addedAt: hoursAgo(2) },
      { id: "c", active: true, addedAt: hoursAgo(30) },
    ];
    expect(pickNextActive(unplayed, "c")).toBe("b");
    expect(pickNextActive([{ id: "only", active: true }], "only")).toBeNull();
  });
});
