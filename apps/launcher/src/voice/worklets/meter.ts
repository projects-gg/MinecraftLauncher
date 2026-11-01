// The pilot tone's meter (VOICE-0), an AudioWorklet: the 1 kHz tone's amplitude in every render
// quantum that arrives; a quantum far below the tone's level is a break somewhere on the way
// (capture, processing, encoding, network, jitter buffer). Its counters go to the page.
import { PILOT_HZ, PILOT_LEVEL, PilotMeter, toneAmplitude } from "../dsp";

// The AudioWorklet scope's globals.
declare const sampleRate: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: typeof AudioWorkletProcessor): void;

/** About twice a second. */
const POST_EVERY = 200;

class Meter extends AudioWorkletProcessor {
  // 12 dB under the tone; the first 3 s settle (the call starts).
  private readonly meter = new PilotMeter(PILOT_LEVEL / 4, Math.round((3 * sampleRate) / 128));

  process(inputs: Float32Array[][]): boolean {
    const x = inputs[0]?.[0];
    this.meter.push(x ? toneAmplitude(x, PILOT_HZ, sampleRate) : 0);
    if (this.meter.blocks % POST_EVERY === 0) this.port.postMessage(this.meter.take());
    return true;
  }
}

registerProcessor("pl-pilot-meter", Meter);
