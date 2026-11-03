// Who speaks (design §6.5), as the speakers worklet measures it: every person's arriving sound on
// its own input, nothing sent to any server. Senders' gates turn the quiet between words into
// silence, so anything above a low level is speech; 300 ms keeps a ring lit across syllables. Only
// changes leave the audio thread: a quiet room posts nothing. Each post carries the slot's
// generation, so one still on its way for the slot's previous person is known as stale.
//
// While the launcher shows its live meters (VOICE-CALL) each slot's loudest of the last ~60 ms
// leaves too, on the settings bar's scale in 64 steps, only when one of them moved: a room where
// nobody speaks still posts nothing.
import { dbOf, levelOf } from "./dsp";

/** Inputs per speakers node (more people take another node). */
export const SPEAKER_SLOTS = 8;
/** Arriving sound above this is speech. */
const SPEECH_DB = -50;
/** Lit this long after the last speech. */
const HOLD_S = 0.3;
/** The levels' pace: 23 quanta of 128 at 48 kHz, about sixteen a second. */
export const LEVEL_QUANTA = 23;
/** The levels' steps. */
const LEVEL_STEPS = 64;

export class SpeakersCore {
  private readonly on: Uint8Array;
  private readonly hold: Float32Array;
  private readonly generation: Uint32Array;
  /** The live meters (VOICE-CALL): on, each slot's loudest RMS so far, and what was posted. */
  private levels = false;
  private readonly peak: Float32Array;
  private readonly sent: Uint8Array;
  private quanta = 0;

  constructor(
    private readonly rate: number,
    slots: number,
    private readonly post: (slot: number, on: boolean, generation: number) => void,
    /** Each slot's level (0–1, 64 steps) and generation, when one of them moved. */
    private readonly postLevels: (levels: number[], generations: number[]) => void = () => {},
  ) {
    this.on = new Uint8Array(slots);
    this.hold = new Float32Array(slots);
    this.generation = new Uint32Array(slots);
    this.peak = new Float32Array(slots);
    this.sent = new Uint8Array(slots);
  }

  /** A slot changes hands: its state starts over, unlit, under the page's new generation. */
  reset(slot: number, generation: number): void {
    if (slot < 0 || slot >= this.on.length) return;
    this.on[slot] = 0;
    this.hold[slot] = 0;
    this.generation[slot] = generation;
    this.peak[slot] = 0;
    this.sent[slot] = 0;
  }

  /** The live meters: each slot's level while on (VOICE-CALL). */
  setLevels(on: boolean): void {
    this.levels = on;
    this.quanta = 0;
    this.peak.fill(0);
    this.sent.fill(0);
  }

  /** One quantum of every input (an input with no channels is not connected). */
  process(inputs: Float32Array[][]): void {
    for (let slot = 0; slot < this.on.length; slot++) {
      const x = inputs[slot]?.[0];
      let speech = false;
      if (x && x.length > 0) {
        let energy = 0;
        for (let i = 0; i < x.length; i++) {
          const v = x[i] ?? 0;
          energy += v * v;
        }
        const rms = Math.sqrt(energy / x.length);
        speech = dbOf(rms) > SPEECH_DB;
        if (this.levels && rms > (this.peak[slot] ?? 0)) this.peak[slot] = rms;
      }
      const left = this.hold[slot] ?? 0;
      if (speech) {
        this.hold[slot] = HOLD_S;
      } else if (left > 0) {
        this.hold[slot] = Math.max(0, left - (x?.length || 128) / this.rate);
      }
      const on = speech || (this.hold[slot] ?? 0) > 0;
      if (on !== (this.on[slot] === 1)) {
        this.on[slot] = on ? 1 : 0;
        this.post(slot, on, this.generation[slot] ?? 0);
      }
    }
    if (this.levels && ++this.quanta >= LEVEL_QUANTA) this.flushLevels();
  }

  /** Every ~60 ms: the slots' levels, if one moved by more than a step (or reached quiet). */
  private flushLevels(): void {
    this.quanta = 0;
    let moved = false;
    const steps: number[] = [];
    for (let slot = 0; slot < this.on.length; slot++) {
      const rms = this.peak[slot] ?? 0;
      this.peak[slot] = 0;
      // Only speech counts (the sender's gate leaves silence between words).
      const level = dbOf(rms) > SPEECH_DB ? levelOf(dbOf(rms)) : 0;
      const step = Math.round(level * LEVEL_STEPS);
      const before = this.sent[slot] ?? 0;
      if (Math.abs(step - before) > 1 || (step === 0) !== (before === 0)) {
        moved = true;
        this.sent[slot] = step;
      }
      steps.push(this.sent[slot] ?? 0);
    }
    if (!moved) return;
    this.postLevels(
      steps.map((step) => step / LEVEL_STEPS),
      Array.from(this.generation),
    );
  }
}
