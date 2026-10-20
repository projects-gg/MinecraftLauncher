import { describe, expect, it } from "vitest";
import type { CrewCall } from "@/lib/api/social-types";
import {
  answersBy,
  CALL_GAP_MS,
  callWaitUntil,
  liveCalls,
  minutesLeft,
  myCall,
  nextCallTick,
  theirCalls,
  unansweredCall,
  waitMinutes,
} from "./calls";

const NOW = 1_792_521_000_000;
const s = (ms: number) => Math.floor(ms / 1000);

function call(id: string, extra: Partial<CrewCall> = {}): CrewCall {
  return {
    id,
    from: "7",
    mine: false,
    kind: "ready",
    startedAt: s(NOW) - 60,
    endsAt: s(NOW) + 19 * 60,
    answers: [],
    ...extra,
  };
}

describe("calls on the launcher's own clock", () => {
  it("hides a call when its time is up, with no message", () => {
    const calls = [call("a"), call("b", { endsAt: s(NOW) })];
    expect(liveCalls(calls, NOW).map((c) => c.id)).toEqual(["a"]);
    expect(liveCalls(calls, NOW + 19 * 60_000).map((c) => c.id)).toEqual([]);
  });

  it("says the minutes left, rounded up and never 0 while on", () => {
    const one = call("a", { endsAt: s(NOW) + 17 * 60 });
    expect(minutesLeft(one, NOW)).toBe(17);
    expect(minutesLeft(one, NOW + 1)).toBe(17);
    expect(minutesLeft(one, NOW + 60_001)).toBe(16);
    expect(minutesLeft(one, NOW + 17 * 60_000 - 5)).toBe(1);
  });

  it("wakes up only when a minute changes or a call ends, and never without a call", () => {
    expect(nextCallTick([], NOW)).toBeNull();
    expect(nextCallTick([call("a", { endsAt: s(NOW) - 1 })], NOW)).toBeNull();
    // 17 minutes and 20 seconds left: the label changes in 20 seconds.
    expect(nextCallTick([call("a", { endsAt: s(NOW) + 17 * 60 + 20 })], NOW)).toBe(20_000);
    // On the minute: the next change is a whole minute away.
    expect(nextCallTick([call("a", { endsAt: s(NOW) + 5 * 60 })], NOW)).toBe(60_000);
    // Two calls: the sooner change.
    expect(
      nextCallTick(
        [call("a", { endsAt: s(NOW) + 5 * 60 }), call("b", { endsAt: s(NOW) + 50 })],
        NOW,
      ),
    ).toBe(50_000);
  });
});

describe("whose call", () => {
  const mine = call("m", { mine: true, from: "100" });
  const older = call("o", { startedAt: s(NOW) - 300 });
  const newer = call("n", { startedAt: s(NOW) - 30, myAnswer: "soon" });

  it("tells the player's own call from teammates' (newest first)", () => {
    expect(myCall([older, mine], NOW)?.id).toBe("m");
    expect(theirCalls([older, mine, newer], NOW).map((c) => c.id)).toEqual(["n", "o"]);
  });

  it("finds a call the player has not answered nor closed here", () => {
    expect(unansweredCall([newer, older], NOW)?.id).toBe("o");
    expect(unansweredCall([newer, older], NOW, ["o"])).toBeUndefined();
    expect(unansweredCall([mine], NOW)).toBeUndefined();
  });

  it("adds up the answers in the crew's order", () => {
    const answered = call("a", {
      answers: [
        { id: "1", answer: "yes" },
        { id: "2", answer: "no" },
        { id: "3", answer: "yes" },
        { id: "4", answer: "soon" },
      ],
    });
    expect(answersBy(answered)).toEqual({ yes: ["1", "3"], soon: ["4"], no: ["2"] });
  });
});

describe("calling again", () => {
  it("waits five minutes after the player's last call, or what the service said", () => {
    const started = NOW - 60_000;
    const mine = call("m", { mine: true, startedAt: s(started) });
    expect(callWaitUntil([mine], 0, null)).toBe(s(started) * 1000 + CALL_GAP_MS);
    expect(callWaitUntil([], NOW + 9 * 60_000, null)).toBe(NOW + 9 * 60_000);
    expect(callWaitUntil([], 0, started)).toBe(started + CALL_GAP_MS);
    expect(callWaitUntil([], 0, null)).toBe(0);
    expect(waitMinutes(NOW + 4 * 60_000 - 1, NOW)).toBe(4);
    expect(waitMinutes(NOW + 1, NOW)).toBe(1);
  });
});
