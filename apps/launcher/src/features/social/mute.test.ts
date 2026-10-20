import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { CrewNotifications } from "@/lib/api/social-types";
import {
  crewCallsHeard,
  MUTE_PRESETS,
  mutedNow,
  muteEndDay,
  muteUntilFor,
  nextMorning,
  nextMuteTick,
  timeDative,
} from "./mute";

const muted = (muteUntil: number | null, calls = true): CrewNotifications => ({
  calls,
  muted: true,
  muteUntil,
});

describe("crew mute presets", () => {
  const zone = process.env.TZ;
  beforeAll(() => {
    // A zone with summer time (Turkey has none): Germany moves its clocks on 29 March and
    // 25 October 2026.
    process.env.TZ = "Europe/Berlin";
  });
  afterAll(() => {
    if (zone === undefined) delete process.env.TZ;
    else process.env.TZ = zone;
  });

  it("runs in the zone the clock changes are about", () => {
    expect(new Date(Date.UTC(2026, 6, 1, 12)).getTimezoneOffset()).toBe(-120);
    expect(new Date(Date.UTC(2026, 0, 1, 12)).getTimezoneOffset()).toBe(-60);
  });

  it("offers the owner's four, in order", () => {
    expect(MUTE_PRESETS).toEqual(["hour", "eightHours", "morning", "forever"]);
  });

  it("ends hours from the next whole minute, and never for 'Ben açana kadar'", () => {
    const now = Date.UTC(2026, 9, 21, 11, 17, 42); // 13:17:42 in Berlin
    expect(muteUntilFor("hour", now)).toBe(Date.UTC(2026, 9, 21, 12, 18) / 1000);
    expect(muteUntilFor("eightHours", now)).toBe(Date.UTC(2026, 9, 21, 19, 18) / 1000);
    const onTheMinute = Date.UTC(2026, 9, 21, 11, 17);
    expect(muteUntilFor("hour", onTheMinute)).toBe(Date.UTC(2026, 9, 21, 12, 17) / 1000);
    expect(muteUntilFor("forever", now)).toBeNull();
  });

  it("ends 'Yarın sabaha kadar' at the next local 08:00, across midnight", () => {
    // 23:30 → tomorrow 08:00; 00:30 → this morning's; 07:59 → in a minute; 08:00 → tomorrow's.
    expect(nextMorning(Date.UTC(2026, 9, 20, 21, 30))).toBe(Date.UTC(2026, 9, 21, 6));
    expect(nextMorning(Date.UTC(2026, 9, 20, 22, 30))).toBe(Date.UTC(2026, 9, 21, 6));
    expect(nextMorning(Date.UTC(2026, 9, 21, 5, 59))).toBe(Date.UTC(2026, 9, 21, 6));
    expect(nextMorning(Date.UTC(2026, 9, 21, 6))).toBe(Date.UTC(2026, 9, 22, 6));
    expect(muteUntilFor("morning", Date.UTC(2026, 9, 20, 21, 30))).toBe(
      Date.UTC(2026, 9, 21, 6) / 1000,
    );
  });

  it("keeps 08:00 on the wall clock when the clocks change overnight", () => {
    // Spring: 23:00 CET on 28 March; 08:00 is summer time, 8 hours later, not 9.
    const spring = Date.UTC(2026, 2, 28, 22);
    expect(nextMorning(spring)).toBe(Date.UTC(2026, 2, 29, 6));
    expect(nextMorning(spring) - spring).toBe(8 * 3_600_000);
    // Autumn: 23:00 CEST on 24 October; 08:00 is winter time, 10 hours later.
    const autumn = Date.UTC(2026, 9, 24, 21);
    expect(nextMorning(autumn)).toBe(Date.UTC(2026, 9, 25, 7));
    expect(nextMorning(autumn) - autumn).toBe(10 * 3_600_000);
    // The hour that repeats (02:00-03:00 twice) still leads to that morning's 08:00.
    expect(nextMorning(Date.UTC(2026, 9, 25, 0, 30))).toBe(Date.UTC(2026, 9, 25, 7));
    expect(nextMorning(Date.UTC(2026, 9, 25, 1, 30))).toBe(Date.UTC(2026, 9, 25, 7));
  });

  it("names the day a mute ends on, by the local calendar", () => {
    const evening = Date.UTC(2026, 9, 20, 21); // 23:00
    expect(muteEndDay(Date.UTC(2026, 9, 20, 21, 50), evening)).toBe("today");
    expect(muteEndDay(Date.UTC(2026, 9, 20, 23, 30), evening)).toBe("tomorrow"); // 01:30
    expect(muteEndDay(Date.UTC(2026, 9, 21, 6), evening)).toBe("tomorrow");
    expect(muteEndDay(Date.UTC(2026, 9, 22, 6), evening)).toBe("later");
    // The night the clocks go back: 25 October is still one day after the 24th.
    expect(muteEndDay(Date.UTC(2026, 9, 25, 7), Date.UTC(2026, 9, 24, 21))).toBe("tomorrow");
  });
});

