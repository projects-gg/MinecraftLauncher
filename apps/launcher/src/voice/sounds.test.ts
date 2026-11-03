import { describe, expect, it } from "vitest";
import { loops, playsWhileDeaf, renderSound, SOUND_NAMES, type SoundName } from "./sounds";

const RATE = 48000;

function peakOf(samples: Float32Array): number {
  let peak = 0;
  for (const v of samples) peak = Math.max(peak, Math.abs(v));
  return peak;
}

/** Where a sound is loudest, in seconds (which way its notes go, roughly). */
function centreOf(samples: Float32Array, from: number, to: number): number {
  let weight = 0;
  let sum = 0;
  for (let i = Math.round(from * RATE); i < Math.round(to * RATE); i++) {
    const v = Math.abs(samples[i] ?? 0);
    weight += v;
    sum += v * i;
  }
  return weight ? sum / weight / RATE : 0;
}

/** Zero crossings a second over a stretch: the pitch, near enough. */
function pitchOf(samples: Float32Array, from: number, to: number): number {
  let crossings = 0;
  const a = Math.round(from * RATE);
  const b = Math.round(to * RATE);
  for (let i = a + 1; i < b; i++) {
    if ((samples[i - 1] ?? 0) <= 0 && (samples[i] ?? 0) > 0) crossings += 1;
  }
  return crossings / (to - from);
}

describe("renderSound", () => {
  it("makes short, quiet sounds that start and end at rest", () => {
    const lengths: Record<SoundName, number> = {
      join: 0.3,
      leave: 0.3,
      micOff: 0.13,
      micOn: 0.13,
      deafOn: 0.3,
      deafOff: 0.3,
      connected: 0.42,
      ended: 0.5,
      lost: 0.48,
      waiting: 3,
    };
    expect([...SOUND_NAMES].sort()).toEqual(Object.keys(lengths).sort());
    for (const [name, seconds] of Object.entries(lengths) as [SoundName, number][]) {
      const samples = renderSound(name, RATE);
      expect(samples.length).toBe(Math.round(seconds * RATE));
      // Quiet: under -12 dBFS, and heard at all.
      expect(peakOf(samples)).toBeLessThan(0.26);
      expect(peakOf(samples)).toBeGreaterThan(0.05);
      expect(Math.abs(samples[0] ?? 1)).toBeLessThan(0.01);
      expect(Math.abs(samples.at(-1) ?? 1)).toBeLessThan(0.01);
    }
  });

  it("rises for on and in, falls for off and out", () => {
    const rises = (name: SoundName, split: number, end: number) => {
      const samples = renderSound(name, RATE);
      return pitchOf(samples, split, end) > pitchOf(samples, 0.005, split);
    };
    expect(rises("join", 0.085, 0.2)).toBe(true);
    expect(rises("leave", 0.085, 0.2)).toBe(false);
    expect(rises("micOn", 0.048, 0.1)).toBe(true);
    expect(rises("micOff", 0.048, 0.1)).toBe(false);
    expect(rises("deafOff", 0.07, 0.2)).toBe(true);
    expect(rises("deafOn", 0.07, 0.2)).toBe(false);
    expect(rises("connected", 0.14, 0.3)).toBe(true);
    expect(rises("ended", 0.17, 0.35)).toBe(false);
    // The microphone's pair is the shortest: a toggle, not news.
    const mic = renderSound("micOff", RATE);
    expect(centreOf(mic, 0, 0.13)).toBeLessThan(0.08);
  });

  it("loops only the caller's waiting tone, which is mostly quiet", () => {
    expect(loops("waiting")).toBe(true);
    expect(loops("join")).toBe(false);
    const waiting = renderSound("waiting", RATE);
    let loud = 0;
    for (const v of waiting) if (Math.abs(v) > 0.001) loud += 1;
    expect(loud / waiting.length).toBeLessThan(0.3);
  });

  it("keeps the player's own sounds while deafened", () => {
    for (const own of [
      "micOff",
      "micOn",
      "deafOn",
      "deafOff",
      "connected",
      "ended",
      "lost",
      "stop",
    ]) {
      expect(playsWhileDeaf(own as SoundName | "stop")).toBe(true);
    }
    for (const others of ["join", "leave", "waiting"] as const) {
      expect(playsWhileDeaf(others)).toBe(false);
    }
  });
});
