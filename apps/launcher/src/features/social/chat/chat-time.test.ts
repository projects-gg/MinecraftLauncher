import { afterAll, beforeAll, describe, expect, it } from "vitest";
import i18n, { loadEnglish } from "@/lib/i18n";
import {
  clockTime,
  dayLineText,
  daysAgo,
  fullTime,
  listStamp,
  localDay,
  sameLocalDay,
} from "./chat-time";

const TR = { today: "Bugün", yesterday: "Dün" };
const EN = { today: "Today", yesterday: "Yesterday" };
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/** A wall time on this computer's clock (month from 1), read inside a test, after its zone is set. */
function local(year: number, month: number, day: number, hour = 12, minute = 0): number {
  return new Date(year, month - 1, day, hour, minute).getTime();
}

/** The block's tests run with this computer's zone set to `zone` (Node reads TZ again at once). */
function inZone(zone: string) {
  const before = process.env.TZ;
  beforeAll(() => {
    process.env.TZ = zone;
  });
  afterAll(() => {
    if (before === undefined) delete process.env.TZ;
    else process.env.TZ = before;
  });
}

describe("chat times in Istanbul", () => {
  inZone("Europe/Istanbul");

  it("runs in Istanbul's zone (no summer time)", () => {
    expect(new Date(Date.UTC(2026, 6, 1, 12)).getTimezoneOffset()).toBe(-180);
    expect(new Date(Date.UTC(2026, 0, 1, 12)).getTimezoneOffset()).toBe(-180);
  });

  it("says today, yesterday, two to six days ago, then the date", () => {
    const now = local(2026, 10, 31, 12); // Saturday
    expect(dayLineText(local(2026, 10, 31, 0, 5), now, TR)).toBe("Bugün");
    expect(dayLineText(local(2026, 10, 30, 23, 59), now, TR)).toBe("Dün");
    expect(dayLineText(local(2026, 10, 29, 12), now, TR)).toBe("2 gün önce");
    expect(dayLineText(local(2026, 10, 28, 9), now, TR)).toBe("3 gün önce");
    expect(dayLineText(local(2026, 10, 25, 23, 59), now, TR)).toBe("6 gün önce");
    expect(dayLineText(local(2026, 10, 24, 23, 59), now, TR)).toBe("24 Ekim");
    expect(dayLineText(local(2026, 1, 3, 8), now, TR)).toBe("3 Ocak");
    expect(dayLineText(local(2025, 12, 31, 22), now, TR)).toBe("31 Aralık 2025");
  });

  it("counts calendar days, not 24 hours, on both sides of midnight", () => {
    const midnight = local(2026, 10, 31, 0, 0);
    expect(dayLineText(midnight - 1, midnight, TR)).toBe("Dün");
    expect(dayLineText(midnight, midnight, TR)).toBe("Bugün");
    expect(sameLocalDay(midnight - 1, midnight)).toBe(false);
    expect(sameLocalDay(midnight, midnight + 24 * HOUR - 1)).toBe(true);
    // Forty minutes back, yet yesterday.
    expect(dayLineText(local(2026, 10, 30, 23, 30), local(2026, 10, 31, 0, 10), TR)).toBe("Dün");
    // Almost three days of hours, two calendar days.
    expect(daysAgo(local(2026, 10, 29, 0, 10), local(2026, 10, 31, 23, 50))).toBe(2);
  });

  it("turns the year with the calendar", () => {
    const now = local(2027, 1, 1, 0, 30);
    expect(dayLineText(local(2026, 12, 31, 23, 50), now, TR)).toBe("Dün");
    expect(dayLineText(local(2026, 12, 26, 12), now, TR)).toBe("6 gün önce");
    expect(dayLineText(local(2026, 12, 25, 12), now, TR)).toBe("25 Aralık 2026");
    expect(localDay(local(2027, 1, 1, 0, 0)) - localDay(local(2026, 12, 31, 23, 59))).toBe(1);
  });

  it("reads a time a little ahead of the clock as today", () => {
    const now = local(2026, 10, 31, 23, 59);
    // 00:01 tomorrow by the service's clock.
    expect(daysAgo(now + 2 * MINUTE, now)).toBe(-1);
    expect(dayLineText(now + 2 * MINUTE, now, TR)).toBe("Bugün");
  });

  it("writes the clock in 24 hours and the full time with the weekday", () => {
    expect(clockTime(local(2026, 10, 28, 0, 5))).toBe("00:05");
    expect(clockTime(local(2026, 10, 28, 14, 5))).toBe("14:05");
    expect(fullTime(local(2026, 10, 28, 14, 5))).toBe("28 Ekim 2026 Çarşamba 14:05");
    expect(fullTime(local(2025, 12, 31, 23, 59))).toBe("31 Aralık 2025 Çarşamba 23:59");
  });

  it("stamps the list: the time today, Dün, a weekday within the week, then the date", () => {
    const now = local(2026, 10, 31, 12); // Saturday
    expect(listStamp(local(2026, 10, 31, 0, 5), now, "Dün")).toBe("00:05");
    expect(listStamp(local(2026, 10, 30, 23, 59), now, "Dün")).toBe("Dün");
    expect(listStamp(local(2026, 10, 29, 12), now, "Dün")).toBe("Per");
    expect(listStamp(local(2026, 10, 26, 9), now, "Dün")).toBe("Pzt");
    expect(listStamp(local(2026, 10, 25, 23, 59), now, "Dün")).toBe("Paz");
    // A week back is today's weekday again: the date.
    expect(listStamp(local(2026, 10, 24, 23, 59), now, "Dün")).toBe("24 Eki");
    expect(listStamp(local(2026, 1, 3, 8), now, "Dün")).toBe("3 Oca");
    expect(listStamp(local(2025, 12, 31, 22), now, "Dün")).toBe("31 Ara 2025");
    // Midnight and New Year, as the day lines.
    const midnight = local(2026, 10, 31, 0, 0);
    expect(listStamp(midnight - 1, midnight, "Dün")).toBe("Dün");
    expect(listStamp(midnight, midnight, "Dün")).toBe("00:00");
    const newYear = local(2027, 1, 1, 0, 30);
    expect(listStamp(local(2026, 12, 31, 23, 50), newYear, "Dün")).toBe("Dün");
    expect(listStamp(local(2026, 12, 26, 12), newYear, "Dün")).toBe("Cmt");
    expect(listStamp(local(2026, 12, 25, 12), newYear, "Dün")).toBe("25 Ara 2026");
    // Ahead of the clock: today's time.
    expect(listStamp(now + 2 * MINUTE, now, "Dün")).toBe("12:02");
  });
});

