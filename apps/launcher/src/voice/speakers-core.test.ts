import { describe, expect, it } from "vitest";
import { levelOf } from "./dsp";
import { LEVEL_QUANTA, SpeakersCore } from "./speakers-core";

const RATE = 48000;

function tone(db: number): Float32Array {
  const amplitude = 10 ** (db / 20) * Math.SQRT2;
  return Float32Array.from({ length: 128 }, (_, i) => amplitude * Math.sin(i / 5));
}

describe("SpeakersCore", () => {
  it("posts only when someone lights up or goes out, 300 ms after their last sound", () => {
    const posts: [number, boolean][] = [];
    const core = new SpeakersCore(RATE, 3, (slot, on) => posts.push([slot, on]));
    const silence: Float32Array[][] = [[new Float32Array(128)], [], [new Float32Array(128)]];
    for (let q = 0; q < 375; q++) core.process(silence);
    expect(posts).toEqual([]);
    // Slot 2 speaks for 100 ms; slot 1 is not connected.
    for (let q = 0; q < 38; q++) core.process([[new Float32Array(128)], [], [tone(-30)]]);
    expect(posts).toEqual([[2, true]]);
    let quanta = 0;
    while (posts.length < 2 && quanta < 1000) {
      core.process(silence);
      quanta += 1;
    }
    expect(posts).toEqual([
      [2, true],
      [2, false],
    ]);
    expect((quanta * 128) / RATE).toBeCloseTo(0.3, 1);
  });

  it("does not light up for the faint noise between words", () => {
    const posts: [number, boolean][] = [];
    const core = new SpeakersCore(RATE, 1, (slot, on) => posts.push([slot, on]));
    for (let q = 0; q < 375; q++) core.process([[tone(-60)]]);
    expect(posts).toEqual([]);
  });

  it("starts a slot over when it changes hands, under the page's new generation", () => {
    const posts: [number, boolean, number][] = [];
    const core = new SpeakersCore(RATE, 1, (slot, on, generation) =>
      posts.push([slot, on, generation]),
    );
    core.reset(0, 1);
    core.process([[tone(-20)]]);
    core.reset(0, 2);
    core.reset(5, 9);
    core.process([[new Float32Array(128)]]);
    // Reset unlit it without a post (the page dropped the old person's ring itself).
    expect(posts).toEqual([[0, true, 1]]);
    core.process([[tone(-20)]]);
    expect(posts).toEqual([
      [0, true, 1],
      [0, true, 2],
    ]);
  });

  describe("levels for the live meters (VOICE-CALL)", () => {
    function rig(slots = 2) {
      const levels: { levels: number[]; generations: number[] }[] = [];
      const core = new SpeakersCore(
        RATE,
        slots,
        () => {},
        (values, generations) => levels.push({ levels: values, generations }),
      );
      return { core, levels };
    }

    it("posts nothing while off, and nothing in a quiet room while on", () => {
      const { core, levels } = rig();
      for (let q = 0; q < 200; q++) core.process([[tone(-20)], []]);
      expect(levels).toEqual([]);
      core.setLevels(true);
      for (let q = 0; q < 400; q++) core.process([[tone(-60)], [new Float32Array(128)]]);
      expect(levels).toEqual([]);
    });

    it("says each slot's loudest about sixteen times a second, only when it moved", () => {
      const { core, levels } = rig();
      core.reset(0, 4);
      core.setLevels(true);
      for (let q = 0; q < LEVEL_QUANTA; q++) core.process([[tone(-20)], []]);
      expect(levels).toHaveLength(1);
      expect(levels[0]?.levels[0]).toBeCloseTo(levelOf(-20), 1);
      expect(levels[0]?.levels[1]).toBe(0);
      expect(levels[0]?.generations[0]).toBe(4);
      // The same voice: nothing new.
      for (let q = 0; q < 10 * LEVEL_QUANTA; q++) core.process([[tone(-20)], []]);
      expect(levels).toHaveLength(1);
      // Louder, then quiet: each said once.
      for (let q = 0; q < LEVEL_QUANTA; q++) core.process([[tone(-8)], []]);
      for (let q = 0; q < LEVEL_QUANTA; q++) core.process([[new Float32Array(128)], []]);
      expect(levels).toHaveLength(3);
      expect(levels[2]?.levels[0]).toBe(0);
      for (let q = 0; q < 10 * LEVEL_QUANTA; q++) core.process([[new Float32Array(128)], []]);
      expect(levels).toHaveLength(3);
    });
  });
});
