// Small signal tools of the voice window, shared by its AudioWorklets and tested here.

/** The pilot tone VOICE-0 adds after processing: its breaks on arrival show breaks on the way. */
export const PILOT_HZ = 1000;
/** Its amplitude: a sine at -30 dBFS. */
export const PILOT_LEVEL = 10 ** (-30 / 20);

/**
 * The amplitude of the `freq` sine in `x` (Goertzel, one bin). For a 128-frame quantum at 48 kHz
 * the bin is wide (375 Hz) and a pure sine reads within a few percent of its amplitude.
 */
export function toneAmplitude(x: Float32Array, freq: number, rate: number): number {
  const c = 2 * Math.cos((2 * Math.PI * freq) / rate);
  let s1 = 0;
  let s2 = 0;
  for (const v of x) {
    const s = v + c * s1 - s2;
    s2 = s1;
    s1 = s;
  }
  const power = s1 * s1 + s2 * s2 - c * s1 * s2;
  return x.length ? (2 * Math.sqrt(Math.max(0, power))) / x.length : 0;
}

export interface PilotCounts {
  /** Quanta measured. */
  blocks: number;
  /** Breaks, and quanta inside them. */
  gaps: number;
  gapBlocks: number;
  /** The longest break since the previous `take`, in quanta. */
  longestGapBlocks: number;
}

/** Counts breaks in the pilot: quanta whose amplitude is below `floor`, after a settling time. */
export class PilotMeter {
  blocks = 0;
  private gaps = 0;
  private gapBlocks = 0;
  private run = 0;
  private longest = 0;

  constructor(
    private readonly floor: number,
    private readonly settleBlocks: number,
  ) {}

  push(amplitude: number): void {
    this.blocks += 1;
    if (this.blocks <= this.settleBlocks) return;
    if (amplitude >= this.floor) {
      this.run = 0;
      return;
    }
    if (this.run === 0) this.gaps += 1;
    this.run += 1;
    this.gapBlocks += 1;
    this.longest = Math.max(this.longest, this.run);
  }

  take(): PilotCounts {
    const counts = {
      blocks: this.blocks,
      gaps: this.gaps,
      gapBlocks: this.gapBlocks,
      longestGapBlocks: this.longest,
    };
    // A break still going on counts in the next stretch too.
    this.longest = this.run;
    return counts;
  }
}

/**
 * Lets speech through and silences the rest, so Opus sends almost nothing between words (design
 * §6.2). Speech is a quantum `marginDb` above a noise floor that follows the room (falls at once,
 * rises 3 dB a second) and above `minDb`; the gate stays open `holdMs` after the last one, so
 * the ends of words are kept.
 */
export class SpeechGate {
  open = false;
  private floorDb = -70;
  private holdLeft = 0;

  constructor(
    private readonly rate: number,
    private readonly holdMs = 400,
    private readonly marginDb = 12,
    private readonly minDb = -50,
  ) {}

  /** One quantum's RMS level and length; whether the gate is open after it. */
  push(rms: number, frames: number): boolean {
    const db = 20 * Math.log10(Math.max(rms, 1e-9));
    const dt = frames / this.rate;
    this.floorDb = db < this.floorDb ? db : Math.min(db, this.floorDb + 3 * dt);
    const speech = db > Math.max(this.floorDb + this.marginDb, this.minDb);
    this.holdLeft = speech ? this.holdMs / 1000 : Math.max(0, this.holdLeft - dt);
    this.open = speech || this.holdLeft > 0;
    return this.open;
  }
}
