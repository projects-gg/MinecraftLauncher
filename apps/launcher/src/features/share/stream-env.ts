// What watching depends on besides the streams (SQOL-U1): the output device, "Herkesin Sesi",
// deafness, the volume's cap and the watching choices ("Yayın Kalitesi", "Biri Konuşunca Yayın
// Sesini Kıs"). The launcher's window reads them from voice chat's settings and state; the "Yayın"
// window has neither and asks the launcher (`share_popout_audio`, then `share-audio` on every
// change). Both play their streams through `StreamAudio` (share/stream-audio.ts), started while the
// room shares and stopped with it.
import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import { keptVoicePrefs, useVoicePrefs } from "@/features/voice/prefs";
import { useVoiceUi } from "@/features/voice/store";
import { listen } from "@/lib/api/listen";
import type { VoiceSettings, VoiceState } from "@/lib/api/voice-types";
import { isTauri } from "@/lib/platform";
import { shareApi } from "@/share/api";
import {
  DEFAULT_STREAM_SOUND,
  duckGain,
  StreamAudio,
  type StreamSound,
  sinkFor,
  streamGain,
  type WantedStream,
} from "@/share/stream-audio";
import {
  DEFAULT_SHARE_SETTINGS,
  type ShareSurface,
  type ShareViewQuality,
  shareKey,
} from "@/share/types";
import { pageEngine, subscribeStreams } from "./store";

/** The launcher's answer to the "Yayın" window (`share_popout_audio`, `share-audio`). */
export interface PopoutAudio {
  device: string | null;
  /** "Herkesin Sesi", percent 0–200. */
  volume: number;
  deaf: boolean;
  volumeMax: number;
  duck: boolean;
  duckLevel: number;
  duckSelf: boolean;
  viewQuality: ShareViewQuality;
}

/** A watching choice changed from a menu (`share_view_prefs`; the launcher keeps it). */
export interface ShareViewPrefs {
  viewQuality?: ShareViewQuality;
  duck?: boolean;
  duckLevel?: number;
  duckSelf?: boolean;
}

export interface StreamEnv {
  /** "Ses çıkışı" (null: the system's default). */
  device: string | null;
  /** The outputs the voice engine knows (null: not known; the chosen one is tried). */
  outputs: readonly string[] | null;
  /** "Herkesin Sesi", percent 0–200. */
  everyone: number;
  deaf: boolean;
  /** "Yayın Sesi"'s top, percent (≤ 200). */
  volumeMax: number;
  duck: boolean;
  duckLevel: number;
  duckSelf: boolean;
  viewQuality: ShareViewQuality;
}

export const DEFAULT_STREAM_ENV: StreamEnv = {
  device: null,
  outputs: null,
  everyone: 100,
  deaf: false,
  volumeMax: 200,
  duck: DEFAULT_SHARE_SETTINGS.duck,
  duckLevel: DEFAULT_SHARE_SETTINGS.duckLevel,
  duckSelf: DEFAULT_SHARE_SETTINGS.duckSelf,
  viewQuality: DEFAULT_SHARE_SETTINGS.viewQuality,
};

export const useStreamEnv = create<StreamEnv>()(() => DEFAULT_STREAM_ENV);

function sameEnv(a: StreamEnv, b: StreamEnv): boolean {
  return (
    a.device === b.device &&
    a.everyone === b.everyone &&
    a.deaf === b.deaf &&
    a.volumeMax === b.volumeMax &&
    a.duck === b.duck &&
    a.duckLevel === b.duckLevel &&
    a.duckSelf === b.duckSelf &&
    a.viewQuality === b.viewQuality &&
    (a.outputs ?? []).join("\n") === (b.outputs ?? []).join("\n") &&
    (a.outputs === null) === (b.outputs === null)
  );
}

function setEnv(next: StreamEnv) {
  if (!sameEnv(useStreamEnv.getState(), next)) useStreamEnv.setState(next, true);
}

/** The launcher's window: from voice chat's settings and state. */
export function mainEnv(settings: VoiceSettings | null, state: VoiceState): StreamEnv {
  const share = { ...DEFAULT_SHARE_SETTINGS, ...settings?.share };
  const outputs = state.devices.outputs.map((d) => d.id);
  return {
    device: settings?.outputDevice ?? null,
    outputs: outputs.length ? outputs : null,
    everyone: settings?.outputVolume ?? 100,
    deaf: state.room?.me.deaf ?? false,
    volumeMax: Math.min(200, state.volumeMax ?? 200),
    duck: share.duck,
    duckLevel: share.duckLevel,
    duckSelf: share.duckSelf,
    viewQuality: share.viewQuality,
  };
}

