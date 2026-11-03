import { describe, expect, it } from "vitest";
import { BLOCK, type CaptureConfig, CaptureCore } from "./capture-core";
import { levelOf } from "./dsp";
import type { Denoiser } from "./rnnoise";

const RATE = 48000;
/** Quanta per second. */
const PER_S = RATE / BLOCK;

/** A test microphone and what came out. */
class Rig {
  readonly me: boolean[] = [];
  readonly levels: { level: number; open: boolean }[] = [];
  readonly meters: { level: number; open: boolean }[] = [];
  slow = 0;
  now = 0;
  readonly core: CaptureCore;
  private phase = 0;

  constructor(config: Partial<CaptureConfig> = {}, denoiser: Denoiser | null = null) {
    this.core = new CaptureCore(
      RATE,
      {
        me: (on) => this.me.push(on),
        level: (level, open) => this.levels.push({ level, open }),
        meter: (level, open) => this.meters.push({ level, open }),
        slow: () => {
          this.slow += 1;
        },
      },
      () => this.now,
    );
    this.core.configure({ gain: 1, ns: true, threshold: null, ptt: false, ...config });
    this.core.setDenoiser(denoiser);
  }

  /** `seconds` of a 300 Hz tone whose RMS is `db` dBFS; returns the RMS of what was sent. */
  feed(db: number, seconds: number): { sent: number[]; monitored: number[]; samples: number[] } {
    const amplitude = 10 ** (db / 20) * Math.SQRT2;
    const sent: number[] = [];
    const monitored: number[] = [];
    const samples: number[] = [];
    for (let q = 0; q < Math.round(seconds * PER_S); q++) {
      const x = new Float32Array(BLOCK);
      for (let i = 0; i < BLOCK; i++) {
        x[i] = amplitude * Math.sin(this.phase);
        this.phase += (2 * Math.PI * 300) / RATE;
      }
      const send = new Float32Array(BLOCK).fill(9);
      const monitor = new Float32Array(BLOCK).fill(9);
      this.core.process(x, send, monitor);
      sent.push(rms(send));
      monitored.push(rms(monitor));
      samples.push(...send);
    }
    return { sent, monitored, samples };
  }
}

/** The largest step between neighbouring samples (a cut is a step; a fade is not). */
function largestStep(samples: number[]): number {
  let largest = 0;
  for (let i = 1; i < samples.length; i++) {
    largest = Math.max(largest, Math.abs((samples[i] ?? 0) - (samples[i - 1] ?? 0)));
  }
  return largest;
}

function rms(x: Float32Array): number {
  let energy = 0;
  for (const v of x) energy += v * v;
  return Math.sqrt(energy / x.length);
}

/** A suppressor that only counts and answers a fixed speech probability. */
function fakeDenoiser(vad: number): Denoiser & { calls: number } {
  const fake = {
    calls: 0,
    process(frame: Float32Array): number {
      fake.calls += 1;
      expect(frame).toHaveLength(480);
      return vad;
    },
  };
  return fake;
}

const QUIET = -65;
const SPEECH = -25;

