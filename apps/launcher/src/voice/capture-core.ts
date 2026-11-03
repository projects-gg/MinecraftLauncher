// The microphone's way to the call (design §6.2), as the capture worklet runs it: the player's gain,
// then 10 ms frames: the voice gate decides on each and RNNoise cleans the frames that may be
// speech or are sent anyway (silence costs nothing), then two outputs: what is sent (the gate, mute
// and push-to-talk) and what the player hears of themselves in "Mikrofonu dene". Every close fades
// over 5 ms (no click). Free of worklet globals: tests drive it. Nothing here allocates per quantum.
import { dbOf, levelOf, VoiceGate } from "./dsp";
import { type Denoiser, RNNOISE_FRAME } from "./rnnoise";

export interface CaptureConfig {
  /** The microphone's gain (1 while the automatic gain control is on). */
  gain: number;
  /** Noise suppression asked for: RNNoise cleans speech when it is loaded. */
  ns: boolean;
  /** null: automatic; else the threshold 0–100. */
  threshold: number | null;
  /** Push-to-talk: `talk` alone opens the way out. */
  ptt: boolean;
}

export interface CaptureEvents {
  /** The microphone sends speech now, or stopped (only on change). */
  me(on: boolean): void;
  /** The settings page's bar: the loudest of the last 100 ms (0–1), and whether it is speech. */
  level(level: number, open: boolean): void;
  /**
   * The launcher's live meter (VOICE-CALL): the loudest of the last 60 ms (0–1, in 64 steps) and
   * whether any of it went out; only on change, and a quiet microphone posts nothing.
   */
  meter(level: number, open: boolean): void;
  /** RNNoise takes more of the audio thread than it can give (once). */
  slow(): void;
}

/** One render quantum. */
export const BLOCK = 128;
const FRAME = RNNOISE_FRAME;
/** Four frames, fifteen quanta. */
const RING = 1920;
/** Input to output: every frame is whole and cleaned before its first sample leaves (13.3 ms). */
const DELAY = 640;
/** RNNoise hears 16-bit samples. */
const SCALE = 32768;
/** Push-to-talk released: the end of the word still goes (200 ms). */
const PTT_TAIL_FRAMES = 20;
/** The level bar's pace: ten a second. */
const LEVEL_FRAMES = 10;
/** The live meter's pace (VOICE-CALL): about sixteen a second. */
const METER_FRAMES = 6;
/** The live meter's steps (a meter of a few bars needs no finer value; fewer posts change). */
const METER_STEPS = 64;
/** Below this (-48 dBFS on the bar's scale) the meter reads 0: a room's quiet lights nothing. */
const METER_QUIET = 0.2;
/** RNNoise's cost is judged over this many cleaned frames (1 s of speech)... */
const SLOW_FRAMES = 100;
/** ...and is too much above this share of their length, twice in a row. */
const SLOW_SHARE = 0.5;
const SLOW_RUNS = 2;

export class CaptureCore {
  private readonly ring = new Float32Array(RING);
  /** The ring's four frames (views made once). */
  private readonly frames: Float32Array[] = [];
  /** RNNoise's warm-up frame at the start of a run. */
  private readonly prime = new Float32Array(FRAME);
  /** The gate's word for each frame of the ring: send it, and play it back (self-test). */
  private readonly sendFrames = new Uint8Array(RING / FRAME);
  private readonly monitorFrames = new Uint8Array(RING / FRAME);
  private readonly gate: VoiceGate;
  private readonly frameMs: number;
  /** A 5 ms fade, per sample. */
  private readonly step: number;
  private write = 0;
  private pos = RING - 2 * FRAME;
  private sendGain = 0;
  private monitorGain = 0;
  private idle = true;
  private config: CaptureConfig = { gain: 1, ns: true, threshold: null, ptt: false };
  private talkOpen = false;
  private tail = 0;
  private testing = false;
  private levels = false;
  /** The launcher's live meter (VOICE-CALL). */
  private metering = false;
  private meterPeak = Number.NEGATIVE_INFINITY;
  private meterOpen = false;
  private meterFrames = 0;
  /** The meter's last post (a step, and whether it went out), to post only changes. */
  private meterSent = 0;
  private meterSentOpen = false;
  private denoiser: Denoiser | null = null;
  /** RNNoise cleaned the previous frame (its state follows the sound). */
  private denoisedLast = false;
  private meOn = false;
  private levelPeak = Number.NEGATIVE_INFINITY;
  private levelOpen = false;
  private levelFrames = 0;
  private cost = 0;
  private costFrames = 0;
  private slowRuns = 0;
  private slowSaid = false;