/** The "Yayın" window: from the launcher's answer. */
export function popoutEnv(audio: PopoutAudio): StreamEnv {
  return {
    device: audio.device,
    outputs: null,
    everyone: audio.volume,
    deaf: audio.deaf,
    volumeMax: Math.min(200, audio.volumeMax),
    duck: audio.duck,
    duckLevel: audio.duckLevel,
    duckSelf: audio.duckSelf,
    viewQuality: audio.viewQuality,
  };
}

/** The launcher's window follows voice chat's settings and state. Returns a stop function. */
export function followMainEnv(): () => void {
  const apply = () => setEnv(mainEnv(useVoicePrefs.getState().settings, useVoiceUi.getState().state));
  apply();
  const stopPrefs = useVoicePrefs.subscribe(apply);
  const stopState = useVoiceUi.subscribe((now, before) => {
    if (now.state !== before.state) apply();
  });
  return () => {
    stopPrefs();
    stopState();
  };
}

/** The "Yayın" window asks the launcher once, then hears each change. Returns a stop function. */
export function followPopoutEnv(): () => void {
  if (!isTauri()) return () => {};
  let heard = false;
  let gone = false;
  let stop: (() => void) | null = null;
  void listen<PopoutAudio>("share-audio", (event) => {
    heard = true;
    setEnv(popoutEnv(event.payload));
  })
    .then((unlisten) => {
      if (gone) unlisten();
      else stop = unlisten;
    })
    .catch(() => {
      // An older launcher sends no changes: the first answer stands.
    });
  void invoke<PopoutAudio>("share_popout_audio")
    .then((audio) => {
      if (!heard && !gone) setEnv(popoutEnv(audio));
    })
    .catch(() => {
      // An older launcher: the defaults (the default output, 100 %).
    });
  return () => {
    gone = true;
    stop?.();
  };
}

/**
 * A watching choice from a menu or a key (both windows): shown at once, kept by the launcher, which
 * tells the other window. The settings page saves through its own copy instead (settings.tsx).
 */
export function setViewPrefs(change: ShareViewPrefs): void {
  useStreamEnv.setState(change);
  const settings = useVoicePrefs.getState().settings;
  if (settings) {
    keptVoicePrefs({ ...settings, share: { ...DEFAULT_SHARE_SETTINGS, ...settings.share, ...change } });
  }
  if (isTauri()) {
    void invoke("share_view_prefs", { change }).catch(() => {
      // The next change tries again; the page shows what it holds.
    });
  }
}

// --- "Yayın Sesi" while it moves ---------------------------------------------------------------

/** A volume the player is moving now (a drag, the arrows): heard at once, saved when it settles. */
export const useStreamLive = create<{ volumes: Record<string, number> }>()(() => ({ volumes: {} }));

/** How long after the last arrow press the volume is saved. */
const KEY_SAVE_MS = 450;
/** A moved volume shows over the saved one until the launcher's state says it (or this long). */
const LIVE_HOLD_MS = 2500;

const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();
const holdTimers = new Map<string, ReturnType<typeof setTimeout>>();

function dropLive(person: string) {
  const volumes = { ...useStreamLive.getState().volumes };
  if (!(person in volumes)) return;
  delete volumes[person];
  useStreamLive.setState({ volumes });
}

/** A person's "Yayın Sesi" as it plays now (a moving value first). */
export function streamSoundOf(person: string): StreamSound {
  const saved = useVoiceUi.getState().state.share?.volumes[person] ?? DEFAULT_STREAM_SOUND;
  const live = useStreamLive.getState().volumes[person];
  return live === undefined ? saved : { ...saved, volume: live };
}

/**
 * "Yayın Sesi" moved: heard at once; `final` (the end of a drag) saves it now, otherwise it is saved
 * once the moves settle (the arrows). Kept per person by the launcher.
 */