describe("chat times in English", () => {
  inZone("Europe/London");
  beforeAll(async () => {
    await loadEnglish();
    await i18n.changeLanguage("en");
  });
  afterAll(async () => {
    await i18n.changeLanguage("tr");
  });

  it("speaks British English", () => {
    const now = local(2026, 10, 31, 12);
    expect(dayLineText(local(2026, 10, 31, 9), now, EN)).toBe("Today");
    expect(dayLineText(local(2026, 10, 30, 9), now, EN)).toBe("Yesterday");
    expect(dayLineText(local(2026, 10, 28, 9), now, EN)).toBe("3 days ago");
    expect(dayLineText(local(2026, 10, 24, 9), now, EN)).toBe("24 October");
    expect(dayLineText(local(2025, 12, 31, 9), now, EN)).toBe("31 December 2025");
    expect(clockTime(local(2026, 10, 28, 14, 5))).toBe("14:05");
    expect(fullTime(local(2026, 10, 28, 14, 5))).toBe("Wednesday, 28 October 2026 at 14:05");
    expect(listStamp(local(2026, 10, 31, 9, 5), now, "Yesterday")).toBe("09:05");
    expect(listStamp(local(2026, 10, 30, 9), now, "Yesterday")).toBe("Yesterday");
    expect(listStamp(local(2026, 10, 26, 9), now, "Yesterday")).toBe("Mon");
    expect(listStamp(local(2026, 10, 24, 9), now, "Yesterday")).toBe("24 Oct");
    expect(listStamp(local(2025, 12, 31, 9), now, "Yesterday")).toBe("31 Dec 2025");
  });
});