  constructor(
    rate: number,
    private readonly events: CaptureEvents,
    /** Milliseconds, for RNNoise's cost. */
    private readonly clock: () => number,
  ) {
    this.gate = new VoiceGate(FRAME / rate);
    this.frameMs = (FRAME / rate) * 1000;
    this.step = 1 / (0.005 * rate);
    for (let i = 0; i < RING / FRAME; i++) {
      this.frames.push(this.ring.subarray(i * FRAME, (i + 1) * FRAME));
    }
  }

  configure(config: CaptureConfig): void {
    if (!config.ptt) this.tail = 0;
    this.config = config;
    this.gate.threshold = config.threshold;
  }

  /**
   * Mute, deafen, a moderator's mute and push-to-talk, as the launcher decides them. A close fades
   * at once, but push-to-talk's release (`tail`) keeps the end of the word 200 ms more.
   */
  talk(open: boolean, tail = true): void {
    if (open) {
      this.tail = 0;
    } else if (this.config.ptt && this.talkOpen && tail) {
      this.tail = PTT_TAIL_FRAMES;
    } else {
      this.tail = 0;
      // What is still on its way fades now.
      this.sendFrames.fill(0);
    }
    this.talkOpen = open;
  }

  /** The settings page's level bar. */
  setLevels(on: boolean): void {
    this.levels = on;
    this.levelFrames = 0;
    this.levelPeak = Number.NEGATIVE_INFINITY;
    this.levelOpen = false;
  }

  /**
   * The launcher's live meter (VOICE-CALL): while on, the microphone is listened to even when
   * nothing may go out (alone in the room, push-to-talk up), so the player sees it hears them.
   */
  setMeter(on: boolean): void {
    if (on === this.metering) return;
    this.metering = on;
    this.meterFrames = 0;
    this.meterPeak = Number.NEGATIVE_INFINITY;
    this.meterOpen = false;
    // Off: the meter rests (the engine says so itself); on: the first post is a change.
    this.meterSent = 0;
    this.meterSentOpen = false;
  }

  /** "Mikrofonu dene": the player hears what would be sent; nothing goes out meanwhile. */
  setTest(on: boolean): void {
    this.testing = on;
    if (on) this.sendFrames.fill(0);
    else this.monitorFrames.fill(0);
  }

  /** RNNoise once loaded; null: none (the built-in suppressor took over, or it failed). */
  setDenoiser(denoiser: Denoiser | null): void {
    this.denoiser = denoiser;
    this.denoisedLast = false;
  }

  /** Another microphone: the gate learns its room anew. */
  newMicrophone(): void {
    this.gate.reset();
  }

  /** Whether anything needs the microphone now (or a fade is still under way). */
  private active(): boolean {
    return (
      this.talkOpen ||
      this.tail > 0 ||
      this.testing ||
      this.levels ||
      this.metering ||
      this.sendGain > 0 ||
      this.monitorGain > 0
    );
  }

  /** One quantum: the microphone in (none: silence), what is sent and what plays back out. */
  process(input: Float32Array | undefined, send: Float32Array, monitor: Float32Array | null): void {
    if (!this.active() || send.length !== BLOCK) {
      send.fill(0);
      monitor?.fill(0);
      this.sleep();
      return;
    }
    if (this.idle) this.wake();
    const gain = this.config.gain;
    const ring = this.ring;
    for (let i = 0; i < BLOCK; i++) {
      const v = (input?.[i] ?? 0) * gain;
      ring[this.write + i] = v > 1 ? 1 : v < -1 ? -1 : v;
    }
    this.write = (this.write + BLOCK) % RING;
    // The first quantum boundaries after each whole frame (480, 960, 1440, 1920).
    if (this.write === 128 || this.write === 512 || this.write === 1024 || this.write === 1536) {
      this.pos = (this.pos + FRAME) % RING;
      this.frame(this.pos);
    }
    const start = (this.write + RING - DELAY) % RING;
    const step = this.step;
    for (let i = 0; i < BLOCK; i++) {
      const at = start + i;
      const slot = (at / FRAME) | 0;
      const sendTo = this.sendFrames[slot] ?? 0;
      const monitorTo = this.monitorFrames[slot] ?? 0;
      this.sendGain += Math.max(-step, Math.min(step, sendTo - this.sendGain));
      this.monitorGain += Math.max(-step, Math.min(step, monitorTo - this.monitorGain));
      const v = ring[at] ?? 0;
      send[i] = v * this.sendGain;
      if (monitor) monitor[i] = v * this.monitorGain;
    }
  }

