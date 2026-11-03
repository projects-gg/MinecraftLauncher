// Who speaks (speakers-core.ts): one input per person, no outputs; posts a slot only when it lights
// up or goes out, with the slot's generation (the page drops posts of a slot's previous person).
// While the launcher shows its live meters (VOICE-CALL) the slots' levels come too, only on change.
import { SPEAKER_SLOTS, SpeakersCore } from "../speakers-core";

// The AudioWorklet scope's globals.
declare const sampleRate: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: typeof AudioWorkletProcessor): void;

export type SpeakersMessage =
  | { t: "reset"; slot: number; generation: number }
  | { t: "levels"; on: boolean }
  | { t: "close" };
export type SpeakersPost =
  | { t: "on"; slot: number; on: boolean; generation: number }
  | { t: "levels"; levels: number[]; generations: number[] };

class Speakers extends AudioWorkletProcessor {
  private readonly core = new SpeakersCore(
    sampleRate,
    SPEAKER_SLOTS,
    (slot, on, generation) =>
      this.port.postMessage({ t: "on", slot, on, generation } satisfies SpeakersPost),
    (levels, generations) =>
      this.port.postMessage({ t: "levels", levels, generations } satisfies SpeakersPost),
  );
  private closed = false;

  constructor() {
    super();
    this.port.onmessage = (event: MessageEvent<SpeakersMessage>) => {
      const message = event.data;
      if (message.t === "reset") this.core.reset(message.slot, message.generation);
      else if (message.t === "levels") this.core.setLevels(message.on);
      else this.closed = true;
    };
  }

  process(inputs: Float32Array[][]): boolean {
    if (this.closed) return false;
    this.core.process(inputs);
    return true;
  }
}

registerProcessor("pl-voice-speakers", Speakers);
