import { describe, expect, it } from "vitest";
import { PILOT_HZ, PILOT_LEVEL, PilotMeter, SpeechGate, toneAmplitude } from "./dsp";

const RATE = 48000;

function sine(amplitude: number, freq: number, start: number, frames = 128): Float32Array {
  const x = new Float32Array(frames);
  for (let i = 0; i < frames; i++)
    x[i] = amplitude * Math.sin((2 * Math.PI * freq * (start + i)) / RATE);
  return x;
}

describe("toneAmplitude", () => {
  it("reads the pilot within a few percent in every quantum", () => {
    for (let block = 0; block < 40; block++) {
      const amplitude = toneAmplitude(sine(PILOT_LEVEL, PILOT_HZ, block * 128), PILOT_HZ, RATE);
      expect(amplitude / PILOT_LEVEL).toBeGreaterThan(0.9);
      expect(amplitude / PILOT_LEVEL).toBeLessThan(1.1);
    }
  });

  it("barely sees other frequencies and silence", () => {
    expect(toneAmplitude(sine(0.5, 3000, 0), PILOT_HZ, RATE)).toBeLessThan(0.05);
    expect(toneAmplitude(new Float32Array(128), PILOT_HZ, RATE)).toBe(0);
    expect(toneAmplitude(new Float32Array(0), PILOT_HZ, RATE)).toBe(0);
  });
});

describe("PilotMeter", () => {
  it("counts breaks after settling, and the longest per stretch", () => {
    const meter = new PilotMeter(0.01, 2);
    // Settling: low readings do not count.
    meter.push(0);
    meter.push(0);
    for (const amplitude of [0.03, 0, 0, 0.03, 0, 0.03]) meter.push(amplitude);
    expect(meter.take()).toEqual({ blocks: 8, gaps: 2, gapBlocks: 3, longestGapBlocks: 2 });
    // A break still going on carries into the next stretch.
    meter.push(0);
    meter.push(0);
    expect(meter.take()).toMatchObject({ gaps: 3, gapBlocks: 5, longestGapBlocks: 2 });
    meter.push(0);
    expect(meter.take()).toMatchObject({ gaps: 3, gapBlocks: 6, longestGapBlocks: 3 });
    meter.push(0.02);
    expect(meter.take()).toMatchObject({ longestGapBlocks: 3 });
    expect(meter.take()).toMatchObject({ longestGapBlocks: 0 });
  });
});

describe("SpeechGate", () => {
  const quantum = (db: number) => 10 ** (db / 20);

  it("opens on speech over the room's noise and holds after it", () => {
    const gate = new SpeechGate(RATE);
    // A second of a fan at -55 dBFS: the floor settles, the gate stays shut.
    for (let i = 0; i < 375; i++) expect(gate.push(quantum(-55), 128)).toBe(false);
    expect(gate.push(quantum(-30), 128)).toBe(true);
    // Back to the fan: open for the 400 ms hold (150 quanta), then shut.
    let open = 0;
    for (let i = 0; i < 300; i++) if (gate.push(quantum(-55), 128)) open++;
    expect(open).toBeGreaterThanOrEqual(149);
    expect(open).toBeLessThanOrEqual(151);
    expect(gate.open).toBe(false);
  });

  it("never opens for very quiet sound", () => {
    const gate = new SpeechGate(RATE);
    for (let i = 0; i < 375; i++) gate.push(quantum(-90), 128);
    expect(gate.push(quantum(-60), 128)).toBe(false);
  });
});
