import { countdownTickMs } from "@pl/ui";
import { describe, expect, it } from "vitest";

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

describe("countdownTickMs", () => {
  it("changes the words once a minute until the last minute", () => {
    expect(countdownTickMs(2 * DAY + 30 * SECOND + 250)).toBe(30_251);
    // 2 h 14 min 57 s: the next change is the minute, 57 s away (never every second).
    expect(countdownTickMs(2 * HOUR + 14 * MINUTE + 57 * SECOND)).toBe(57_001);
  });

  it("brings the seconds in as the last minute begins", () => {
    // Exactly one minute left still says "1 dakika"; a millisecond later it counts seconds.
    expect(countdownTickMs(MINUTE)).toBe(1);
    expect(countdownTickMs(MINUTE - 1)).toBe(1000);
  });

  it("changes each second in the last minute", () => {
    // 4.3 s left says 4 s; 3 s from 3.999 s.
    expect(countdownTickMs(4300)).toBe(301);
    expect(countdownTickMs(4000)).toBe(1);
  });

  it("stops once the moment has come", () => {
    expect(countdownTickMs(0)).toBeNull();
    expect(countdownTickMs(-1)).toBeNull();
  });
});
