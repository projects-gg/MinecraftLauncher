// The voice window's own short sounds, made here (no files to ship or license): one family of soft
// sine notes with a little of their octave, a quick attack and a natural decay, quiet (peaks near
// -12 dBFS and under, "%50" of design §6.4), all in A major so any two of them sit well together
// (VOICE-CALL). Rising says "on" or "in", falling "off" or "out": someone came (two notes up) or
// left (the same two down), the player got in (three up) or left (three down), the microphone (a
// short high pair) and the sound (a low pair). A dropped connection leans out of the key on purpose.
// "waiting" is one cycle of the caller's tone, played in a loop until someone comes.
//
// Screen sharing (SHARE-LIVE) lands on two notes at once, an open chord, which none of the others
// do: the player's own stream high (a note up into the chord when it goes live, the chord down to the
// note when it ends), someone else's an octave lower and softer (going live; the end of a stream the
// player watched is the softest of all).

export type SoundName =
  | "join"
  | "leave"
  | "micOff"
  | "micOn"
  | "deafOn"
  | "deafOff"
  | "connected"
  | "ended"
  | "lost"
  | "waiting"
  | "shareStart"
  | "shareStop"
  | "streamStart"
  | "streamEnd";

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

const E4 = 329.63;
const A4 = 440;
const C5 = 523.25;
const CS5 = 554.37;
const E5 = 659.25;
const A5 = 880;
const B5 = 987.77;
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
  // The same fourth, falling: someone left.
  leave: {
    length: 0.3,
    notes: [
      { freqs: [A5], at: 0, length: 0.14, peak: 0.2, decay: 0.05 },
      { freqs: [E5], at: 0.085, length: 0.2, peak: 0.19, decay: 0.06 },
    ],
  },
  // A short high pair, falling: the microphone is off.
  micOff: {
    length: 0.13,
    notes: [
      { freqs: [E6], at: 0, length: 0.05, peak: 0.11, decay: 0.014 },
      { freqs: [B5], at: 0.048, length: 0.075, peak: 0.12, decay: 0.02 },
    ],
  },
  // The pair rising: the microphone is on.
  micOn: {
    length: 0.13,
    notes: [
      { freqs: [B5], at: 0, length: 0.05, peak: 0.11, decay: 0.014 },
      { freqs: [E6], at: 0.048, length: 0.075, peak: 0.12, decay: 0.02 },
    ],
  },
  // A low pair, falling: the sound is off (deafened).
  deafOn: {
    length: 0.3,
    notes: [
      { freqs: [A4], at: 0, length: 0.12, peak: 0.17, decay: 0.045 },
      { freqs: [E4], at: 0.07, length: 0.22, peak: 0.18, decay: 0.07 },
    ],
  },
  // The low pair rising: the sound is back.
  deafOff: {
    length: 0.3,
    notes: [
      { freqs: [E4], at: 0, length: 0.12, peak: 0.17, decay: 0.045 },
      { freqs: [A4], at: 0.07, length: 0.22, peak: 0.18, decay: 0.07 },
    ],
  },
  // The triad rising: in (a call answered, a room joined, the connection back).
  connected: {
    length: 0.42,
    notes: [
      { freqs: [CS5], at: 0, length: 0.12, peak: 0.15, decay: 0.045 },
      { freqs: [E5], at: 0.07, length: 0.12, peak: 0.16, decay: 0.045 },
      { freqs: [A5], at: 0.14, length: 0.26, peak: 0.2, decay: 0.08 },
    ],
  },
  // The triad falling, a little slower: out (the player left, the call ended).
  ended: {
    length: 0.5,
    notes: [
      { freqs: [A5], at: 0, length: 0.13, peak: 0.16, decay: 0.05 },
      { freqs: [E5], at: 0.085, length: 0.13, peak: 0.15, decay: 0.05 },
      { freqs: [CS5], at: 0.17, length: 0.3, peak: 0.16, decay: 0.09 },
    ],
  },
  // Down to a note outside the key: the connection dropped.
  lost: {
    length: 0.48,
    notes: [
      { freqs: [E5], at: 0, length: 0.16, peak: 0.15, decay: 0.06 },
      { freqs: [C5], at: 0.12, length: 0.34, peak: 0.16, decay: 0.1 },
    ],
  },
  // The caller's tone: a soft major third twice, then quiet; three seconds a cycle.
  waiting: {
    length: 3,
    notes: [
      { freqs: [A4, CS5], at: 0, length: 0.32, peak: 0.16, decay: 0 },
      { freqs: [A4, CS5], at: 0.45, length: 0.32, peak: 0.16, decay: 0 },
    ],
  },
  // A note up into an open fifth, high: the player's stream is live.
  shareStart: {
    length: 0.45,
    notes: [
      { freqs: [E5], at: 0, length: 0.11, peak: 0.14, decay: 0.045 },
      { freqs: [A5, E6], at: 0.075, length: 0.34, peak: 0.2, decay: 0.1 },
    ],
  },
  // The fifth down to the note: the player's stream ended.
  shareStop: {
    length: 0.45,
    notes: [
      { freqs: [A5, E6], at: 0, length: 0.12, peak: 0.16, decay: 0.05 },
      { freqs: [E5], at: 0.085, length: 0.3, peak: 0.16, decay: 0.09 },
    ],
  },
  // An octave lower and softer: someone else in the room went live.
  streamStart: {
    length: 0.4,
    notes: [
      { freqs: [A4], at: 0, length: 0.11, peak: 0.13, decay: 0.045 },
      { freqs: [E5, A5], at: 0.075, length: 0.3, peak: 0.17, decay: 0.09 },
    ],
  },
  // The softest: the stream the player watched ended.
  streamEnd: {
    length: 0.36,
    notes: [
      { freqs: [E5, A5], at: 0, length: 0.1, peak: 0.12, decay: 0.04 },
      { freqs: [A4], at: 0.075, length: 0.26, peak: 0.11, decay: 0.08 },
    ],
  },
};

const ATTACK_S = 0.006;
const RELEASE_S = 0.04;

/** Every sound's name (tests, and the page draws them once each). */
export const SOUND_NAMES = Object.keys(SOUNDS) as SoundName[];

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

/**
 * Sounds of the player's own state (VOICE-CALL): they play while deafened too (the player's own
 * stream going live and ending among them); someone coming, leaving, going live, the end of a
 * stream they share and the caller's tone do not.
 */
export function playsWhileDeaf(name: SoundName | "stop"): boolean {
  return (
    name !== "join" &&
    name !== "leave" &&
    name !== "waiting" &&
    name !== "streamStart" &&
    name !== "streamEnd"
  );
}
