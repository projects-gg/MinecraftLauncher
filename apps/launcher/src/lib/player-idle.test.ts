import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IDLE_AFTER_MS, isPlayerIdle, playerActive, trackPlayerIdle } from "./player-idle";

describe("player idle", () => {
  let target: EventTarget;
  let stop: () => void;

  beforeEach(() => {
    vi.useFakeTimers();
    target = new EventTarget();
    stop = trackPlayerIdle(target);
  });

  afterEach(() => {
    stop();
    vi.useRealTimers();
  });

  const input = (type = "pointermove") => target.dispatchEvent(new Event(type));

  it("lets the opening moment play, then rests without input", () => {
    expect(isPlayerIdle()).toBe(false);
    vi.advanceTimersByTime(IDLE_AFTER_MS - 1);
    expect(isPlayerIdle()).toBe(false);
    vi.advanceTimersByTime(1);
    expect(isPlayerIdle()).toBe(true);
  });

  it("counts from the last input", () => {
    vi.advanceTimersByTime(5_000);
    input();
    vi.advanceTimersByTime(IDLE_AFTER_MS - 1);
    expect(isPlayerIdle()).toBe(false);
    vi.advanceTimersByTime(1);
    expect(isPlayerIdle()).toBe(true);
  });

  it("wakes on any input", () => {
    for (const type of ["pointermove", "pointerdown", "wheel", "keydown", "touchstart"]) {
      vi.advanceTimersByTime(IDLE_AFTER_MS);
      expect(isPlayerIdle()).toBe(true);
      input(type);
      expect(isPlayerIdle()).toBe(false);
    }
  });

  it("rests at once when another window takes the focus, and wakes with it back", () => {
    target.dispatchEvent(new Event("blur"));
    expect(isPlayerIdle()).toBe(true);
    target.dispatchEvent(new Event("focus"));
    expect(isPlayerIdle()).toBe(false);
    vi.advanceTimersByTime(IDLE_AFTER_MS);
    expect(isPlayerIdle()).toBe(true);
  });

  it("wakes when the window comes back", () => {
    vi.advanceTimersByTime(IDLE_AFTER_MS);
    playerActive();
    expect(isPlayerIdle()).toBe(false);
  });

  it("runs one timer however often the pointer moves", () => {
    for (let i = 0; i < 200; i++) {
      input();
      vi.advanceTimersByTime(10);
    }
    expect(vi.getTimerCount()).toBe(1);
  });

  it("stops listening", () => {
    stop();
    vi.advanceTimersByTime(IDLE_AFTER_MS);
    input();
    target.dispatchEvent(new Event("blur"));
    expect(vi.getTimerCount()).toBe(0);
    stop = trackPlayerIdle(target);
  });
});
