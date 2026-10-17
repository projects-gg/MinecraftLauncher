import { describe, expect, it } from "vitest";
import { type AwayWindow, isWindowAway, refreshWindowAway, trackWindowAway } from "./window-away";

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

const settle = () => new Promise((r) => setTimeout(r, 0));

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

  it("follows a minimize that comes without a focus change, but not resizes in front", async () => {
    const w = fakeWindow();
    const stop = trackWindowAway(w.win);
    w.focus(true);
    w.state.minimized = true;
    w.resize();
    await settle();
    expect(isWindowAway()).toBe(false);
    w.focus(false);
    await settle();
    expect(isWindowAway()).toBe(true);
    w.state.minimized = false;
    w.resize();
    await settle();
    expect(isWindowAway()).toBe(false);
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
});