describe("crew mute state", () => {
  const now = 1_792_600_000_000;
  const at = now / 1000;

  it("ends a timed mute by the clock, never one until unmuted", () => {
    expect(mutedNow(undefined, now)).toBe(false);
    expect(mutedNow({ calls: true, muted: false, muteUntil: null }, now)).toBe(false);
    expect(mutedNow(muted(at + 60), now)).toBe(true);
    expect(mutedNow(muted(at), now)).toBe(false);
    expect(mutedNow(muted(null), now + 365 * 86_400_000)).toBe(true);
  });

  it("hears crew calls by the service's settings where it keeps them, else this computer's", () => {
    expect(crewCallsHeard(undefined, true, now)).toBe(true);
    expect(crewCallsHeard(undefined, false, now)).toBe(false);
    // The service's switch wins over a stale local one.
    expect(crewCallsHeard({ calls: true, muted: false, muteUntil: null }, false, now)).toBe(true);
    expect(crewCallsHeard({ calls: false, muted: false, muteUntil: null }, true, now)).toBe(false);
    expect(crewCallsHeard(muted(at + 60), true, now)).toBe(false);
    expect(crewCallsHeard(muted(at - 60), true, now)).toBe(true);
    expect(crewCallsHeard(muted(at - 60, false), true, now)).toBe(false);
  });

  it("needs one timer, to a timed mute's end, and none otherwise", () => {
    expect(nextMuteTick(muted(at + 90), now)).toBe(90_000);
    expect(nextMuteTick(muted(at - 1), now)).toBeNull();
    expect(nextMuteTick(muted(null), now)).toBeNull();
    expect(nextMuteTick({ calls: true, muted: false, muteUntil: null }, now)).toBeNull();
    expect(nextMuteTick(undefined, now)).toBeNull();
  });
});

describe("Turkish time endings", () => {
  it("follow the minutes' last word, or the hour's on the hour", () => {
    const read = (hour: number, minute: number) =>
      `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}'${timeDative(hour, minute)}`;
    expect(
      [
        [14, 30],
        [14, 0],
        [16, 0],
        [12, 0],
        [20, 0],
        [10, 0],
        [19, 0],
        [8, 0],
        [0, 0],
        [9, 45],
        [9, 50],
        [21, 40],
        [23, 7],
        [1, 9],
        [13, 1],
        [6, 2],
        [17, 3],
        [18, 6],
        [22, 10],
        [11, 20],
      ].map(([hour, minute]) => read(hour ?? 0, minute ?? 0)),
    ).toEqual([
      "14:30'a",
      "14:00'e",
      "16:00'ya",
      "12:00'ye",
      "20:00'ye",
      "10:00'a",
      "19:00'a",
      "08:00'e",
      "00:00'a",
      "09:45'e",
      "09:50'ye",
      "21:40'a",
      "23:07'ye",
      "01:09'a",
      "13:01'e",
      "06:02'ye",
      "17:03'e",
      "18:06'ya",
      "22:10'a",
      "11:20'ye",
    ]);
  });
});
