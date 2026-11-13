// A watched share's sound (SQOL-U1; sqol.md §1.1 "Ses yolu"): the `<video>` plays muted (it only
// shows the picture), and each stream's sound runs through WebAudio, so it can go up to 200 % and
// duck while someone talks:
//
//   stream → its gain ("Yayın Sesi" × "Herkesin Sesi", the cap at most; a limiter above 100 %)
//          → the shared duck gain ("Biri Konuşunca Yayın Sesini Kıs") → a limiter → the output
//
// The output is voice chat's ("Ses çıkışı"; the system's default while that device is gone). The
// context opens in idle time before a stream comes (someone in the room went live, a watch began),
// never on a press: the first one opens the audio device on the page's main thread (design.md §8).
// With nothing to play it rests (suspended: no audio thread waking), and it closes when the room no
// longer shares. Ducking only lowers the streams the player watches: never the game, never the
// system. Without WebAudio each stream plays through its own element (100 % at most).

/** The sound a person's stream plays at, as the player set it ("Yayın Sesi"). */
export interface StreamSound {
  /** Percent, 0 to the cap. */
  volume: number;
  muted: boolean;
}

export const DEFAULT_STREAM_SOUND: StreamSound = { volume: 100, muted: false };

/** What a stream's gain depends on besides its own volume. */
export interface StreamLevelEnv {
  /** "Herkesin Sesi", percent 0–200. */
  everyone: number;
  deaf: boolean;
  /** The volume's cap, percent (200, or 100 where WebAudio is missing). */
  volumeMax: number;
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

/** A stream's gain: its volume times "Herkesin Sesi", the cap at most; none muted or deafened. */
export function streamGain(sound: StreamSound, env: StreamLevelEnv): number {
  if (sound.muted || env.deaf) return 0;
  const cap = clamp(env.volumeMax, 0, 200) / 100;
  const volume = clamp(sound.volume, 0, 200) / 100;
  const everyone = clamp(env.everyone, 0, 200) / 100;
  return Math.min(cap, volume * everyone);
}

/** "Biri Konuşunca Yayın Sesini Kıs" and its two choices. */
export interface DuckEnv {
  duck: boolean;
  /** "Kısma Miktarı": how much quieter, percent 20–90 (60 ≈ −8 dB). */
  duckLevel: number;
  /** "Ben Konuşunca da Kıs". */
  duckSelf: boolean;
}

export const DUCK_LEVEL_MIN = 20;
export const DUCK_LEVEL_MAX = 90;

/**
 * The shared gain the streams pass through now: lower while someone else in the room speaks (whom
 * the player muted is never measured, voice chat's rule), or the player too with "Ben Konuşunca da
 * Kıs".
 */
export function duckGain(
  env: DuckEnv,
  speaking: readonly string[],
  meTalking: boolean,
  me: string | null,
): number {
  if (!env.duck) return 1;
  const others = speaking.some((id) => id !== me);
  if (!others && !(env.duckSelf && meTalking)) return 1;
  return 1 - clamp(env.duckLevel, DUCK_LEVEL_MIN, DUCK_LEVEL_MAX) / 100;
}

/** Ducking goes down this fast (most of the way)… */
export const DUCK_ATTACK_S = 0.08;
/** …and comes back up this slowly (OBS guides: attack 50–200 ms, release 500–850 ms). */
export const DUCK_RELEASE_S = 0.7;

/**
 * The duck gain's next move: none when it is there already; down with the attack, up with the
 * release. `tau` is `setTargetAtTime`'s time constant: about 95 % of the way after three of them.
 */
export function duckMove(from: number, to: number): { to: number; tau: number } | null {
  if (Math.abs(from - to) < 1e-4) return null;
  return { to, tau: (to < from ? DUCK_ATTACK_S : DUCK_RELEASE_S) / 3 };
}

/** The output the streams play on: the chosen one while the system has it, else the default (""). */
export function sinkFor(device: string | null, outputs: readonly string[] | null): string {
  if (!device) return "";
  if (outputs && !outputs.includes(device)) return "";
  return device;
}

/** A volume change glides (no click). */
const GLIDE_S = 0.015;
/** With nothing to play the device rests after this long (a layout change re-attaches at once). */
const REST_MS = 2000;

/** A limiter as voice chat's (`voice/graph.ts` `limiterOf`). */
function limiterOf(ctx: AudioContext): DynamicsCompressorNode {
  const node = ctx.createDynamicsCompressor();
  node.threshold.value = -3;
  node.knee.value = 0;
  node.ratio.value = 20;
  node.attack.value = 0.003;
  node.release.value = 0.1;
  return node;
}

/** A stream that should play now. */
export interface WantedStream {
  key: string;
  stream: MediaStream;
  /** Its gain (`streamGain`). */
  level: number;
}

/** What the engine needs of the browser (tests stand in). */
export interface StreamAudioHost {
  createContext(): AudioContext;
  createElement(): HTMLAudioElement;
  createStream(tracks: MediaStreamTrack[]): MediaStream;
  setTimeout(work: () => void, ms: number): number;
  clearTimeout(id: number): void;
  /** Waits for the player's next press or key (a context the browser kept suspended resumes then). */
  onInput(work: () => void): () => void;
}

const browserHost: StreamAudioHost = {
  createContext: () => new AudioContext({ latencyHint: "interactive" }),
  createElement: () => new Audio(),
  createStream: (tracks) => new MediaStream(tracks),
  setTimeout: (work, ms) => window.setTimeout(work, ms),
  clearTimeout: (id) => window.clearTimeout(id),
  onInput: (work) => {
    const stop = () => {
      window.removeEventListener("pointerdown", once, true);
      window.removeEventListener("keydown", once, true);
    };
    function once() {
      stop();
      work();
    }
    window.addEventListener("pointerdown", once, true);
    window.addEventListener("keydown", once, true);
    return stop;
  },
};

type Sinkable = { setSinkId?: (id: string) => Promise<void> };

/** An element or a context on `id`, the default when that fails. */
async function sinkTo(target: unknown, id: string): Promise<string> {
  const sinkable = target as Sinkable;
  if (typeof sinkable.setSinkId !== "function") return id;
  try {
    await sinkable.setSinkId(id);
    return id;
  } catch {
    if (id !== "") await sinkable.setSinkId("").catch(() => {});
    return "";
  }
}

interface Entry {
  track: MediaStreamTrack;
  /** Plays the track muted: Chromium hands a remote track to WebAudio only while an element plays it. */
  element: HTMLAudioElement;
  source: MediaStreamAudioSourceNode | null;
  gain: GainNode | null;
  /** Above 100 % a stream's own limiter keeps its peaks from pressing the other one down. */
  boost: DynamicsCompressorNode | null;
  level: number;
}

export class StreamAudio {
  private ctx: AudioContext | null = null;
  private duckNode: GainNode | null = null;
  private readonly entries = new Map<string, Entry>();
  private duck = 1;
  private sinkWanted = "";
  private sinkNow = "";
  private sinkWork: Promise<void> = Promise.resolve();
  /** No WebAudio here: each stream plays through its element. */
  private plain = false;
  private restTimer: number | null = null;
  private waking = false;
  private stopInput: (() => void) | null = null;
  private disposed = false;