describe("CaptureCore: the live meter (VOICE-CALL)", () => {
  it("listens without sending, and the noise suppressor sleeps meanwhile", () => {
    const denoiser = fakeDenoiser(0.9);
    const rig = new Rig({}, denoiser);
    rig.core.setMeter(true);
    const { sent } = rig.feed(SPEECH, 0.5);
    expect(sent.every((v) => v === 0)).toBe(true);
    expect(denoiser.calls).toBe(0);
    expect(rig.meters.length).toBeGreaterThan(0);
    // Heard, not sent: never "open".
    expect(rig.meters.every((m) => !m.open)).toBe(true);
    expect(Math.max(...rig.meters.map((m) => m.level))).toBeCloseTo(levelOf(SPEECH), 1);
    expect(rig.me).toEqual([]);
  });

  it("says what goes out, about sixteen times a second at most, and nothing in the quiet", () => {
    const rig = new Rig({ ns: false });
    rig.core.talk(true);
    rig.core.setMeter(true);
    rig.feed(QUIET, 1);
    expect(rig.meters).toEqual([]);
    rig.feed(SPEECH, 1);
    expect(rig.meters.length).toBeGreaterThan(0);
    expect(rig.meters.length).toBeLessThanOrEqual(17);
    expect(rig.meters.some((m) => m.open)).toBe(true);
    // A steady voice changes nothing more after it settles.
    const settled = rig.meters.length;
    rig.feed(SPEECH, 1);
    expect(rig.meters.length - settled).toBeLessThanOrEqual(1);
    // Quiet again: one post that it rests, then none.
    rig.feed(QUIET, 1);
    expect(rig.meters.at(-1)).toEqual({ level: 0, open: false });
    const rested = rig.meters.length;
    rig.feed(QUIET, 1);
    expect(rig.meters).toHaveLength(rested);
  });

  it("stops listening when the meter goes off", () => {
    const rig = new Rig({ ns: false });
    rig.core.setMeter(true);
    rig.feed(SPEECH, 0.3);
    rig.core.setMeter(false);
    const before = rig.meters.length;
    rig.feed(SPEECH, 0.5);
    expect(rig.meters).toHaveLength(before);
  });
});

