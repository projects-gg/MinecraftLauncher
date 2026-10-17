import { describe, expect, it } from "vitest";
import {
  type AwayWindow,
  isWindowAway,
  onWindowAwayChange,
  RESIZE_SETTLE_MS,
  refreshWindowAway,
  trackWindowAway,
} from "./window-away";

/** A window whose answers the test sets; answers can be held back to model a slow round trip. */
function fakeWindow() {
  const state = { visible: true, minimized: false };
  const handlers: { focus?: (e: { payload: boolean }) => void; resize?: () => void } = {};
  let held: (() => void)[] | null = null;
  // The answer is what the window was when asked, even if it arrives later.
  const answer = <T>(value: () => T) =>
    new Promise<T>((resolve) => {
      const now = value();
      const reply = () => resolve(now);
      if (held) held.push(reply);
      else reply();
    });
  const win: AwayWindow = {
    isVisible: () => answer(() => state.visible),
    isMinimized: () => answer(() => state.minimized),
    onFocusChanged: async (handler) => {
      handlers.focus = handler;
      return () => {};
    },
    onResized: async (handler) => {
      handlers.resize = handler;
      return () => {};
    },
  };
  return {
    win,
    state,
    focus: (payload: boolean) => handlers.focus?.({ payload }),
    resize: () => handlers.resize?.(),
    hold: () => {
      held = [];
    },
    release: () => {
      const replies = held ?? [];
      held = null;
      for (const reply of replies) reply();
    },
  };
}

const settle = (ms = 0) => new Promise((r) => setTimeout(r, ms));

describe("trackWindowAway", () => {
  it("is away in the tray from the start and back when the window gains focus", async () => {
    const w = fakeWindow();
    const stop = trackWindowAway(w.win);
    w.state.visible = false;
    await refreshWindowAway();
    expect(isWindowAway()).toBe(true);
    w.state.visible = true;
    w.focus(true);
    expect(isWindowAway()).toBe(false);
    stop();
  });

  it("asks again when focus goes: hidden or minimized is away, merely behind is not", async () => {
    const w = fakeWindow();
    const stop = trackWindowAway(w.win);
    w.focus(true);
    w.focus(false);
    await settle();
    expect(isWindowAway()).toBe(false);
    w.state.minimized = true;
    w.focus(false);
    await settle();
    expect(isWindowAway()).toBe(true);
    w.state.minimized = false;
    w.focus(true);
    w.state.visible = false;
    w.focus(false);
    await settle();
    expect(isWindowAway()).toBe(true);
    stop();
  });

  it("follows a minimize and a restore through resizes, once they settle", async () => {
    const w = fakeWindow();
    const stop = trackWindowAway(w.win);
    w.focus(true);
    // On Windows the window reports a blur right after it was shown; it is still in front.
    w.focus(false);
    await settle();
    expect(isWindowAway()).toBe(false);
    w.state.minimized = true;
    w.resize();
    await settle();
    expect(isWindowAway()).toBe(false);
    await settle(RESIZE_SETTLE_MS + 30);
    expect(isWindowAway()).toBe(true);
    w.state.minimized = false;
    w.resize();
    w.resize();
    await settle(RESIZE_SETTLE_MS + 30);
    expect(isWindowAway()).toBe(false);
    stop();
  });

  it("learns about a hide only when asked, since hiding sends no event", async () => {
    const w = fakeWindow();
    const stop = trackWindowAway(w.win);
    w.focus(true);
    w.state.visible = false;
    await settle(RESIZE_SETTLE_MS + 30);
    expect(isWindowAway()).toBe(false);
    await refreshWindowAway();
    expect(isWindowAway()).toBe(true);
    stop();
  });

  it("drops an answer that arrives after the window came back", async () => {
    const w = fakeWindow();
    const stop = trackWindowAway(w.win);
    w.focus(true);
    w.state.visible = false;
    w.hold();
    w.focus(false);
    w.state.visible = true;
    w.focus(true);
    w.release();
    await settle();
    expect(isWindowAway()).toBe(false);
    stop();
  });

  it("tells subscribers (polling follows it) each change until they unsubscribe", async () => {
    const w = fakeWindow();
    const stop = trackWindowAway(w.win);
    w.focus(true);
    const seen: boolean[] = [];
    const unsubscribe = onWindowAwayChange((away) => seen.push(away));
    // The blur Windows sends right after a show changes nothing.
    w.focus(false);
    await settle();
    w.state.visible = false;
    await refreshWindowAway();
    w.state.visible = true;
    w.focus(true);
    unsubscribe();
    w.state.minimized = true;
    w.focus(false);
    await settle();
    expect(seen).toEqual([true, false]);
    expect(isWindowAway()).toBe(true);
    stop();
  });
});
