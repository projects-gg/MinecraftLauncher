import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import {
  friendPlayed,
  nextPlayedChange,
  nextPlayedTick,
  PLAYED_TICK_MIN_MS,
  playedFor,
  playedSpoken,
  playedText,
  withPlayed,
} from "./played";

const t = ((key: string, values?: Record<string, unknown>) =>
  values ? `${key} ${JSON.stringify(values)}` : key) as unknown as TFunction;

const START = 1_792_400_100;
/** `seconds` after the session began, in ms. */
const elapsed = (seconds: number) => (START + seconds) * 1000;

describe("how long a friend has been playing (CREW-PLUS)", () => {
  it("says nothing under five minutes, five-minute steps under an hour, then whole hours", () => {
    expect(playedFor(undefined, elapsed(900))).toBeNull();
    expect(playedFor(START, elapsed(-60))).toBeNull();
    expect(playedFor(START, elapsed(299))).toBeNull();
    expect(playedFor(START, elapsed(300))).toEqual({ unit: "minutes", value: 5 });
    expect(playedFor(START, elapsed(1_799))).toEqual({ unit: "minutes", value: 25 });
    expect(playedFor(START, elapsed(3_599))).toEqual({ unit: "minutes", value: 55 });
    expect(playedFor(START, elapsed(3_600))).toEqual({ unit: "hours", value: 1 });
    expect(playedFor(START, elapsed(7_199))).toEqual({ unit: "hours", value: 1 });
    expect(playedFor(START, elapsed(36_000))).toEqual({ unit: "hours", value: 10 });
  });

  it("shows only while the friend plays", () => {
    expect(friendPlayed({ state: "playing", playingSince: START }, elapsed(1_500))).toEqual({
      unit: "minutes",
      value: 25,
    });
    expect(friendPlayed({ state: "launcher", playingSince: START }, elapsed(1_500))).toBeNull();
    expect(friendPlayed({ state: "playing" }, elapsed(1_500))).toBeNull();
    // A crewmate who is a friend reads the same in the crew.
    const member = { id: "7", state: "playing" as const };
    expect(withPlayed(member, { playingSince: START })).toEqual({ ...member, playingSince: START });
    expect(withPlayed(member, undefined)).toBe(member);
    expect(withPlayed(member, {})).toBe(member);
    expect(playedText({ unit: "hours", value: 2 }, t)).toBe('crewPlus.played.hours {"count":2}');
    expect(playedSpoken({ unit: "minutes", value: 5 }, t)).toBe(
      'crewPlus.played.spoken.minutes {"count":5}',
    );
  });

  it("wakes only when a line changes, never more than once a minute", () => {
    expect(nextPlayedChange(START, elapsed(10))).toBe(elapsed(300));
    expect(nextPlayedChange(START, elapsed(301))).toBe(elapsed(600));
    expect(nextPlayedChange(START, elapsed(3_599))).toBe(elapsed(3_600));
    expect(nextPlayedChange(START, elapsed(3_600))).toBe(elapsed(7_200));
    expect(nextPlayedTick([], elapsed(0))).toBeNull();
    // The soonest change of all, at least a minute away.
    expect(nextPlayedTick([START, START - 3_000], elapsed(3_590))).toBe(PLAYED_TICK_MIN_MS);
    expect(nextPlayedTick([START], elapsed(4_000))).toBe((7_200 - 4_000) * 1000);
  });
});
