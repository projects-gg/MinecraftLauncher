// The live meters on the page (VOICE-CALL): how loud each person speaks and the player's own
// microphone, from `voice-levels`. They come only while the page asks: in a room, the window on
// screen and in front (a focused game or another app takes them away), motion allowed (reduced
// motion and Performance mode keep the plain speaking ring). Nothing renders for them: each meter
// and face hands in a painter that writes its own transform and opacity (short CSS transitions
// smooth the steps); when the stream stops every painter gets 0 once. No timer, no frame loop.
import { useMotionPrefs } from "@pl/ui";
import { useEffect, useRef, useSyncExternalStore } from "react";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceLevels } from "@/lib/api/voice-types";
import { isTauri } from "@/lib/platform";
import { useWindowAway } from "@/lib/window-away";
import { useVoiceUi, voiceOn } from "./store";

/** A painter for one source's level (0–1, the settings bar's scale) and whether it goes out. */
export type Painter = (level: number, open: boolean) => void;

/** The player's own microphone among the sources (people are their ids). */
export const MIC = "\u0000mic";

interface Reading {
  level: number;
  open: boolean;
}

const REST: Reading = { level: 0, open: false };
const painters = new Map<string, Set<Painter>>();
let shown = new Map<string, Reading>();

/** One frame from the launcher: every painter whose source moved paints. */
export function paintLevels(frame: VoiceLevels): void {
  const next = new Map<string, Reading>();
  if (frame.me > 0 || frame.open) next.set(MIC, { level: frame.me, open: frame.open });
  for (const person of frame.people) {
    if (person.level > 0) next.set(person.id, { level: person.level, open: true });
  }
  const before = shown;
  shown = next;
  for (const [id, set] of painters) {
    const now = next.get(id) ?? REST;
    const was = before.get(id) ?? REST;
    if (now.level === was.level && now.open === was.open) continue;
    for (const paint of set) paint(now.level, now.open);
  }
}

/** The stream stopped: everything rests (once). */
export function restLevels(): void {
  if (shown.size) paintLevels({ me: 0, open: false, people: [] });
}

/** What a source shows now (tests, and a painter that joins late). */
export function levelNow(id: string): Reading {
  return shown.get(id) ?? REST;
}

/**
 * Paints `id`'s level with `paint` while mounted (the latest `paint` is used; it gets the level
 * now at once; null: nothing). Never renders the component again.
 */
export function usePainter(id: string | null, paint: Painter): void {
  const latest = useRef(paint);
  latest.current = paint;
  useEffect(() => {
    if (id === null) return;
    const painter: Painter = (level, open) => latest.current(level, open);
    let set = painters.get(id);
    if (!set) {
      set = new Set();
      painters.set(id, set);
    }
    set.add(painter);
    const now = levelNow(id);
    painter(now.level, now.open);
    return () => {
      set.delete(painter);
      if (!set.size) painters.delete(id);
      painter(0, false);
    };
  }, [id]);
}

/** Whether the launcher's window has the keyboard focus (another app or the game does not). */
function useWindowFocused(): boolean {
  return useSyncExternalStore(
    (listener) => {
      window.addEventListener("focus", listener);
      window.addEventListener("blur", listener);
      return () => {
        window.removeEventListener("focus", listener);
        window.removeEventListener("blur", listener);
      };
    },
    () => document.hasFocus(),
  );
}

/** Whether the live meters run now (a room, the window in front, motion allowed). */
export function useLevelsWanted(): boolean {
  const inRoom = useVoiceUi((s) => voiceOn(s.state) && s.state.room !== null);
  const away = useWindowAway();
  // The browser preview's headless captures have no focus; the app's window does.
  const focused = useWindowFocused() || !isTauri();
  const { reduce } = useMotionPrefs();
  return inRoom && !away && focused && !reduce;
}

/** Asks for the live meters while they may run, and rests them when they stop (VoiceHost). */
export function useLevelStream(): void {
  const wanted = useLevelsWanted();
  useEffect(() => {
    if (!wanted) return;
    const stop = voiceApi.onLevels(paintLevels);
    void voiceApi.setLevels(true).catch(() => {
      // An older launcher without meters: the speaking ring stays as it is.
    });
    return () => {
      stop();
      void voiceApi.setLevels(false).catch(() => {});
      restLevels();
    };
  }, [wanted]);
}
