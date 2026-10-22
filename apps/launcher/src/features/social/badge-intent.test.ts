import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  blurChip,
  closeCard,
  enterCard,
  focusChip,
  GRACE_MS,
  holdCard,
  hoverChip,
  leaveCard,
  leaveChip,
  OPEN_DELAY_MS,
  pressChip,
  resetBadgeIntent,
  SKIP_MS,
  useBadgeIntent,
} from "./badge-intent";

const state = () => useBadgeIntent.getState();

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(Date.UTC(2026, 9, 22, 18, 0));
  resetBadgeIntent();
});

afterEach(() => {
  resetBadgeIntent();
  vi.useRealTimers();
});

describe("a tag's card under the pointer", () => {
  it("opens after the pointer rests on the chip, as tooltips do", () => {
    hoverChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS - 1);
    expect(state().open).toBeNull();
    vi.advanceTimersByTime(1);
    expect(state()).toMatchObject({ open: "a", instant: false, pinned: false });
  });

  it("never opens for a pointer that only passes over the chip", () => {
    hoverChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS / 2);
    leaveChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    expect(state().open).toBeNull();
  });

  it("goes chip to chip at once and without its entrance", () => {
    hoverChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    leaveChip("a");
    hoverChip("b");
    expect(state()).toMatchObject({ open: "b", instant: true });
  });

  it("opens the next chip's card at once shortly after one closed", () => {
    hoverChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    closeCard("a");
    vi.advanceTimersByTime(SKIP_MS - 10);
    hoverChip("b");
    expect(state()).toMatchObject({ open: "b", instant: true });
    // Later, the delay again.
    closeCard("b");
    vi.advanceTimersByTime(SKIP_MS + 10);
    hoverChip("c");
    expect(state().open).toBeNull();
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    expect(state()).toMatchObject({ open: "c", instant: false });
  });

  it("stays open on the way to the card and closes a moment after both are left", () => {
    hoverChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    leaveChip("a");
    vi.advanceTimersByTime(GRACE_MS - 10);
    enterCard("a");
    vi.advanceTimersByTime(GRACE_MS * 3);
    expect(state().open).toBe("a");
    leaveCard("a");
    vi.advanceTimersByTime(GRACE_MS - 1);
    expect(state().open).toBe("a");
    vi.advanceTimersByTime(1);
    expect(state().open).toBeNull();
  });

  it("comes back to the chip without closing", () => {
    hoverChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    leaveCard("a");
    hoverChip("a");
    vi.advanceTimersByTime(GRACE_MS * 2);
    expect(state().open).toBe("a");
  });

  it("keeps a card open while its menu is open or its request goes", () => {
    hoverChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    holdCard("a", true);
    leaveCard("a");
    vi.advanceTimersByTime(GRACE_MS * 3);
    expect(state().open).toBe("a");
    holdCard("a", false);
    leaveCard("a");
    vi.advanceTimersByTime(GRACE_MS);
    expect(state().open).toBeNull();
  });
});

describe("a press on a chip", () => {
  it("pins the card: leaving closes nothing, another chip's hover changes nothing", () => {
    pressChip("a", false);
    expect(state()).toMatchObject({ open: "a", pinned: true, focus: null });
    leaveChip("a");
    vi.advanceTimersByTime(GRACE_MS * 3);
    hoverChip("b");
    vi.advanceTimersByTime(OPEN_DELAY_MS * 2);
    expect(state().open).toBe("a");
  });

  it("closes a pinned card when pressed again", () => {
    pressChip("a", false);
    pressChip("a", false);
    expect(state().open).toBeNull();
  });

  it("pins a preview instead of closing it", () => {
    hoverChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    pressChip("a", false);
    expect(state()).toMatchObject({ open: "a", pinned: true });
  });

  it("asks for the focus in the card from the keyboard, once per press", () => {
    pressChip("a", true);
    const first = state().focus;
    expect(first?.id).toBe("a");
    pressChip("a", true);
    expect(state().focus?.n).toBe((first?.n ?? 0) + 1);
    // A hover-opened card leaves the focus alone.
    closeCard("a");
    vi.advanceTimersByTime(SKIP_MS * 2);
    hoverChip("b");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    expect(state().focus).toBeNull();
  });
});

describe("the keyboard's focus on a chip", () => {
  it("opens the card as a preview after the delay, unless the focus moved on", () => {
    focusChip("a");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    expect(state()).toMatchObject({ open: "a", pinned: false, focus: null });
    closeCard("a");
    vi.advanceTimersByTime(SKIP_MS * 2);
    focusChip("b");
    blurChip("b");
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    expect(state().open).toBeNull();
  });
});