export function moveStreamVolume(person: string, volume: number, final: boolean): void {
  const max = useStreamEnv.getState().volumeMax;
  const value = Math.round(Math.min(max, Math.max(0, volume)));
  useStreamLive.setState({ volumes: { ...useStreamLive.getState().volumes, [person]: value } });
  clearTimeout(saveTimers.get(person));
  const save = () => {
    saveTimers.delete(person);
    const muted = useVoiceUi.getState().state.share?.volumes[person]?.muted ?? false;
    void shareApi.streamVolume(person, value, muted).catch(() => {});
    clearTimeout(holdTimers.get(person));
    holdTimers.set(
      person,
      setTimeout(() => {
        holdTimers.delete(person);
        if (useStreamLive.getState().volumes[person] === value) dropLive(person);
      }, LIVE_HOLD_MS),
    );
  };
  if (final) save();
  else saveTimers.set(person, setTimeout(save, KEY_SAVE_MS));
}

/** "Yayını Sessize Al" (M on the tile). */
export function toggleStreamMute(person: string): void {
  const sound = streamSoundOf(person);
  void shareApi.streamVolume(person, Math.round(sound.volume), !sound.muted).catch(() => {});
}

/** The saved volume caught up with the moving one: it shows by itself again. */
function settleLive(state: VoiceState) {
  const live = useStreamLive.getState().volumes;
  for (const [person, volume] of Object.entries(live)) {
    if (saveTimers.has(person)) continue;
    if (state.share?.volumes[person]?.volume === volume) dropLive(person);
  }
}

// --- playing -----------------------------------------------------------------------------------

/** The sound closes this long after nothing in the room needs it any more… */
const CLOSE_AFTER_MS = 60_000;
/** …and opens in idle time before a stream comes (at most this late). */
const IDLE_WAIT_MS = 2000;

function whenIdle(work: () => void): () => void {
  if (typeof window.requestIdleCallback === "function") {
    const id = window.requestIdleCallback(work, { timeout: IDLE_WAIT_MS });
    return () => window.cancelIdleCallback(id);
  }
  const id = window.setTimeout(work, 200);
  return () => window.clearTimeout(id);
}

/**
 * Plays the streams this window watches (`surface`), while it lives: their gains, the ducking, the
 * output. The sound opens in idle time once someone else in the room shares or a watch begins, and
 * closes a minute after neither is true. Returns a stop function.
 */
export function startStreamPlayback(surface: ShareSurface): () => void {
  const audio = new StreamAudio();
  let queued = false;
  let cancelIdle: (() => void) | null = null;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  let needed = false;

  const run = () => {
    queued = false;
    const ui = useVoiceUi.getState();
    const share = ui.state.share?.on ? ui.state.share : null;
    const me = ui.state.room?.me.id ?? null;
    const here = share !== null && share.surface === surface;
    const env = useStreamEnv.getState();

    // The sound is ready before a stream comes; it lets go a minute after nothing needs it.
    const need =
      here && (share.watching.length > 0 || share.live.some((live) => live.person !== me));
    if (need && !needed) {
      clearTimeout(closeTimer);
      closeTimer = undefined;
      cancelIdle?.();
      cancelIdle = whenIdle(() => {
        cancelIdle = null;
        audio.prepare();
      });
    } else if (!need && needed) {
      cancelIdle?.();
      cancelIdle = null;
      closeTimer = setTimeout(() => audio.close(), CLOSE_AFTER_MS);
    }
    needed = need;

    const wanted: WantedStream[] = [];
    const engine = pageEngine();
    if (here && engine) {
      const levelEnv = {
        everyone: env.everyone,
        deaf: env.deaf,
        volumeMax: audio.boosted ? env.volumeMax : Math.min(100, env.volumeMax),
      };
      for (const watch of share.watching) {
        if (watch.person === me) continue;
        const stream = engine.stream(watch.person, watch.n);
        if (!stream) continue;
        const level = streamGain(streamSoundOf(watch.person), levelEnv);
        wanted.push({ key: shareKey(watch.person, watch.n), stream, level });
      }
    }
    audio.sync(wanted, duckGain(env, ui.speaking, ui.meTalking, me), sinkFor(env.device, env.outputs));
  };

  const schedule = () => {
    if (queued) return;
    queued = true;
    queueMicrotask(run);
  };

  const stops = [
    subscribeStreams(schedule),
    useVoiceUi.subscribe((now, before) => {
      if (now.state !== before.state) settleLive(now.state);
      if (
        now.state !== before.state ||
        now.speaking !== before.speaking ||
        now.meTalking !== before.meTalking
      ) {
        schedule();
      }
    }),
    useStreamEnv.subscribe(schedule),
    useStreamLive.subscribe(schedule),
  ];
  schedule();
  return () => {
    for (const stop of stops) stop();
    cancelIdle?.();
    clearTimeout(closeTimer);
    audio.dispose();
  };
}
