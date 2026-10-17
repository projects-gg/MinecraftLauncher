import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { READY_FOR_MS, SHOW_AFTER_MS, usePrepare } from "./prepare";

const preparing = (percent: number, instanceId = "projects") =>
  ({ state: "preparing", instanceId, percent }) as const;
const ready = (instanceId = "projects") => ({ state: "ready", instanceId }) as const;
const idle = { state: "idle" } as const;
const receive = usePrepare.getState().receive;
const hint = () => usePrepare.getState().hint;

describe("usePrepare", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    receive(idle);
    usePrepare.setState({ hint: null });
  });
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it("shows a preparation only once it lasts, with its latest figure", () => {
    receive(preparing(3));
    expect(hint()).toBeNull();
    vi.advanceTimersByTime(SHOW_AFTER_MS / 2);
    receive(preparing(9));
    expect(hint()).toBeNull();
    vi.advanceTimersByTime(SHOW_AFTER_MS / 2);
    expect(hint()).toEqual({ instanceId: "projects", kind: "preparing", percent: 9 });
    receive(preparing(42));
    expect(hint()).toEqual({ instanceId: "projects", kind: "preparing", percent: 42 });
  });

  it("never flickers for a quick look", () => {
    receive(preparing(0));
    receive(ready());
    vi.advanceTimersByTime(SHOW_AFTER_MS + READY_FOR_MS);
    expect(hint()).toBeNull();
    receive(preparing(0));
    receive(idle);
    vi.advanceTimersByTime(SHOW_AFTER_MS);
    expect(hint()).toBeNull();
  });

  it("says ready for a moment after a preparation the player saw", () => {
    receive(preparing(10));
    vi.advanceTimersByTime(SHOW_AFTER_MS);
    receive(ready());
    expect(hint()).toEqual({ instanceId: "projects", kind: "ready" });
    vi.advanceTimersByTime(READY_FOR_MS - 1);
    expect(hint()?.kind).toBe("ready");
    vi.advanceTimersByTime(1);
    expect(hint()).toBeNull();
  });

  it("goes quiet when a preparation pauses", () => {
    receive(preparing(10));
    vi.advanceTimersByTime(SHOW_AFTER_MS);
    receive(idle);
    expect(hint()).toBeNull();
    // Ready for something the bar no longer showed says nothing.
    receive(ready());
    expect(hint()).toBeNull();
  });

  it("follows the instance a preparation belongs to", () => {
    receive(preparing(10, "a"));
    vi.advanceTimersByTime(SHOW_AFTER_MS);
    receive(ready("b"));
    expect(hint()).toEqual({ instanceId: "a", kind: "preparing", percent: 10 });
    receive(preparing(5, "b"));
    // Another instance's run waits its own delay; the old line stays until a status replaces it.
    expect(hint()?.instanceId).toBe("a");
    vi.advanceTimersByTime(SHOW_AFTER_MS);
    expect(hint()).toEqual({ instanceId: "b", kind: "preparing", percent: 5 });
  });
});
