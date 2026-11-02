// The speech gate, an AudioWorklet: speech passes, the rest becomes silence (5 ms fades), so
// Opus sends almost nothing between words (design §6.2). It tells the page how much was open.
import { SpeechGate } from "../dsp";

// The AudioWorklet scope's globals.
declare const sampleRate: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: typeof AudioWorkletProcessor): void;

/** About once a second. */
const POST_EVERY = 375;

class Gate extends AudioWorkletProcessor {
  private readonly gate = new SpeechGate(sampleRate);
  private readonly step = 1 / (0.005 * sampleRate);
  private gain = 0;
  private blocks = 0;
  private openBlocks = 0;

  process(inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
    const x = inputs[0]?.[0];
    const y = outputs[0]?.[0];
    if (!y) return true;
    if (!x) {
      y.fill(0);
      return true;
    }
    let energy = 0;
    for (const v of x) energy += v * v;
    const open = this.gate.push(Math.sqrt(energy / x.length), x.length);
    const target = open ? 1 : 0;
    for (let i = 0; i < y.length; i++) {
      this.gain += Math.max(-this.step, Math.min(this.step, target - this.gain));
      y[i] = (x[i] ?? 0) * this.gain;
    }
    this.blocks += 1;
    if (open) this.openBlocks += 1;
    if (this.blocks % POST_EVERY === 0) {
      this.port.postMessage({ blocks: this.blocks, openBlocks: this.openBlocks });
    }
    return true;
  }
}

registerProcessor("pl-speech-gate", Gate);
