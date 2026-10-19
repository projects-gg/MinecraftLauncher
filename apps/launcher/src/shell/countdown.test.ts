import { countdownCells, countdownTickMs } from "@pl/ui";
import { describe, expect, it } from "vitest";

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

describe("countdownCells", () => {
  it("shows days, hours and minutes while days are left", () => {
    expect(countdownCells(2 * DAY + 3 * HOUR + 4 * MINUTE + 5 * SECOND)).toEqual({
      days: 2,
      hours: 3,
      minutes: 4,
      seconds: null,
    });
  });

  it("shows the seconds on the last day, hours even at zero", () => {
    expect(countdownCells(DAY - 1)).toEqual({ days: 0, hours: 23, minutes: 59, seconds: 59 });
    expect(countdownCells(43 * MINUTE + 8 * SECOND + 400)).toEqual({
      days: 0,
      hours: 0,
      minutes: 43,
      seconds: 8,
    });
  });

  it("stays at zero once the moment has come", () => {
    expect(countdownCells(-5 * SECOND)).toEqual({ days: 0, hours: 0, minutes: 0, seconds: 0 });
  });
});

describe("countdownTickMs", () => {
  it("renders again when the seconds change on the last day", () => {
    // 4.3 s left shows 04; 03 from 2.999 s.
    expect(countdownTickMs(4300)).toBe(301);
    expect(countdownTickMs(4000)).toBe(1);
  });

  it("renders once a minute while days are left", () => {
    expect(countdownTickMs(2 * DAY + 30 * SECOND + 250)).toBe(30_251);
  });

  it("brings the seconds in as the last day begins", () => {
    // Exactly one day left still shows "01 gün"; a millisecond later it is 23:59:59.
    expect(countdownTickMs(DAY)).toBe(1);
    expect(countdownCells(DAY - 1).seconds).toBe(59);
  });

  it("stops once the moment has come", () => {
    expect(countdownTickMs(0)).toBeNull();
    expect(countdownTickMs(-1)).toBeNull();
  });
});
