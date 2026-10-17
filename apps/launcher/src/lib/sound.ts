import { usePrefs } from "@/stores/prefs";
import { encodeWav } from "./wav";

/**
 * Small interface sounds, synthesized with WebAudio (no audio files to ship or license).
 * They follow Settings → Görünüm (on/off and volume) and stay quiet by design.
 *
 * Each sound is drawn once by an OfflineAudioContext into a short clip that an audio element
 * plays. There is no live AudioContext on purpose: creating the first one opens the audio device
 * on the page's main thread, which froze the OYNA press for about 150 ms.
 */
export type SoundName = "play" | "success" | "error";

const SAMPLE_RATE = 44_100;
/** Clip lengths in seconds: the last tone and its fade, with a little room. */
const LENGTH: Record<SoundName, number> = { play: 0.75, success: 0.55, error: 0.35 };
/** Sounds start a hair into the clip so the first ramp is never cut. */
const START = 0.01;

const clips = new Map<SoundName, Promise<HTMLAudioElement | null>>();

function envelope(
  ctx: BaseAudioContext,
  peak: number,
  attack: number,
  release: number,
  at: number,
) {
  const gain = ctx.createGain();
  gain.gain.setValueAtTime(0, at);
  gain.gain.linearRampToValueAtTime(peak, at + attack);
  gain.gain.exponentialRampToValueAtTime(0.0001, at + attack + release);
  gain.connect(ctx.destination);
  return gain;
}

function tone(
  ctx: BaseAudioContext,
  type: OscillatorType,
  from: number,
  to: number,
  peak: number,
  at: number,
  length: number,
) {
  const osc = ctx.createOscillator();
  osc.type = type;
  osc.frequency.setValueAtTime(from, at);
  osc.frequency.exponentialRampToValueAtTime(to, at + length);
  osc.connect(envelope(ctx, peak, Math.min(0.03, length / 4), length, at));
  osc.start(at);
  osc.stop(at + length + 0.05);
}

/** An ember "whoosh": filtered noise sweeping up, under a warm fifth. */
function ignite(ctx: BaseAudioContext, at: number) {
  const length = 0.55;
  const buffer = ctx.createBuffer(1, Math.ceil(ctx.sampleRate * length), ctx.sampleRate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
  const noise = ctx.createBufferSource();
  noise.buffer = buffer;
  const filter = ctx.createBiquadFilter();
  filter.type = "bandpass";
  filter.Q.value = 0.9;
  filter.frequency.setValueAtTime(380, at);
  filter.frequency.exponentialRampToValueAtTime(2600, at + length * 0.8);
  noise.connect(filter);
  filter.connect(envelope(ctx, 0.09, 0.06, length, at));
  noise.start(at);
  noise.stop(at + length + 0.05);
  tone(ctx, "sine", 196, 294, 0.05, at + 0.04, 0.6);
  tone(ctx, "sine", 294, 440, 0.035, at + 0.08, 0.55);
}

function draw(ctx: BaseAudioContext, name: SoundName) {
  switch (name) {
    case "play":
      ignite(ctx, START);
      break;
    case "success":
      tone(ctx, "triangle", 659, 659, 0.06, START, 0.22);
      tone(ctx, "triangle", 988, 988, 0.05, START + 0.11, 0.32);
      break;
    case "error":
      tone(ctx, "sine", 150, 92, 0.09, START, 0.26);
      break;
  }
}

async function render(name: SoundName): Promise<HTMLAudioElement | null> {
  if (typeof OfflineAudioContext === "undefined" || typeof Audio === "undefined") return null;
  try {
    const ctx = new OfflineAudioContext(1, Math.ceil(SAMPLE_RATE * LENGTH[name]), SAMPLE_RATE);
    draw(ctx, name);
    const rendered = await ctx.startRendering();
    const wav = new Blob([encodeWav(rendered.getChannelData(0), SAMPLE_RATE)], {
      type: "audio/wav",
    });
    const element = new Audio(URL.createObjectURL(wav));
    element.preload = "auto";
    return element;
  } catch {
    return null;
  }
}

function clip(name: SoundName) {
  let pending = clips.get(name);
  if (!pending) {
    pending = render(name);
    clips.set(name, pending);
  }
  return pending;
}

/** Draws the clips in idle time after start-up, so the first sound costs the press nothing. */
export function prepareSounds() {
  if (typeof window === "undefined" || !usePrefs.getState().sounds) return;
  const run = () => {
    for (const name of Object.keys(LENGTH) as SoundName[]) void clip(name);
  };
  if (typeof requestIdleCallback === "function") requestIdleCallback(run, { timeout: 5000 });
  else setTimeout(run, 2000);
}

export function playSound(name: SoundName) {
  const { sounds, soundVolume } = usePrefs.getState();
  if (!sounds || soundVolume <= 0) return;
  const volume = Math.min(1, soundVolume / 100);
  void clip(name).then((element) => {
    if (!element) return;
    element.volume = volume;
    element.currentTime = 0;
    // Audio is a nicety; never let it break an action.
    element.play().catch(() => {});
  });
}