describe("CaptureCore", () => {
  it("does nothing while nothing needs the microphone (muted, no bar, no test)", () => {
    const denoiser = fakeDenoiser(0.9);
    const rig = new Rig({}, denoiser);
    const { sent, monitored } = rig.feed(SPEECH, 1);
    expect(sent.every((v) => v === 0)).toBe(true);
    expect(monitored.every((v) => v === 0)).toBe(true);
    expect(denoiser.calls).toBe(0);
    expect(rig.me).toEqual([]);
  });

  it("sends speech above the room and keeps the end of words, nothing in the quiet", () => {
    const rig = new Rig({ ns: false });
    rig.core.talk(true);
    const quiet = rig.feed(QUIET, 1);
    expect(quiet.sent.every((v) => v === 0)).toBe(true);
    const speech = rig.feed(SPEECH, 0.5);
    // After the 13 ms delay and a 5 ms fade it goes as it came.
    const steady = speech.sent.slice(20);
    expect(Math.min(...steady)).toBeGreaterThan(10 ** (SPEECH / 20) * 0.9);
    expect(rig.me).toEqual([true]);
    // Back to quiet: open 400 ms more (the hold), then shut.
    const after = rig.feed(QUIET, 1);
    const open = after.sent.filter((v) => v > 0).length / PER_S;
    expect(open).toBeGreaterThan(0.38);
    expect(open).toBeLessThan(0.46);
    expect(after.sent.slice(-20).every((v) => v === 0)).toBe(true);
    expect(rig.me).toEqual([true, false]);
  });

  it("wakes the suppressor only for frames that may be speech", () => {
    const denoiser = fakeDenoiser(0.95);
    const rig = new Rig({}, denoiser);
    rig.core.talk(true);
    rig.feed(QUIET, 2);
    expect(denoiser.calls).toBe(0);
    rig.feed(SPEECH, 1);
    // Every frame of the speech (100 a second), give or take the edges.
    expect(denoiser.calls).toBeGreaterThan(95);
    const during = denoiser.calls;
    rig.feed(QUIET, 2);
    // The hold (400 ms) and one frame more, then nothing.
    expect(denoiser.calls - during).toBeLessThan(45);
    const settled = denoiser.calls;
    rig.feed(QUIET, 1);
    expect(denoiser.calls).toBe(settled);
  });

  it("keeps out loud sound the suppressor says is not speech (keyboards, clicks)", () => {
    const rig = new Rig({}, fakeDenoiser(0.05));
    rig.core.talk(true);
    rig.feed(QUIET, 1);
    const { sent } = rig.feed(SPEECH, 1);
    expect(sent.every((v) => v === 0)).toBe(true);
    expect(rig.me).toEqual([]);
  });

  it("push-to-talk: the key alone opens, and the word's end goes 200 ms after it", () => {
    const rig = new Rig({ ns: false, ptt: true });
    rig.feed(QUIET, 1);
    rig.core.talk(true);
    const held = rig.feed(QUIET, 0.5);
    // Quiet as it is, it goes while the key is down.
    expect(held.sent.slice(10).every((v) => v > 0)).toBe(true);
    rig.core.talk(false);
    const released = rig.feed(QUIET, 0.5);
    const tail = released.sent.filter((v) => v > 0).length / PER_S;
    // 200 ms, what was on its way (13 ms) and a 5 ms fade.
    expect(tail).toBeGreaterThan(0.2);
    expect(tail).toBeLessThan(0.25);
    // Speech without the key goes nowhere.
    expect(rig.feed(SPEECH, 0.5).sent.every((v) => v === 0)).toBe(true);
  });

  it("mutes at once in voice activity", () => {
    const rig = new Rig({ ns: false });
    rig.core.talk(true);
    rig.feed(QUIET, 1);
    rig.feed(SPEECH, 0.5);
    rig.core.talk(false);
    const muted = rig.feed(SPEECH, 0.2);
    // The 13 ms already on its way, a 5 ms fade, then silence.
    expect(muted.sent.slice(8).every((v) => v === 0)).toBe(true);
    expect(rig.me.at(-1)).toBe(false);
  });

  it("self-test plays the gated sound back and sends nothing", () => {
    const rig = new Rig({ ns: false });
    rig.core.talk(true);
    rig.core.setTest(true);
    rig.feed(QUIET, 1);
    const { sent, monitored } = rig.feed(SPEECH, 0.5);
    expect(sent.every((v) => v === 0)).toBe(true);
    expect(Math.min(...monitored.slice(20))).toBeGreaterThan(0.04);
    expect(rig.me).toEqual([]);
  });

  it("feeds the settings page's bar ten times a second with the loudest moment", () => {
    const rig = new Rig({ ns: false });
    rig.core.setLevels(true);
    rig.feed(QUIET, 1);
    expect(rig.levels.length).toBeGreaterThanOrEqual(9);
    expect(rig.levels.length).toBeLessThanOrEqual(10);
    expect(rig.levels.every((l) => !l.open)).toBe(true);
    rig.feed(SPEECH, 1);
    const last = rig.levels.at(-1);
    expect(last?.open).toBe(true);
    expect(last?.level).toBeCloseTo(levelOf(SPEECH), 1);
    rig.core.setLevels(false);
    const count = rig.levels.length;
    rig.feed(SPEECH, 1);
    expect(rig.levels).toHaveLength(count);
  });

  it("opens at the manual threshold exactly where the bar says", () => {
    // 50 on the bar's scale is -30 dBFS.
    const rig = new Rig({ ns: false, threshold: 50 });
    rig.core.talk(true);
    rig.core.setLevels(true);
    expect(rig.feed(-33, 0.5).sent.every((v) => v === 0)).toBe(true);
    expect(rig.levels.at(-1)?.open).toBe(false);
    expect(
      rig
        .feed(-27, 0.5)
        .sent.slice(20)
        .every((v) => v > 0),
    ).toBe(true);
    expect(rig.levels.at(-1)?.open).toBe(true);
  });

  it("lets everything through at threshold 0", () => {
    const rig = new Rig({ ns: false, threshold: 0 });
    rig.core.talk(true);
    expect(
      rig
        .feed(-80, 0.5)
        .sent.slice(10)
        .every((v) => v > 0),
    ).toBe(true);
  });

  it("says once that the suppressor is too slow (over half of real time, twice running)", () => {
    const rig = new Rig({});
    rig.core.setDenoiser({
      process: () => {
        rig.now += 6; // 6 ms for a 10 ms frame
        return 0.9;
      },
    });
    rig.core.talk(true);
    rig.feed(QUIET, 0.5);
    rig.feed(SPEECH, 1.5);
    expect(rig.slow).toBe(0);
    rig.feed(SPEECH, 1);
    expect(rig.slow).toBe(1);
    rig.feed(SPEECH, 3);
    expect(rig.slow).toBe(1);
  });

  it("holds a raised gain to full scale", () => {
    const rig = new Rig({ ns: false, gain: 2 });
    rig.core.talk(true);
    rig.feed(QUIET, 0.5);
    const { sent } = rig.feed(-3, 0.5);
    expect(Math.max(...sent.slice(20))).toBeLessThanOrEqual(1);
    expect(Math.max(...sent.slice(20))).toBeGreaterThan(0.6);
  });

  it("starts shut after a mute, still knowing the room", () => {
    const rig = new Rig({ ns: false });
    rig.core.talk(true);
    rig.feed(-50, 2);
    rig.core.talk(false);
    rig.feed(-50, 1);
    rig.core.talk(true);
    // The fan the gate learned before the mute does not open it.
    expect(rig.feed(-50, 1).sent.every((v) => v === 0)).toBe(true);
  });

  it("fades a mute out in 5 ms instead of cutting it (no click)", () => {
    const rig = new Rig({ ns: false });
    rig.core.talk(true);
    rig.feed(QUIET, 1);
    const speaking = rig.feed(SPEECH, 0.5);
    rig.core.talk(false);
    const muted = rig.feed(SPEECH, 0.1);
    // Not cut: the first quantum still carries the fading sound...
    expect(muted.sent[0] ?? 0).toBeGreaterThan(0);
    // ...gone within two quanta. A cut would step by the tone's amplitude (25 times its own
    // largest step here); the fade adds a tenth at most.
    expect(muted.sent.slice(2).every((v) => v === 0)).toBe(true);
    const tone = largestStep(speaking.samples.slice(-BLOCK * 10));
    expect(largestStep([...speaking.samples.slice(-BLOCK), ...muted.samples])).toBeLessThanOrEqual(
      tone * 1.5,
    );
  });

  it("closes push-to-talk at once when the close is a mute", () => {
    const rig = new Rig({ ns: false, ptt: true });
    rig.core.talk(true);
    rig.feed(QUIET, 0.5);
    rig.core.talk(false, false);
    const muted = rig.feed(QUIET, 0.5);
    expect(muted.sent.slice(2).every((v) => v === 0)).toBe(true);
  });

  it("cleans every frame push-to-talk sends, the quiet ones too", () => {
    const denoiser = fakeDenoiser(0.1);
    const rig = new Rig({ ptt: true }, denoiser);
    rig.feed(QUIET, 0.5);
    expect(denoiser.calls).toBe(0);
    rig.core.talk(true);
    rig.feed(QUIET, 1);
    // A frame each 10 ms, and one more to start the run.
    expect(denoiser.calls).toBeGreaterThanOrEqual(99);
    expect(denoiser.calls).toBeLessThanOrEqual(102);
  });

  it("starts each run of the suppressor from the frame that came just before", () => {
    const inputs: number[] = [];
    const denoiser: Denoiser = {
      process(frame) {
        inputs.push(rms(frame));
        return 0.9;
      },
    };
    const rig = new Rig({}, denoiser);
    rig.core.talk(true);
    rig.feed(QUIET, 1);
    expect(inputs).toEqual([]);
    rig.feed(SPEECH, 0.02);
    // The previous frame first (quiet, its output dropped), then the speech itself.
    expect(inputs.length).toBeGreaterThanOrEqual(2);
    const scale = 32768;
    expect((inputs[0] ?? 0) / scale).toBeLessThan(10 ** (-60 / 20));
    expect((inputs[1] ?? 0) / scale).toBeGreaterThan(10 ** (-40 / 20));
  });
});
