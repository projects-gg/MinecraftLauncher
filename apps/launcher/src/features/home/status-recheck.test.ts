import { describe, expect, it } from "vitest";
import { nextLook, RECHECK_BEFORE_EXPIRY_MS, RECHECK_MIN_MS, RECHECK_MS } from "./status-recheck";

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const END = Date.parse("2026-10-19T16:24:15Z");

/**
 * The looks a notice on screen makes, as the hook runs them: each timer fires `wait` (plus the
 * timer's own lateness, negative when it fires early by the wall clock) after the one before,
 * and the notice leaves at `END` (the hook stops with it). Times are relative to the expiry.
 */
function looks(arrivesBefore: number, lateness: (n: number) => number = () => 0): number[] {
  const times: number[] = [];
  let now = END - arrivesBefore;
  for (;;) {
    const look = nextLook(END, now);
    if (!look) break;
    now += look.wait + lateness(times.length);
    if (now >= END) break;
    times.push(now - END);
    if (look.last) break;
  }
  return times;
}

describe("status notice rechecks", () => {
  it("looks every five minutes and once a minute before expiry", () => {
    expect(looks(15 * MINUTE)).toEqual([-10 * MINUTE, -5 * MINUTE, -RECHECK_BEFORE_EXPIRY_MS]);
  });

  it("looks once in the last minute, not three times (E2E1)", () => {
    for (const arrival of [15 * MINUTE, 6 * MINUTE, 3 * MINUTE, 61 * SECOND]) {
      const lastMinute = looks(arrival).filter((t) => t >= -RECHECK_BEFORE_EXPIRY_MS);
      expect(lastMinute).toHaveLength(1);
    }
  });

  it("stops after the look before expiry even when its timer fires early", () => {
    // Timers run on a monotonic clock, Date.now() on the wall clock: the last one can seem 2 ms
    // early. Deciding at the clock would plan one more look 30 s later.
    expect(looks(15 * MINUTE, (n) => (n === 2 ? -2 : 0))).toEqual([
      -10 * MINUTE,
      -5 * MINUTE,
      -RECHECK_BEFORE_EXPIRY_MS - 2,
    ]);
    expect(looks(15 * MINUTE, () => 16)).toHaveLength(3);
  });

  it("folds a regular look that would come just before the last one into it", () => {
    // 6.4 minutes left: one look a minute before expiry, not one at 1.4 and one at 0.9 minutes.
    expect(looks(6.4 * MINUTE)).toEqual([-RECHECK_BEFORE_EXPIRY_MS]);
    expect(looks(5.5 * MINUTE + RECHECK_BEFORE_EXPIRY_MS + SECOND)).toEqual([
      -0.5 * MINUTE - RECHECK_BEFORE_EXPIRY_MS - SECOND,
      -RECHECK_BEFORE_EXPIRY_MS,
    ]);
  });

  it("still looks once at a notice that comes in its last minute", () => {
    // Shown again from the home's cache (refreshed at most every five minutes): a renewal on
    // the server would otherwise be missed and the notice would leave while the problem lasts.
    expect(looks(50 * SECOND)).toEqual([-20 * SECOND]);
  });

  it("does not look at a notice that leaves before the shortest wait", () => {
    expect(looks(RECHECK_MIN_MS)).toEqual([]);
    expect(nextLook(END, END - 10 * SECOND)).toBeNull();
    expect(nextLook(Number.NaN, END)).toBeNull();
  });

  it("never looks twice within the shortest wait", () => {
    for (let arrival = 31 * SECOND; arrival <= 16 * MINUTE; arrival += 7 * SECOND) {
      const times = looks(arrival);
      times.reduce((before, time) => {
        expect(time - before).toBeGreaterThanOrEqual(RECHECK_MIN_MS);
        return time;
      }, -arrival);
      // Never more often than every five minutes, apart from the look before expiry.
      expect(times.length).toBeLessThanOrEqual(Math.ceil(arrival / RECHECK_MS) + 1);
    }
  });
});
