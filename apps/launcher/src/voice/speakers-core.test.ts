import { describe, expect, it } from "vitest";
import { SpeakersCore } from "./speakers-core";

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

  it("starts a slot over when it changes hands", () => {
    const posts: [number, boolean][] = [];
    const core = new SpeakersCore(RATE, 1, (slot, on) => posts.push([slot, on]));
    core.process([[tone(-20)]]);
    core.reset(0);
    core.reset(5);
    core.process([[new Float32Array(128)]]);
    // Reset unlit it without a post (the page dropped the old person's ring itself).
    expect(posts).toEqual([[0, true]]);
    core.process([[tone(-20)]]);
    expect(posts).toEqual([
      [0, true],
      [0, true],
    ]);
  });
});