  private frame(at: number): void {
    const slot = at / FRAME;
    const frame = this.frames[slot];
    if (!frame) return;
    let energy = 0;
    for (let i = 0; i < FRAME; i++) {
      const v = frame[i] ?? 0;
      energy += v * v;
    }
    const db = dbOf(Math.sqrt(energy / FRAME));
    // Push-to-talk sends every frame while the key is down: all of them are cleaned.
    const sendable = this.config.ptt && (this.talkOpen || this.tail > 0) && !this.testing;
    let vad: number | null = null;
    const denoiser = this.config.ns ? this.denoiser : null;
    // Only the live meter listens (nothing may go out, no test, no settings bar): the level is
    // all it needs, the noise suppressor sleeps (VOICE-CALL).
    const heard = this.talkOpen || this.tail > 0 || this.testing || this.levels;
    if (denoiser && heard && (sendable || this.gate.wantsDenoise(db))) {
      const begin = this.clock();
      if (!this.denoisedLast) {
        // RNNoise answers each frame with the one before it: a run starts from the frame that
        // really came before (its output is dropped), never from a frame of the last run.
        const previous = this.frames[(slot + 3) % 4];
        if (previous) {
          for (let i = 0; i < FRAME; i++) this.prime[i] = (previous[i] ?? 0) * SCALE;
          denoiser.process(this.prime);
        }
      }
      for (let i = 0; i < FRAME; i++) frame[i] = (frame[i] ?? 0) * SCALE;
      vad = denoiser.process(frame);
      for (let i = 0; i < FRAME; i++) frame[i] = (frame[i] ?? 0) / SCALE;
      this.measure(this.clock() - begin);
      this.denoisedLast = true;
    } else {
      this.denoisedLast = false;
    }
    const voice = this.gate.push(db, vad);
    let send = false;
    if (!this.testing) {
      send = this.config.ptt ? this.talkOpen || this.tail > 0 : this.talkOpen && voice;
    }
    if (this.tail > 0 && !this.talkOpen) this.tail -= 1;
    this.sendFrames[slot] = send ? 1 : 0;
    this.monitorFrames[slot] = this.testing && voice ? 1 : 0;
    this.setMe(send && voice);
    if (this.metering) this.meterFrame(db, send);
    if (this.levels) {
      this.levelPeak = Math.max(this.levelPeak, db);
      this.levelOpen ||= voice;
      this.levelFrames += 1;
      if (this.levelFrames >= LEVEL_FRAMES) {
        this.events.level(levelOf(this.levelPeak), this.levelOpen);
        this.levelFrames = 0;
        this.levelPeak = Number.NEGATIVE_INFINITY;
        this.levelOpen = false;
      }
    }
  }

  /**
   * One frame for the live meter: every 60 ms its loudest, posted when it moved by more than a
   * step or its "out" changed. Under the room's quiet (-48 dBFS) it reads 0, so a still microphone
   * posts nothing at all.
   */
  private meterFrame(db: number, sent: boolean): void {
    this.meterPeak = Math.max(this.meterPeak, db);
    this.meterOpen ||= sent;
    this.meterFrames += 1;
    if (this.meterFrames < METER_FRAMES) return;
    const level = levelOf(this.meterPeak);
    const step = level < METER_QUIET ? 0 : Math.round(level * METER_STEPS);
    const open = this.meterOpen && step > 0;
    this.meterFrames = 0;
    this.meterPeak = Number.NEGATIVE_INFINITY;
    this.meterOpen = false;
    const moved = Math.abs(step - this.meterSent) > 1 || (step === 0) !== (this.meterSent === 0);
    if (!moved && open === this.meterSentOpen) return;
    this.meterSent = step;
    this.meterSentOpen = open;
    this.events.meter(step / METER_STEPS, open);
  }

  private measure(ms: number): void {
    if (this.slowSaid) return;
    this.cost += Math.max(0, ms);
    this.costFrames += 1;
    if (this.costFrames < SLOW_FRAMES) return;
    const share = this.cost / (this.costFrames * this.frameMs);
    this.slowRuns = share > SLOW_SHARE ? this.slowRuns + 1 : 0;
    this.cost = 0;
    this.costFrames = 0;
    if (this.slowRuns >= SLOW_RUNS) {
      this.slowSaid = true;
      this.events.slow();
    }
  }

  private setMe(on: boolean): void {
    if (on === this.meOn) return;
    this.meOn = on;
    this.events.me(on);
  }

  /** Nothing to do: a wake starts from silence, shut (the room's noise floor is kept). */
  private sleep(): void {
    this.setMe(false);
    this.idle = true;
  }

  private wake(): void {
    this.idle = false;
    this.ring.fill(0);
    this.sendFrames.fill(0);
    this.monitorFrames.fill(0);
    this.sendGain = 0;
    this.monitorGain = 0;
    this.denoisedLast = false;
    this.gate.close();
  }
}
