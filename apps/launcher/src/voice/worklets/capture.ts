// The microphone's worklet (capture-core.ts): output 0 is sent, output 1 plays back during
// "Mikrofonu dene". RNNoise's bytes come with the node; it loads here, and until it does (or if it
// cannot) the gate decides by level alone and the page turns the built-in suppressor on.
import { type CaptureConfig, CaptureCore } from "../capture-core";
import { createRnnoise, type Rnnoise } from "../rnnoise";

// The AudioWorklet scope's globals.
declare const sampleRate: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(
  name: string,
  processor: new (options: { processorOptions?: CaptureOptions }) => AudioWorkletProcessor,
): void;

export interface CaptureOptions {
  /** RNNoise's module (none: no noise suppressor here). */
  wasm: ArrayBuffer | null;
  config: CaptureConfig;
}

export type CaptureMessage =
  | { t: "config"; config: CaptureConfig }
  | { t: "talk"; open: boolean; tail: boolean }
  | { t: "levels"; on: boolean }
  | { t: "test"; on: boolean }
  | { t: "mic" }
  | { t: "close" };

export type CapturePost =
  | { t: "ns"; ok: boolean }
  | { t: "me"; on: boolean }
  | { t: "level"; level: number; open: boolean }
  | { t: "slow" };

/** The audio thread's clock: `performance` where the scope has it, else the date (1 ms steps). */
const clock: () => number =
  typeof performance === "object" && typeof performance?.now === "function"
    ? () => performance.now()
    : () => Date.now();

class Capture extends AudioWorkletProcessor {
  private readonly core: CaptureCore;
  private rnnoise: Rnnoise | null = null;
  private closed = false;

  constructor(options: { processorOptions?: CaptureOptions }) {
    super();
    const post = (message: CapturePost) => this.port.postMessage(message);
    this.core = new CaptureCore(
      sampleRate,
      {
        me: (on) => post({ t: "me", on }),
        level: (level, open) => post({ t: "level", level, open }),
        slow: () => post({ t: "slow" }),
      },
      clock,
    );
    const init = options.processorOptions;
    if (init) this.core.configure(init.config);
    this.port.onmessage = (event: MessageEvent<CaptureMessage>) => this.receive(event.data);
    if (!init?.wasm) {
      post({ t: "ns", ok: false });
      return;
    }
    createRnnoise(init.wasm).then(
      (rnnoise) => {
        if (this.closed) {
          rnnoise.destroy();
          return;
        }
        this.rnnoise = rnnoise;
        this.core.setDenoiser(rnnoise);
        post({ t: "ns", ok: true });
      },
      () => post({ t: "ns", ok: false }),
    );
  }

  private receive(message: CaptureMessage): void {
    switch (message.t) {
      case "config":
        this.core.configure(message.config);
        break;
      case "talk":
        this.core.talk(message.open, message.tail);
        break;
      case "levels":
        this.core.setLevels(message.on);
        break;
      case "test":
        this.core.setTest(message.on);
        break;
      case "mic":
        this.core.newMicrophone();
        break;
      case "close":
        this.closed = true;
        this.core.setDenoiser(null);
        this.rnnoise?.destroy();
        this.rnnoise = null;
        break;
    }
  }

  process(inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
    if (this.closed) return false;
    const send = outputs[0]?.[0];
    if (send) this.core.process(inputs[0]?.[0], send, outputs[1]?.[0] ?? null);
    return true;
  }
}

registerProcessor("pl-voice-capture", Capture);
