import {
  ARM_MS,
  guardElapsed,
  guardLeave,
  guardPress,
  guardStart,
  guardSwap,
  type PressGuard,
} from "@pl/ui";
import { describe, expect, it } from "vitest";

/** Presses in order at the given times: whether each counted. */
function presses(start: PressGuard, list: Array<[now: number, detail: number]>): boolean[] {
  let guard = start;
  return list.map(([now, detail]) => {
    const result = guardPress(guard, now, detail);
    guard = result.guard;
    return result.counts;
  });
}

describe("press guard", () => {
  it("lets a control that was there from the start take every press", () => {
    const guard = guardStart("connect", null, 0);
    expect(guard.armed).toBe(true);
    expect(
      presses(guard, [
        [5, 1],
        [150, 2],
        [400, 0],
      ]),
    ).toEqual([true, true, true]);
  });

  it("never lets the rest of a double click land on the control that took the first press's place", () => {
    // "Bağla" at 0 ms, "İptal" in its place at 120 ms, the second click at 250 ms.
    const cancel = guardSwap(guardStart("connect", null, 0), "cancel", 120);
    expect(presses(cancel, [[250, 2]])).toEqual([false]);
    // A slow double-click setting: the second click still counts as the double click's.
    expect(presses(cancel, [[1100, 2]])).toEqual([false]);
    // A triple click is one gesture too.
    expect(
      presses(cancel, [
        [250, 2],
        [1100, 3],
      ]),
    ).toEqual([false, false]);
  });

  it("takes a press under a resting pointer once the moment has passed", () => {
    // E2E2: the pointer stayed where "Bağla" was pressed; "İptal" must still work.
    const cancel = guardSwap(guardStart("connect", null, 0), "cancel", 300);
    expect(presses(cancel, [[300 + ARM_MS - 1, 1]])).toEqual([false]);
    expect(presses(cancel, [[300 + ARM_MS, 1]])).toEqual([true]);
    expect(guardElapsed(cancel, 300 + ARM_MS).armed).toBe(true);
    expect(guardElapsed(cancel, 300 + ARM_MS - 1).armed).toBe(false);
  });

  it("takes a press at once after the pointer has left the control", () => {
    const cancel = guardLeave(guardSwap(guardStart("connect", null, 0), "cancel", 100));
    expect(presses(cancel, [[200, 1]])).toEqual([true]);
  });

  it("ignores the keyboard for a moment too (Enter pressed twice)", () => {
    const cancel = guardSwap(guardStart("connect", null, 0), "cancel", 0);
    expect(
      presses(cancel, [
        [200, 0],
        [ARM_MS + 10, 0],
      ]),
    ).toEqual([false, true]);
  });

  it("lets the player double-click the new control on purpose", () => {
    const cancel = guardSwap(guardStart("connect", null, 0), "cancel", 0);
    expect(
      presses(cancel, [
        [2000, 1],
        [2150, 2],
      ]),
    ).toEqual([true, true]);
  });

  it("starts over when the control's role changes again", () => {
    // "İptal" pressed at 2 s, "Bağla" back in its place at 2.05 s: its double click's second
    // press must not connect again.
    let guard = guardSwap(guardStart("connect", null, 0), "cancel", 0);
    const first = guardPress(guard, 2000, 1);
    expect(first.counts).toBe(true);
    guard = guardSwap(first.guard, "connect", 2050);
    expect(
      presses(guard, [
        [2150, 2],
        [2400, 1],
        [2050 + ARM_MS, 1],
      ]),
    ).toEqual([false, false, true]);
  });

  it("keeps the guard when the role stays the same", () => {
    const guard = guardSwap(guardStart("cancel", null, 0), "cancel", 500);
    expect(guard.since).toBeNull();
    expect(guard.armed).toBe(true);
  });

  it("starts a control that appeared in another's place from when it did", () => {
    // A job's bar: the job started 100 ms ago, or long ago (navigated back to it).
    expect(guardStart("job", 900, 1000).armed).toBe(false);
    expect(presses(guardStart("job", 900, 1000), [[1000, 2]])).toEqual([false]);
    expect(guardStart("job", 0, ARM_MS).armed).toBe(true);
    expect(presses(guardStart("job", 0, 5000), [[5000, 1]])).toEqual([true]);
  });
});
