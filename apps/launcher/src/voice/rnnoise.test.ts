import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";
import { createRnnoise, RNNOISE_FRAME } from "./rnnoise";

const require = createRequire(import.meta.url);
const wasm = readFileSync(require.resolve("@sapphi-red/web-noise-suppressor/rnnoise_simd.wasm"));

/** A deterministic noise source, -1 to 1. */
function noise(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return (state / 2 ** 32 - 0.5) * 2;
  };
}

/** A room's low rumble (fans, a PC), 16-bit scale: about -35 dBFS. */
function rumble(): (i: number) => number {
  const next = noise(5);
  let y = 0;
  return () => {
    y = 0.995 * y + next() * 0.1;
    return y * 1000;
  };
}

/** A crude voiced sound: 140 Hz harmonics with vibrato and syllables (16-bit scale). */
function voiced(): (i: number) => number {
  let phase = 0;
  return (i) => {
    const t = i / 48000;
    phase += (2 * Math.PI * (140 + 10 * Math.sin(2 * Math.PI * 5 * t))) / 48000;
    let v = 0;
    for (let h = 1; h < 25; h++) v += (Math.sin(h * phase) / h) * (h < 4 ? 1 : 0.6);
    return v * 3000 * (0.5 + 0.5 * Math.sin(2 * Math.PI * 3 * t));
  };
}

/** Runs 4 s of `signal`; the change in energy (dB) and the speech probability after the first 1.5 s. */
async function through(signal: (i: number) => number) {
  const rnnoise = await createRnnoise(wasm);
  let inEnergy = 0;
  let outEnergy = 0;
  let vadSum = 0;
  let vadMax = 0;
  let counted = 0;
  for (let n = 0; n < 400; n++) {
    const frame = new Float32Array(RNNOISE_FRAME);
    for (let i = 0; i < frame.length; i++) frame[i] = signal(n * RNNOISE_FRAME + i);
    let before = 0;
    for (const v of frame) before += v * v;
    const vad = rnnoise.process(frame);
    expect(vad).toBeGreaterThanOrEqual(0);
    expect(vad).toBeLessThanOrEqual(1);
    if (n < 150) continue;
    let after = 0;
    for (const v of frame) after += v * v;
    inEnergy += before;
    outEnergy += after;
    vadSum += vad;
    vadMax = Math.max(vadMax, vad);
    counted += 1;
  }
  rnnoise.destroy();
  rnnoise.destroy();
  return { db: 10 * Math.log10(outEnergy / inEnergy), vad: vadSum / counted, vadMax };
}

describe("createRnnoise", () => {
  it("runs the module with its three imports and removes a room's rumble", async () => {
    const { db, vadMax } = await through(rumble());
    expect(db).toBeLessThan(-20);
    expect(vadMax).toBeLessThan(0.1);
  });

  it("keeps a voiced sound and calls it speech", async () => {
    const { db, vad } = await through(voiced());
    expect(db).toBeGreaterThan(-3);
    expect(vad).toBeGreaterThan(0.6);
  });

  it("gives silence back for silence", async () => {
    const rnnoise = await createRnnoise(wasm);
    const frame = new Float32Array(RNNOISE_FRAME);
    for (let n = 0; n < 20; n++) rnnoise.process(frame);
    let peak = 0;
    for (const v of frame) peak = Math.max(peak, Math.abs(v));
    expect(peak).toBeLessThan(1);
    rnnoise.destroy();
  });

  it("refuses bytes that are not the module", async () => {
    await expect(createRnnoise(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]))).rejects.toThrow();
  });
});