describe("chat times across summer time", () => {
  inZone("Europe/Berlin");

  it("runs in Berlin's zone", () => {
    expect(new Date(Date.UTC(2026, 6, 1, 12)).getTimezoneOffset()).toBe(-120);
    expect(new Date(Date.UTC(2026, 0, 1, 12)).getTimezoneOffset()).toBe(-60);
  });

  it("keeps 25 October, 25 hours long, one day", () => {
    const now = local(2026, 10, 26, 0, 30);
    expect(dayLineText(local(2026, 10, 24, 23, 30), now, TR)).toBe("2 gün önce");
    expect(dayLineText(local(2026, 10, 25, 0, 10), now, TR)).toBe("Dün");
    expect(dayLineText(local(2026, 10, 25, 23, 59), now, TR)).toBe("Dün");
    // The clocks went back at 03:00: 02:30 came twice, both on 25 October.
    const first = Date.UTC(2026, 9, 25, 0, 30);
    const second = Date.UTC(2026, 9, 25, 1, 30);
    expect(clockTime(first)).toBe("02:30");
    expect(clockTime(second)).toBe("02:30");
    expect(sameLocalDay(first, second)).toBe(true);
  });

  it("keeps 29 March, 23 hours long, one day", () => {
    const morning = local(2026, 3, 29, 0, 30);
    const night = local(2026, 3, 29, 23, 30);
    expect(night - morning).toBe(22 * HOUR);
    expect(dayLineText(morning, night, TR)).toBe("Bugün");
    expect(dayLineText(local(2026, 3, 28, 23, 59), night, TR)).toBe("Dün");
    expect(clockTime(night)).toBe("23:30");
  });
});

describe("chat times in the reader's own zone", () => {
  const before = process.env.TZ;
  afterAll(() => {
    if (before === undefined) delete process.env.TZ;
    else process.env.TZ = before;
  });

  it("puts one moment on each reader's own day and clock", () => {
    const moment = Date.UTC(2026, 9, 31, 2, 30);
    const later = Date.UTC(2026, 9, 31, 12, 0);
    process.env.TZ = "Europe/Istanbul";
    expect([dayLineText(moment, later, TR), clockTime(moment)]).toEqual(["Bugün", "05:30"]);
    process.env.TZ = "America/New_York";
    expect([dayLineText(moment, later, TR), clockTime(moment)]).toEqual(["Dün", "22:30"]);
    process.env.TZ = "Pacific/Auckland";
    // 15:30 on 31 October there, read at 01:00 on 1 November.
    expect([dayLineText(moment, later, TR), clockTime(moment)]).toEqual(["Dün", "15:30"]);
  });

  it("stamps the list on each reader's own day", () => {
    const moment = Date.UTC(2026, 9, 31, 2, 30);
    const later = Date.UTC(2026, 9, 31, 12, 0);
    process.env.TZ = "Europe/Istanbul";
    expect(listStamp(moment, later, "Dün")).toBe("05:30");
    process.env.TZ = "America/New_York";
    expect(listStamp(moment, later, "Dün")).toBe("Dün");
    process.env.TZ = "Europe/Berlin";
    // Six days back over the 25-hour 25 October: still a weekday ("Paz", 25 October; read 31 October).
    expect(listStamp(Date.UTC(2026, 9, 25, 1, 30), Date.UTC(2026, 9, 31, 12, 0), "Dün")).toBe(
      "Paz",
    );
  });
});
