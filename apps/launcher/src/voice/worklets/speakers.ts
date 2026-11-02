// Who speaks (speakers-core.ts): one input per person, no outputs; posts a slot only when it lights
// up or goes out.
import { SPEAKER_SLOTS, SpeakersCore } from "../speakers-core";

// The AudioWorklet scope's globals.
declare const sampleRate: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: typeof AudioWorkletProcessor): void;

export type SpeakersMessage = { t: "reset"; slot: number } | { t: "close" };
export type SpeakersPost = { slot: number; on: boolean };

class Speakers extends AudioWorkletProcessor {
  private readonly core = new SpeakersCore(sampleRate, SPEAKER_SLOTS, (slot, on) =>
    this.port.postMessage({ slot, on } satisfies SpeakersPost),
  );
  private closed = false;

  constructor() {
    super();
    this.port.onmessage = (event: MessageEvent<SpeakersMessage>) => {
      if (event.data.t === "reset") this.core.reset(event.data.slot);
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
