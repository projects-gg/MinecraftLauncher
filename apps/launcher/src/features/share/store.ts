// What screen sharing's interface keeps on the page (SHARE): whether the picker is open, whether
// the stage is small in the corner, and the watch engine of this page (its streams come from it).
// The state itself is voice chat's (`useVoiceUi.state.share`).
import { useSyncExternalStore } from "react";
import { create } from "zustand";
import { useVoiceUi } from "@/features/voice/store";
import { param } from "@/share/param";
import { HIDDEN_SHARE_VIEW, type VoiceShareView } from "@/share/types";
import type { ShareQuality, WatchEngine } from "@/share/watch";

interface ShareUi {
  /** The picker ("Ekranını paylaş"). */
  picker: boolean;
  openPicker: () => void;
  closePicker: () => void;
  /** The stage is a small picture in the corner ("Küçült"). */
  small: boolean;
  setSmall: (small: boolean) => void;
  /** The stage fills the window. */
  full: boolean;
  setFull: (full: boolean) => void;
}

export const useShareUi = create<ShareUi>()((set) => ({
  picker: param("picker") === "1",
  openPicker: () => set({ picker: true }),
  closePicker: () => set({ picker: false }),
  small: param("stage") === "small",
  setSmall: (small) => set({ small }),
  full: false,
  setFull: (full) => set({ full }),
}));

/** Screen sharing in the room now (nothing of it while it is off). */
export function useShare(): VoiceShareView {
  return useVoiceUi((s) => (s.state.share?.on ? s.state.share : HIDDEN_SHARE_VIEW));
}

// --- this page's watch engine ------------------------------------------------------------------

let engine: WatchEngine | null = null;
const engineListeners = new Set<() => void>();

/** The page's engine came or went (the stage subscribes to it). */
export function setPageEngine(next: WatchEngine | null) {
  engine = next;
  for (const listener of engineListeners) listener();
}

export function pageEngine(): WatchEngine | null {
  return engine;
}

const noStream = () => null;

function subscribeAll(listener: () => void): () => void {
  engineListeners.add(listener);
  const stop = engine?.subscribe(listener);
  return () => {
    engineListeners.delete(listener);
    stop?.();
  };
}

/** A watched share's picture and sound on this page, when it came. */
export function useShareStream(person: string, n: number): MediaStream | null {
  return useSyncExternalStore(subscribeAll, () => engine?.stream(person, n) ?? null, noStream);
}

/** A watched share's quality on this page. */
export function useShareQuality(person: string, n: number): ShareQuality {
  return useSyncExternalStore(
    subscribeAll,
    () => engine?.quality(person, n) ?? "good",
    () => "good",
  );
}