  constructor(private readonly host: StreamAudioHost = browserHost) {}

  /** The streams can go above 100 % (false: their elements play them, 100 % at most). */
  get boosted(): boolean {
    return !this.plain;
  }

  /** Opens the sound ahead of time: call it in idle time, never on a press. */
  prepare(): void {
    if (this.ctx || this.plain || this.disposed) return;
    let ctx: AudioContext;
    try {
      ctx = this.host.createContext();
    } catch {
      this.plain = true;
      return;
    }
    const limiter = limiterOf(ctx);
    limiter.connect(ctx.destination);
    const duck = ctx.createGain();
    duck.gain.value = this.duck;
    duck.connect(limiter);
    this.ctx = ctx;
    this.duckNode = duck;
    this.sinkNow = "";
    this.applySink();
    // Nothing to play yet: the device rests until a stream comes (a stream that comes meanwhile
    // keeps it awake: the rest waits a moment and looks again).
    this.rest();
  }

  /** What plays now: these streams at these gains, through `duck`, on `sink`. */
  sync(wanted: readonly WantedStream[], duck: number, sink: string): void {
    if (this.disposed) return;
    this.sinkWanted = sink;
    const keep = new Set<string>();
    for (const want of wanted) {
      const track = want.stream.getAudioTracks()[0];
      if (!track || track.readyState === "ended") continue;
      keep.add(want.key);
      const entry = this.entries.get(want.key);
      if (entry && entry.track === track) {
        this.setLevel(entry, want.level);
        continue;
      }
      if (entry) this.drop(want.key, entry);
      this.add(want.key, track, want.level);
    }
    for (const [key, entry] of [...this.entries]) {
      if (!keep.has(key)) this.drop(key, entry);
    }
    this.setDuck(duck);
    this.applySink();
    this.rest();
  }

  /** Everything stops and the device closes (the room no longer shares). */
  close(): void {
    for (const [key, entry] of [...this.entries]) this.drop(key, entry);
    if (this.restTimer !== null) this.host.clearTimeout(this.restTimer);
    this.restTimer = null;
    this.stopInput?.();
    this.stopInput = null;
    const ctx = this.ctx;
    this.ctx = null;
    this.duckNode = null;
    this.sinkNow = "";
    if (ctx) void ctx.close().catch(() => {});
  }

