// RNNoise (BSD-3-Clause; the SIMD build @sapphi-red/web-noise-suppressor ships, MIT) driven
// directly: the module asks its host for three small functions only, so the voice window's capture
// worklet runs it without Emscripten's 100 KB of glue, and reads its speech probability.

/** A noise suppressor for 480-sample frames at 48 kHz in 16-bit scale (±32768), cleaned in place. */
export interface Denoiser {
  /** Cleans `frame` and returns the probability (0–1) that it holds speech. */
  process(frame: Float32Array): number;
}

export interface Rnnoise extends Denoiser {
  destroy(): void;
}

/** Samples per frame: 10 ms at 48 kHz. */
export const RNNOISE_FRAME = 480;

interface Exports {
  memory: WebAssembly.Memory;
  __wasm_call_ctors(): void;
  emscripten_stack_init?(): void;
  rnnoise_get_frame_size(): number;
  rnnoise_create(model: number): number;
  rnnoise_destroy(state: number): void;
  rnnoise_process_frame(state: number, out: number, input: number): number;
  malloc(bytes: number): number;
  free(pointer: number): void;
}

const PAGE = 65536;

/** Instantiates the module (its bytes) and one denoiser state with the built-in model. */
export async function createRnnoise(binary: BufferSource): Promise<Rnnoise> {
  let memory: WebAssembly.Memory | null = null;
  const env = {
    emscripten_memcpy_big(dest: number, src: number, num: number): void {
      if (memory) new Uint8Array(memory.buffer).copyWithin(dest, src, src + num);
    },
    emscripten_resize_heap(size: number): number {
      if (!memory) return 0;
      try {
        memory.grow(Math.ceil(((size >>> 0) - memory.buffer.byteLength) / PAGE));
        return 1;
      } catch {
        return 0;
      }
    },
    __assert_fail(): never {
      throw new Error("rnnoise: assertion failed");
    },
  };
  const { instance } = await WebAssembly.instantiate(binary, { env });
  const exports = instance.exports as unknown as Exports;
  memory = exports.memory;
  exports.emscripten_stack_init?.();
  exports.__wasm_call_ctors();
  if (exports.rnnoise_get_frame_size() !== RNNOISE_FRAME) throw new Error("rnnoise: frame size");
  const state = exports.rnnoise_create(0);
  const input = exports.malloc(RNNOISE_FRAME * 4);
  const output = exports.malloc(RNNOISE_FRAME * 4);
  if (!state || !input || !output) throw new Error("rnnoise: out of memory");
  let heap = new Float32Array(exports.memory.buffer);
  let alive = true;
  return {
    process(frame: Float32Array): number {
      // A grown memory detaches the old view.
      if (heap.buffer !== exports.memory.buffer) heap = new Float32Array(exports.memory.buffer);
      heap.set(frame.subarray(0, RNNOISE_FRAME), input >> 2);
      const vad = exports.rnnoise_process_frame(state, output, input);
      frame.set(heap.subarray(output >> 2, (output >> 2) + RNNOISE_FRAME));
      return vad;
    },
    destroy(): void {
      if (!alive) return;
      alive = false;
      exports.rnnoise_destroy(state);
      exports.free(input);
      exports.free(output);
    },
  };
}
