import { emberHue, particleFade, snowTopFade } from "@pl/ui";
import { describe, expect, it } from "vitest";
import {
  type ClockEnv,
  DEFAULT_SCRIM,
  EVENING_GRADE,
  homeParticles,
  MAX_TIMER_MS,
  NIGHT_GRADE,
  seasonDue,
  seasonLook,
  seasonWait,
  startSeasonClock,
} from "./season";

/** A page whose time, timers and window the test moves by hand. */
function fakeClock(start: number) {
  let now = start;
  let visible = true;
  let nextId = 1;
  const timers = new Map<number, { at: number; run: () => void }>();
  const watchers = new Set<() => void>();
  const env: ClockEnv = {
    now: () => now,
    setTimeout: (run, ms) => {
      const id = nextId++;
      timers.set(id, { at: now + ms, run });
      return id;
    },
    clearTimeout: (id) => {
      timers.delete(id);
    },
    visible: () => visible,
    onVisible: (run) => {
      watchers.add(run);
      return () => watchers.delete(run);
    },
  };
  return {
    env,
    timers,
    watchers,
    /** Moves the clock on, running the timers that come due on the way, in order. */
    advance(ms: number) {
      const end = now + ms;
      for (;;) {
        const due = [...timers.entries()]
          .filter(([, t]) => t.at <= end)
          .sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        timers.delete(due[0]);
        now = due[1].at;
        due[1].run();
      }
      now = end;
    },
    /** Jumps the wall clock (sleep, a clock change) without running timers. */
    jump(ms: number) {
      now += ms;
    },
    show(on: boolean) {
      visible = on;
      if (on) for (const run of [...watchers]) run();
    },
  };
}

const NOON = Date.parse("2026-11-02T09:00:00Z");
const MIDNIGHT = "2026-11-02T21:00:00Z"; // 3 November, 00:00 in Turkey

describe("season looks", () => {
  it("dress Home by season and leave the rest to Studio", () => {
    expect(seasonLook("winter")).toEqual({ particles: "snow", palette: "ember", scrim: "6 13 28" });
    expect(seasonLook("halloween")).toMatchObject({ palette: "halloween", grade: NIGHT_GRADE });
    expect(seasonLook("holiday")).toMatchObject({
      particles: "lights",
      palette: "gold",
      grade: EVENING_GRADE,
    });
    expect(seasonLook("event").grade).toBeUndefined();
    expect(seasonLook("none")).toEqual({ palette: "ember", scrim: DEFAULT_SCRIM });
    expect(seasonLook(undefined)).toEqual(seasonLook("none"));
  });

  it("grade the art only by multiplying it, darkest at the top", () => {
    // A multiply grade never lightens: every stop is a colour, the last one of the evening white.
    for (const grade of [NIGHT_GRADE, EVENING_GRADE]) {
      const stops = [...grade.matchAll(/rgb\((\d+) (\d+) (\d+)\)/g)].map((m) =>
        [m[1], m[2], m[3]].map(Number).reduce((sum, v) => sum + v, 0),
      );
      expect(stops.length).toBeGreaterThanOrEqual(3);
      expect([...stops].sort((a, b) => a - b)).toEqual(stops);
    }
    expect(EVENING_GRADE).toMatch(/rgb\(255 255 255\) \d+%\)$/);
  });

  it("let lights hold their strength and snow come in under the title bar", () => {
    expect(particleFade("lights", 0.5)).toBe(1);
    expect(particleFade("lights", 0)).toBe(0);
    expect(particleFade("lights", 1)).toBe(0);
    // Embers flare up and die out: never at full strength for long.
    expect(particleFade("embers", 0.5)).toBeLessThan(0.4);
    expect(snowTopFade(10, 34)).toBe(0);
    expect(snowTopFade(34 + 28, 34)).toBe(0.5);
    expect(snowTopFade(200, 34)).toBe(1);
    expect(snowTopFade(5, 0)).toBe(1);
  });

  it("never bring particles back that the player or Studio turned off", () => {
    const winter = seasonLook("winter");
    expect(homeParticles(true, "embers", winter)).toBe("snow");
    expect(homeParticles(false, "embers", winter)).toBe("none");
    expect(homeParticles(true, "none", winter)).toBe("none");
    expect(homeParticles(true, "sparks", seasonLook("none"))).toBe("sparks");
  });

  it("pick their ember colours from the palette's bands", () => {
    expect(emberHue("ember", 0.5, 0)).toBe(28);
    expect(emberHue("ember", 0.5, 1)).toBe(46);
    // Halloween: most pumpkin orange, some witch violet.
    expect(emberHue("halloween", 0.1, 0.5)).toBeLessThan(40);
    expect(emberHue("halloween", 0.9, 0.5)).toBeGreaterThan(260);
    expect(emberHue("gold", 0.3, 0.5)).toBe(46);
  });
});