  /** The engine is done for good (its page went away). */
  dispose(): void {
    this.close();
    this.disposed = true;
  }

  private add(key: string, track: MediaStreamTrack, level: number): void {
    if (!this.ctx && !this.plain) this.prepare();
    const element = this.host.createElement();
    const stream = this.host.createStream([track]);
    element.srcObject = stream;
    const entry: Entry = { track, element, source: null, gain: null, boost: null, level };
    const ctx = this.ctx;
    if (ctx && this.duckNode) {
      element.muted = true;
      try {
        const source = ctx.createMediaStreamSource(stream);
        const gain = ctx.createGain();
        gain.gain.value = level;
        source.connect(gain);
        entry.source = source;
        entry.gain = gain;
        this.route(entry);
      } catch {
        // This stream plays through its element instead.
        entry.source?.disconnect();
        entry.source = null;
        entry.gain = null;
      }
    }
    if (!entry.gain) this.playPlain(entry);
    void element.play().catch(() => {});
    this.entries.set(key, entry);
  }

  /** The stream's gain into the duck gain, through its own limiter while above 100 %. */
  private route(entry: Entry): void {
    const ctx = this.ctx;
    const duck = this.duckNode;
    if (!ctx || !duck || !entry.gain) return;
    entry.gain.disconnect();
    entry.boost?.disconnect();
    entry.boost = null;
    if (entry.level > 1) {
      const boost = limiterOf(ctx);
      entry.gain.connect(boost);
      boost.connect(duck);
      entry.boost = boost;
    } else {
      entry.gain.connect(duck);
    }
  }

  /** Without WebAudio: the element itself, 100 % at most, ducked by its volume. */
  private playPlain(entry: Entry): void {
    entry.element.muted = entry.level <= 0;
    entry.element.volume = clamp(entry.level * this.duck, 0, 1);
  }

  private setLevel(entry: Entry, level: number): void {
    if (Math.abs(entry.level - level) < 1e-4) return;
    const crossed = entry.level > 1 !== level > 1;
    entry.level = level;
    if (!entry.gain || !this.ctx) {
      this.playPlain(entry);
      return;
    }
    entry.gain.gain.setTargetAtTime(level, this.ctx.currentTime, GLIDE_S);
    if (crossed) this.route(entry);
  }

  private setDuck(target: number): void {
    const move = duckMove(this.duck, target);
    if (!move) return;
    this.duck = target;
    const ctx = this.ctx;
    const node = this.duckNode;
    if (ctx && node) {
      const now = ctx.currentTime;
      node.gain.cancelScheduledValues(now);
      node.gain.setValueAtTime(node.gain.value, now);
      node.gain.setTargetAtTime(move.to, now, move.tau);
    }
    for (const entry of this.entries.values()) if (!entry.gain) this.playPlain(entry);
  }

  /** The chosen output while it is there, else the default; one change at a time. */
  private applySink(): void {
    this.sinkWork = this.sinkWork.then(async () => {
      const wanted = this.sinkWanted;
      const ctx = this.ctx;
      if (ctx && wanted !== this.sinkNow) {
        const now = await sinkTo(ctx, wanted);
        if (this.ctx === ctx) this.sinkNow = now;
      }
      for (const entry of this.entries.values()) {
        if (!entry.gain) await sinkTo(entry.element, wanted);
      }
    });
  }

  /** Plays while something does, rests a moment after the last stream went. */
  private rest(): void {
    const ctx = this.ctx;
    if (this.entries.size === 0) {
      this.stopInput?.();
      this.stopInput = null;
      if (ctx && this.restTimer === null && ctx.state === "running") {
        this.restTimer = this.host.setTimeout(() => {
          this.restTimer = null;
          if (this.entries.size === 0 && this.ctx === ctx) void ctx.suspend().catch(() => {});
        }, REST_MS);
      }
      return;
    }
    if (this.restTimer !== null) {
      this.host.clearTimeout(this.restTimer);
      this.restTimer = null;
    }
    if (!ctx || ctx.state === "running" || this.waking) return;
    this.waking = true;
    void ctx
      .resume()
      .catch(() => {})
      .then(() => {
        this.waking = false;
        // A context the browser keeps suspended (the page never heard the player): the next press
        // or key wakes it. A resume, not a new context: the device is open already.
        const state = ctx.state as AudioContextState;
        if (this.ctx !== ctx || state === "running" || this.stopInput) return;
        this.stopInput = this.host.onInput(() => {
          this.stopInput = null;
          if (this.ctx === ctx && this.entries.size > 0) void ctx.resume().catch(() => {});
        });
      });
  }

  private drop(key: string, entry: Entry): void {
    this.entries.delete(key);
    entry.boost?.disconnect();
    entry.gain?.disconnect();
    entry.source?.disconnect();
    entry.element.pause();
    entry.element.srcObject = null;
  }
}
