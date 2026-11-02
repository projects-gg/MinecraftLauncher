// Who speaks (design §6.5), as the speakers worklet measures it: every person's arriving sound on
// its own input, nothing sent to any server. Senders' gates turn the quiet between words into
// silence, so anything above a low level is speech; 300 ms keeps a ring lit across syllables. Only
// changes leave the audio thread: a quiet room posts nothing.
import { dbOf } from "./dsp";

/** Inputs per speakers node (more people take another node). */
export const SPEAKER_SLOTS = 8;
/** Arriving sound above this is speech. */
const SPEECH_DB = -50;
/** Lit this long after the last speech. */
const HOLD_S = 0.3;

export class SpeakersCore {
  private readonly on: Uint8Array;
  private readonly hold: Float32Array;

  constructor(
    private readonly rate: number,
    slots: number,
    private readonly post: (slot: number, on: boolean) => void,
  ) {
    this.on = new Uint8Array(slots);
    this.hold = new Float32Array(slots);
  }

  /** A slot changes hands: its state starts over, unlit (the page dropped the old person's). */
  reset(slot: number): void {
    if (slot < 0 || slot >= this.on.length) return;
    this.on[slot] = 0;
    this.hold[slot] = 0;
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
        speech = dbOf(Math.sqrt(energy / x.length)) > SPEECH_DB;
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
        this.post(slot, on);
      }
    }
  }
}
