import { countdownCells, countdownTickMs } from "@pl/ui";
import { describe, expect, it } from "vitest";

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

describe("countdownCells", () => {
  it("shows days, hours and minutes while days are left", () => {
    expect(countdownCells(2 * DAY + 3 * HOUR + 4 * MINUTE + 5 * SECOND)).toEqual([
      [2, "days"],
      [3, "hours"],
      [4, "minutes"],
    ]);
    // Hours stay at zero while days are left.
    expect(countdownCells(DAY + 4 * MINUTE)).toEqual([
      [1, "days"],
      [0, "hours"],
      [4, "minutes"],
    ]);
  });

  it("shows hours and minutes on the last day, never seconds", () => {
    expect(countdownCells(DAY - 1)).toEqual([
      [23, "hours"],
      [59, "minutes"],
    ]);
    expect(countdownCells(2 * HOUR + 14 * MINUTE + 57 * SECOND)).toEqual([
      [2, "hours"],
      [14, "minutes"],
    ]);
  });

  it("shows the minutes alone in the last hour and the seconds in the last minute", () => {
    expect(countdownCells(43 * MINUTE + 8 * SECOND + 400)).toEqual([[43, "minutes"]]);
    expect(countdownCells(MINUTE)).toEqual([[1, "minutes"]]);
    expect(countdownCells(MINUTE - 1)).toEqual([[59, "seconds"]]);
  });

  it("stays at zero seconds once the moment has come", () => {
    expect(countdownCells(-5 * SECOND)).toEqual([[0, "seconds"]]);
  });
});

describe("countdownTickMs", () => {
  it("renders once a minute until the last minute", () => {
    expect(countdownTickMs(2 * DAY + 30 * SECOND + 250)).toBe(30_251);
    // 2 h 14 min 57 s: the next change is the minute, 57 s away (never every second).
    expect(countdownTickMs(2 * HOUR + 14 * MINUTE + 57 * SECOND)).toBe(57_001);
  });

  it("brings the seconds in as the last minute begins", () => {
    // Exactly one minute left still shows "01 dakika"; a millisecond later it is 59 seconds.
    expect(countdownTickMs(MINUTE)).toBe(1);
    expect(countdownCells(MINUTE - 1)).toEqual([[59, "seconds"]]);
  });

  it("renders when the seconds change in the last minute", () => {
    // 4.3 s left shows 04; 03 from 3.999 s.
    expect(countdownTickMs(4300)).toBe(301);
    expect(countdownTickMs(4000)).toBe(1);
  });

  it("stops once the moment has come", () => {
    expect(countdownTickMs(0)).toBeNull();
    expect(countdownTickMs(-1)).toBeNull();
  });
});
