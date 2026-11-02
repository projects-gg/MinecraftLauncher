import { describe, expect, it } from "vitest";
import { loops, renderSound, type SoundName } from "./sounds";

const RATE = 48000;

function peakOf(samples: Float32Array): number {
  let peak = 0;
  for (const v of samples) peak = Math.max(peak, Math.abs(v));
  return peak;
}

describe("renderSound", () => {
  it("makes short, quiet sounds that start and end at rest", () => {
    const lengths: Record<SoundName, number> = { join: 0.3, leave: 0.3, tick: 0.05, waiting: 3 };
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

  it("loops only the caller's waiting tone, which is mostly quiet", () => {
    expect(loops("waiting")).toBe(true);
    expect(loops("join")).toBe(false);
    const waiting = renderSound("waiting", RATE);
    let loud = 0;
    for (const v of waiting) if (Math.abs(v) > 0.001) loud += 1;
    expect(loud / waiting.length).toBeLessThan(0.3);
  });
});
