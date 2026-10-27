import { describe, expect, it } from "vitest";
import { createHold, HOLD_MS, HOLD_REPEAT_MS, type Timers } from "./hold";
import type { MoveOp } from "./model";

/** Timers that run only when the test moves the clock. */
function clock() {
  let now = 0;
  let id = 0;
  const due = new Map<number, { at: number; run: () => void }>();
  const timers: Timers = {
    set: (run, ms) => {
      id += 1;
      due.set(id, { at: now + ms, run });
      return id;
    },
    clear: (handle) => {
      due.delete(handle);
    },
  };
  const advance = (ms: number) => {
    const until = now + ms;
    for (;;) {
      const next = [...due.entries()].sort((a, b) => a[1].at - b[1].at)[0];
      if (!next || next[1].at > until) break;
      due.delete(next[0]);
      now = next[1].at;
      next[1].run();
    }
    now = until;
  };
  return { timers, advance };
}

describe("a direction button", () => {
  it("steps one block on a tap", () => {
    const sent: MoveOp[] = [];
    const { timers, advance } = clock();
    const hold = createHold((op) => sent.push(op), timers);
    hold.press("forward");
    advance(HOLD_MS - 100);
    hold.release("forward");
    advance(5000);
    expect(sent).toEqual([{ action: "step", dir: "forward", n: 1 }]);
    expect(hold.active).toBeNull();
  });

  it("asks for eight blocks while held, again every 1.5 s, and stops when let go", () => {
    const sent: MoveOp[] = [];
    const { timers, advance } = clock();
    const hold = createHold((op) => sent.push(op), timers);
    hold.press("left");
    hold.press("left"); // a key's auto-repeat changes nothing
    advance(HOLD_MS);
    expect(sent).toEqual([{ action: "step", dir: "left", n: 8 }]);
    advance(HOLD_REPEAT_MS * 2);
    expect(sent).toHaveLength(3);
    hold.release("left");
    expect(sent.at(-1)).toEqual({ action: "stop" });
    advance(HOLD_REPEAT_MS * 3);
    expect(sent).toHaveLength(4);
  });

  it("ends the first direction when another one is pressed, and a lost press only stops a hold", () => {
    const sent: MoveOp[] = [];
    const { timers, advance } = clock();
    const hold = createHold((op) => sent.push(op), timers);
    hold.press("forward");
    advance(HOLD_MS + 10);
    hold.press("right");
    expect(sent).toEqual([{ action: "step", dir: "forward", n: 8 }, { action: "stop" }]);
    hold.release("forward"); // the old one's key coming up later changes nothing
    expect(hold.active).toBe("right");
    hold.cancel();
    expect(sent).toHaveLength(2);
    hold.press("back");
    advance(HOLD_MS + 10);
    hold.cancel();
    expect(sent.at(-1)).toEqual({ action: "stop" });
  });
});
