import { describe, expect, it } from "vitest";
import {
  dbOf,
  GATE,
  LEVEL_FLOOR_DB,
  levelOf,
  PILOT_HZ,
  PILOT_LEVEL,
  PilotMeter,
  SpeechGate,
  thresholdDb,
  toneAmplitude,
  VoiceGate,
} from "./dsp";

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

describe("the level bar and the threshold", () => {
  it("maps -60 to 0 dBFS onto the bar", () => {
    expect(levelOf(LEVEL_FLOOR_DB)).toBe(0);
    expect(levelOf(-80)).toBe(0);
    expect(levelOf(-30)).toBeCloseTo(0.5);
    expect(levelOf(0)).toBe(1);
    expect(levelOf(6)).toBe(1);
    expect(dbOf(0)).toBe(-180);
    expect(dbOf(1)).toBe(0);
  });

  it("puts the threshold where the bar is (0 lets everything through)", () => {
    expect(thresholdDb(0)).toBe(Number.NEGATIVE_INFINITY);
    expect(thresholdDb(30)).toBeCloseTo(-42);
    expect(thresholdDb(100)).toBeCloseTo(0);
    expect(thresholdDb(150)).toBeCloseTo(0);
    for (const t of [10, 30, 55, 90]) expect(levelOf(thresholdDb(t))).toBeCloseTo(t / 100);
  });
});

describe("VoiceGate", () => {
  const FRAME = 0.01;
  const run = (gate: VoiceGate, db: number, frames: number, vad: number | null = null) => {
    let open = 0;
    for (let i = 0; i < frames; i++) if (gate.push(db, vad)) open++;
    return open;
  };

  it("learns the room from its first frames and opens on speech above it", () => {
    const gate = new VoiceGate(FRAME);
    // A fan at -45 dBFS from the first frame: never speech.
    expect(run(gate, -45, 200)).toBe(0);
    expect(gate.push(-25, null)).toBe(true);
    // Held 400 ms after.
    expect(run(gate, -45, 100)).toBe(40);
  });

  it("does not take a device's first digital silence for the room", () => {
    const gate = new VoiceGate(FRAME);
    run(gate, -180, 20);
    expect(run(gate, -48, 100)).toBe(0);
  });

  it("asks the suppressor's opinion: loud but not speech stays out", () => {
    const gate = new VoiceGate(FRAME);
    run(gate, -60, 100);
    expect(gate.push(-25, 0.2)).toBe(false);
    expect(gate.push(-25, GATE.vadOpen)).toBe(true);
    // Open: a weaker probability keeps it (unvoiced sounds inside words).
    expect(gate.push(-25, GATE.vadStay)).toBe(true);
  });

  it("wakes the suppressor only near speech, and while open", () => {
    const gate = new VoiceGate(FRAME);
    run(gate, -60, 100);
    expect(gate.wantsDenoise(-60)).toBe(false);
    expect(gate.wantsDenoise(-58)).toBe(false);
    expect(gate.wantsDenoise(-50)).toBe(true);
    gate.push(-20, 0.9);
    expect(gate.wantsDenoise(-80)).toBe(true);
  });

  it("follows a manual threshold alone", () => {
    const gate = new VoiceGate(FRAME);
    gate.threshold = 50;
    expect(gate.push(-31, 0)).toBe(false);
    expect(gate.push(-29, 0)).toBe(true);
    expect(gate.wantsDenoise(-35)).toBe(true);
    gate.close();
    expect(gate.open).toBe(false);
    expect(gate.wantsDenoise(-37)).toBe(false);
  });

  it("keeps the room across a close and forgets it on reset", () => {
    const gate = new VoiceGate(FRAME);
    run(gate, -45, 100);
    gate.close();
    expect(gate.push(-44, null)).toBe(false);
    gate.reset();
    // A new microphone: its first frame is its room.
    expect(gate.push(-30, null)).toBe(false);
  });
});
