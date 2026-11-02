import { describe, expect, it } from "vitest";
import { SpeakingReporter } from "./speaking";
import { FakeTimers } from "./testing";

describe("SpeakingReporter", () => {
  it("says changes only, at most ten a second, the last one always", () => {
    const timers = new FakeTimers();
    const sent: [string[], boolean][] = [];
    const reporter = new SpeakingReporter((ids, me) => sent.push([ids, me]), timers);
    reporter.set(["u2"], false);
    expect(sent).toEqual([[["u2"], false]]);
    timers.advance(10);
    reporter.set(["u2", "u3"], false);
    reporter.set(["u3", "u2"], true);
    // Within the 100 ms: one timer for the rest of it.
    expect(sent).toHaveLength(1);
    expect(timers.pending).toBe(1);
    timers.advance(90);
    expect(sent).toEqual([
      [["u2"], false],
      [["u2", "u3"], true],
    ]);
    // The same again: nothing, and no timer.
    timers.advance(500);
    reporter.set(["u3", "u2"], true);
    expect(sent).toHaveLength(2);
    expect(timers.pending).toBe(0);
  });

  it("flickers that end where they began are not sent", () => {
    const timers = new FakeTimers();
    const sent: [string[], boolean][] = [];
    const reporter = new SpeakingReporter((ids, me) => sent.push([ids, me]), timers);
    reporter.set(["u2"], false);
    timers.advance(20);
    reporter.set([], false);
    reporter.set(["u2"], false);
    timers.advance(100);
    expect(sent).toEqual([[["u2"], false]]);
  });

  it("clears at the end of a stay, once", () => {
    const timers = new FakeTimers();
    const sent: [string[], boolean][] = [];
    const reporter = new SpeakingReporter((ids, me) => sent.push([ids, me]), timers);
    reporter.clear();
    expect(sent).toEqual([]);
    reporter.set(["u2"], true);
    timers.advance(20);
    reporter.set(["u2", "u4"], true);
    reporter.clear();
    expect(sent).toEqual([
      [["u2"], true],
      [[], false],
    ]);
    expect(timers.pending).toBe(0);
  });
});
