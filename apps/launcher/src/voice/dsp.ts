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
 * VOICE-0's energy gate (the probe's): speech is a quantum `marginDb` above a noise floor that
 * follows the room (falls at once, rises 3 dB a second) and above `minDb`; open `holdMs` after it.
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

/** dBFS of an RMS amplitude (digital silence reads -180). */
export function dbOf(rms: number): number {
  return 20 * Math.log10(Math.max(rms, 1e-9));
}

/** The settings page's level bar: -60 dBFS reads 0, 0 dBFS reads 1 (a dB scale, as ears hear). */
export const LEVEL_FLOOR_DB = -60;

/** A level in dBFS on the bar's 0–1 scale. */
export function levelOf(db: number): number {
  return Math.min(1, Math.max(0, 1 - db / LEVEL_FLOOR_DB));
}

/**
 * The manual threshold ("Hassasiyet": 0 lets everything through, 100 nothing) in dBFS, on the
 * bar's own scale: a level of `threshold / 100` is exactly the threshold, so what the slider says
 * and when the bar turns green agree.
 */
export function thresholdDb(threshold: number): number {
  if (threshold <= 0) return Number.NEGATIVE_INFINITY;
  return LEVEL_FLOOR_DB * (1 - Math.min(100, threshold) / 100);
}

/** How the voice gate decides (design §6.2), in one place. */
export const GATE = {
  /** Speech keeps the gate open this long after it (the ends of words). */
  holdS: 0.4,
  /** Automatic: speech is this far above the room's noise floor... */
  marginDb: 12,
  /** ...and above this, whatever the room. */
  minDb: -55,
  /** The noise suppressor already runs this much below where speech starts (warm for the onset). */
  warmDb: 6,
  /** The suppressor's speech probability that opens the gate, and that keeps it open. */
  vadOpen: 0.6,
  vadStay: 0.3,
  /** The noise floor rises this fast (dB a second) and never sits below `floorMinDb`. */
  riseDbPerS: 3,
  floorMinDb: -90,
  /** Below this a frame is digital silence (a device starting, a muted input), not the room. */
  silenceDb: -100,
} as const;

/**
 * Voice activity, one 10 ms frame at a time. Automatic: a frame is speech when it stands
 * `marginDb` above the room's noise floor (which falls at once and rises slowly, so it settles on
 * the quiet between words) and above `minDb`, and, when the noise suppressor ran, when its speech
 * probability agrees (keyboards, clicks and fans are loud but not speech). Manual: the frame's level
 * against the threshold. The gate stays open `holdS` after the last speech.
 */
export class VoiceGate {
  open = false;
  /** null: automatic; else the threshold 0–100. */
  threshold: number | null = null;
  private wasOpen = false;
  private floorDb = Number.POSITIVE_INFINITY;
  /** Frames the gate stays open without speech. */
  private holdLeft = 0;
  private readonly holdFrames: number;

  constructor(private readonly frameSeconds: number) {
    this.holdFrames = Math.round(GATE.holdS / frameSeconds);
  }

  /**
   * Whether the noise suppressor should hear a frame at `db`: it may be speech, or the gate is (or
   * was a frame ago) open, so what is sent is always cleaned. Quiet frames never wake it.
   */
  wantsDenoise(db: number): boolean {
    if (this.open || this.wasOpen) return true;
    if (this.threshold !== null) return db >= thresholdDb(this.threshold) - GATE.warmDb;
    const floor = Number.isFinite(this.floorDb) ? this.floorDb : db;
    return db > Math.max(floor + GATE.marginDb, GATE.minDb) - GATE.warmDb;
  }

  /** A frame's level and the suppressor's speech probability (null: it did not run). */
  push(db: number, vad: number | null): boolean {
    this.wasOpen = this.open;
    const dt = this.frameSeconds;
    if (db > GATE.silenceDb) {
      this.floorDb = Math.max(
        GATE.floorMinDb,
        db < this.floorDb ? db : Math.min(db, this.floorDb + GATE.riseDbPerS * dt),
      );
    }
    let speech: boolean;
    if (this.threshold !== null) {
      speech = db >= thresholdDb(this.threshold);
    } else {
      const loud = db > Math.max(this.floorDb + GATE.marginDb, GATE.minDb);
      speech = loud && (vad === null || vad >= (this.open ? GATE.vadStay : GATE.vadOpen));
    }
    if (speech) {
      this.holdLeft = this.holdFrames;
      this.open = true;
    } else if (this.holdLeft > 0) {
      this.holdLeft -= 1;
      this.open = true;
    } else {
      this.open = false;
    }
    return this.open;
  }

  /** Shut, keeping what it learned of the room (the microphone was idle a while). */
  close(): void {
    this.open = false;
    this.wasOpen = false;
    this.holdLeft = 0;
  }

  /** Forgets the room too (another microphone). */
  reset(): void {
    this.close();
    this.floorDb = Number.POSITIVE_INFINITY;
  }
}