describe("season clock", () => {
  it("waits until the moment, capped below setTimeout's limit", () => {
    expect(seasonWait(undefined, NOON)).toBeUndefined();
    expect(seasonWait("not a date", NOON)).toBeUndefined();
    expect(seasonWait(MIDNIGHT, NOON)).toBe(12 * 3_600_000 + 250);
    expect(seasonWait(MIDNIGHT, Date.parse(MIDNIGHT) + 5000)).toBe(250);
    expect(seasonWait("2027-06-01T00:00:00Z", NOON)).toBe(MAX_TIMER_MS);
    expect(seasonDue(MIDNIGHT, Date.parse(MIDNIGHT) - 1)).toBe(false);
    expect(seasonDue(MIDNIGHT, Date.parse(MIDNIGHT))).toBe(true);
    expect(seasonDue(undefined, NOON)).toBe(false);
  });

  it("refreshes once at the moment and not before", () => {
    const clock = fakeClock(NOON);
    let refreshes = 0;
    startSeasonClock(MIDNIGHT, () => refreshes++, clock.env);
    clock.advance(12 * 3_600_000);
    expect(refreshes).toBe(0);
    clock.advance(250);
    expect(refreshes).toBe(1);
    clock.show(false);
    clock.show(true);
    clock.advance(86_400_000);
    expect(refreshes).toBe(1);
    expect(clock.timers.size).toBe(0);
    expect(clock.watchers.size).toBe(0);
  });

  it("does nothing while nothing changes ahead", () => {
    const clock = fakeClock(NOON);
    let refreshes = 0;
    const stop = startSeasonClock(undefined, () => refreshes++, clock.env);
    expect(clock.timers.size + clock.watchers.size).toBe(0);
    stop();
    expect(refreshes).toBe(0);
  });

  it("sleeps in long stretches and wakes only a few times for a far change", () => {
    const clock = fakeClock(NOON);
    let refreshes = 0;
    let wakes = 0;
    const until = new Date(NOON + 60 * 86_400_000).toISOString();
    const counting: ClockEnv = {
      ...clock.env,
      setTimeout: (run, ms) => {
        wakes++;
        return clock.env.setTimeout(run, ms);
      },
    };
    startSeasonClock(until, () => refreshes++, counting);
    clock.advance(59 * 86_400_000);
    expect(refreshes).toBe(0);
    clock.advance(86_400_000 + 250);
    expect(refreshes).toBe(1);
    expect(wakes).toBe(3);
  });

  it("waits for the window when the moment comes in the tray", () => {
    const clock = fakeClock(NOON);
    let refreshes = 0;
    startSeasonClock(MIDNIGHT, () => refreshes++, clock.env);
    clock.show(false);
    clock.advance(13 * 3_600_000);
    expect(refreshes).toBe(0);
    expect(clock.timers.size).toBe(0);
    clock.show(true);
    expect(refreshes).toBe(1);
  });

  it("catches up when the window shows after a sleep past the moment", () => {
    const clock = fakeClock(NOON);
    let refreshes = 0;
    startSeasonClock(MIDNIGHT, () => refreshes++, clock.env);
    // The computer slept through the night: the timer has not fired yet.
    clock.jump(20 * 3_600_000);
    clock.show(false);
    clock.show(true);
    expect(refreshes).toBe(1);
    // A window shown before the moment changes nothing.
    const early = fakeClock(NOON);
    let none = 0;
    startSeasonClock(MIDNIGHT, () => none++, early.env);
    early.show(true);
    expect(none).toBe(0);
  });

  it("stops cleanly", () => {
    const clock = fakeClock(NOON);
    let refreshes = 0;
    const stop = startSeasonClock(MIDNIGHT, () => refreshes++, clock.env);
    stop();
    clock.advance(86_400_000);
    clock.show(true);
    expect(refreshes).toBe(0);
    expect(clock.timers.size + clock.watchers.size).toBe(0);
  });
});
