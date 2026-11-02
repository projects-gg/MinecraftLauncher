// The voice window's own short sounds, made here (no files to ship or license): soft sine notes
// with a quick attack and a natural decay, quiet (peaks near -12 dBFS, "%50" of design §6.4).
// "waiting" is one cycle of the caller's tone, played in a loop until someone comes.

export type SoundName = "join" | "leave" | "tick" | "waiting";

interface Note {
  /** Hz; two for a chord. */
  freqs: number[];
  /** Start and length, seconds. */
  at: number;
  length: number;
  /** Peak amplitude of the note (all its frequencies together). */
  peak: number;
  /** Exponential decay's time constant (s); 0: held, with a soft release at its end. */
  decay: number;
}

const E5 = 659.25;
const A5 = 880;
const D5 = 587.33;
const C5 = 523.25;
const E6 = 1318.51;

const SOUNDS: Record<SoundName, { length: number; notes: Note[] }> = {
  // A rising fourth: someone came.
  join: {
    length: 0.3,
    notes: [
      { freqs: [E5], at: 0, length: 0.14, peak: 0.2, decay: 0.05 },
      { freqs: [A5], at: 0.085, length: 0.2, peak: 0.22, decay: 0.06 },
    ],
  },
  // Falling: someone left.
  leave: {
    length: 0.3,
    notes: [
      { freqs: [A5], at: 0, length: 0.14, peak: 0.2, decay: 0.05 },
      { freqs: [D5], at: 0.085, length: 0.2, peak: 0.2, decay: 0.06 },
    ],
  },
  // A short, high tick for a shortcut's mute or deafen.
  tick: {
    length: 0.05,
    notes: [{ freqs: [E6], at: 0, length: 0.045, peak: 0.14, decay: 0.01 }],
  },
  // The caller's tone: a soft third twice, then quiet; three seconds a cycle.
  waiting: {
    length: 3,
    notes: [
      { freqs: [C5, E5], at: 0, length: 0.32, peak: 0.16, decay: 0 },
      { freqs: [C5, E5], at: 0.45, length: 0.32, peak: 0.16, decay: 0 },
    ],
  },
};

const ATTACK_S = 0.006;
const RELEASE_S = 0.04;

/** The sound's samples (mono) at `rate`. */
export function renderSound(name: SoundName, rate: number): Float32Array<ArrayBuffer> {
  const sound = SOUNDS[name];
  const out = new Float32Array(Math.round(sound.length * rate));
  for (const note of sound.notes) {
    const start = Math.round(note.at * rate);
    const frames = Math.min(Math.round(note.length * rate), out.length - start);
    const share = note.peak / note.freqs.length;
    for (let i = 0; i < frames; i++) {
      const t = i / rate;
      const attack = Math.min(1, t / ATTACK_S);
      const left = note.length - t;
      const release = Math.min(1, left / (note.decay > 0 ? 0.003 : RELEASE_S));
      const decay = note.decay > 0 ? Math.exp(-t / note.decay) : 1;
      const envelope = attack * release * decay;
      let v = 0;
      for (const freq of note.freqs) {
        const phase = 2 * Math.PI * freq * t;
        // A little of the octave for warmth.
        v += Math.sin(phase) + 0.12 * Math.sin(2 * phase);
      }
      const at = start + i;
      out[at] = (out[at] ?? 0) + (share / 1.12) * envelope * v;
    }
  }
  return out;
}

/** Whether the sound repeats until stopped. */
export function loops(name: SoundName): boolean {
  return name === "waiting";
}
