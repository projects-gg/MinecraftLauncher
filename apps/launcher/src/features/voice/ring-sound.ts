// Voice chat's ring on the page (VOICE-L3): while a call or invite rings for the player and the
// launcher is in front (VOICE-L1's rule: behind, Windows' call notification rings with its own
// sound). Our own synthesized chime (no file to ship or license), drawn once in an
// OfflineAudioContext into a WAV that an audio element loops. Nothing plays while nothing rings,
// and nothing with the window away. The caller's waiting tone is the voice window's (VOICE-CALL:
// one tone, heard in the tray and the game too; the page's own ringback doubled it).
import { encodeWav } from "@/lib/wav";

export type VoiceTone = "ring";

const SAMPLE_RATE = 44_100;
/** One loop of the ring, its rest included (seconds). */
const LENGTH: Record<VoiceTone, number> = { ring: 2.6 };
/** A ring is meant to be heard. */
const VOLUME: Record<VoiceTone, number> = { ring: 0.6 };

/** A soft bell: a sine and its quiet octave, a quick attack, a long fall. */
function chime(ctx: BaseAudioContext, frequency: number, at: number, peak: number, length: number) {
  const partials: Array<[number, number]> = [
    [frequency, peak],
    [frequency * 2, peak * 0.25],
  ];
  for (const [f, p] of partials) {
    const osc = ctx.createOscillator();
    osc.type = "sine";
    osc.frequency.setValueAtTime(f, at);
    const gain = ctx.createGain();
    gain.gain.setValueAtTime(0, at);
    gain.gain.linearRampToValueAtTime(p, at + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.0001, at + length);
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.start(at);
    osc.stop(at + length + 0.05);
  }
}

function draw(ctx: BaseAudioContext) {
  // Two rising pairs ("ding-dong, ding-dong"), then a rest.
  chime(ctx, 659.25, 0.02, 0.22, 0.5);
  chime(ctx, 880, 0.22, 0.2, 0.6);
  chime(ctx, 659.25, 0.72, 0.22, 0.5);
  chime(ctx, 880, 0.92, 0.2, 0.7);
}

const clips = new Map<VoiceTone, Promise<HTMLAudioElement | null>>();

async function render(tone: VoiceTone): Promise<HTMLAudioElement | null> {
  if (typeof OfflineAudioContext === "undefined" || typeof Audio === "undefined") return null;
  try {
    const ctx = new OfflineAudioContext(1, Math.ceil(SAMPLE_RATE * LENGTH[tone]), SAMPLE_RATE);
    draw(ctx);
    const rendered = await ctx.startRendering();
    const wav = new Blob([encodeWav(rendered.getChannelData(0), SAMPLE_RATE)], {
      type: "audio/wav",
    });
    const element = new Audio(URL.createObjectURL(wav));
    element.loop = true;
    element.preload = "auto";
    return element;
  } catch {
    return null;
  }
}

function clip(tone: VoiceTone) {
  let pending = clips.get(tone);
  if (!pending) {
    pending = render(tone);
    clips.set(tone, pending);
  }
  return pending;
}

let wanted: VoiceTone | null = null;
let playing: { tone: VoiceTone; element: HTMLAudioElement } | null = null;

/** The tone that should sound now (null: silence). Calling it again with the same tone does nothing. */
export function playTone(tone: VoiceTone | null) {
  wanted = tone;
  if (playing && playing.tone !== tone) {
    playing.element.pause();
    playing = null;
  }
  if (!tone || playing) return;
  void clip(tone).then((element) => {
    // Drawn meanwhile: only if it is still wanted and nothing else started.
    if (!element || wanted !== tone || playing) return;
    element.volume = VOLUME[tone];
    element.currentTime = 0;
    playing = { tone, element };
    // Sound is a nicety; a refused play (autoplay rules) leaves the card to say it.
    element.play().catch(() => {
      if (playing?.element === element) playing = null;
    });
  });
}
