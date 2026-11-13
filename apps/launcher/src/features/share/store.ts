// What screen sharing's interface keeps on the page (SHARE, SQOL-U1): whether the picker is open,
// how the stage shows (small in the corner, full screen, which share is forward, "Yayın Bilgileri",
// the player's own share as a tile), and the watch engine of this page (its streams come from it).
// The state itself is voice chat's (`useVoiceUi.state.share`).
import { useEffect, useSyncExternalStore } from "react";
import { create } from "zustand";
import { useFace, useVoiceUi } from "@/features/voice/store";
import { isTauri } from "@/lib/platform";
import { mockInfo, mockQuality } from "@/share/mock-view";
import { param } from "@/share/param";
import { HIDDEN_SHARE_VIEW, type ShareBy, type VoiceShareView } from "@/share/types";
import type { ShareInfo, ShareQuality, WatchEngine } from "@/share/watch";

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
  /** SQOL: the share brought forward ("Bu Yayını Öne Al"; its `shareKey`); null: side by side. */
  forward: string | null;
  setForward: (key: string | null) => void;
  /** The forward share before a tile's "Tam Ekran" brought its own forward (back on leaving). */
  forwardBefore: string | null | undefined;
  /** SQOL: "Yayın Bilgileri" on every tile. */
  info: boolean;
  setInfo: (on: boolean) => void;
  /** Menus of the stage open now: its controls never hide meanwhile. */
  menus: number;
  menuOpen: (open: boolean) => void;
}

export const useShareUi = create<ShareUi>()((set) => ({
  picker: param("picker") === "1",
  openPicker: () => set({ picker: true }),
  closePicker: () => set({ picker: false }),
  small: param("stage") === "small",
  setSmall: (small) => set({ small }),
  full: param("stage") === "full",
  setFull: (full) => set({ full }),
  forward: param("forward"),
  setForward: (forward) => set({ forward }),
  forwardBefore: undefined,
  info: param("info") === "1",
  setInfo: (info) => set({ info }),
  menus: 0,
  menuOpen: (open) => set((s) => ({ menus: Math.max(0, s.menus + (open ? 1 : -1)) })),
}));

/** Where the stage shows in this window (sqol.md §4: U3 docks the room panel beside "page"). */
export type StageMode = "page" | "corner" | "full" | null;

/** SQOL: the stage's place, for the room panel (`useShareStage(s => s.mode)`). */
export const useShareStage = create<{ mode: StageMode }>()(() => ({ mode: null }));

/** Screen sharing in the room now (nothing of it while it is off). */
export function useShare(): VoiceShareView {
  return useVoiceUi((s) => (s.state.share?.on ? s.state.share : HIDDEN_SHARE_VIEW));
}

/** Set by the "Yayın" window's page: no friend list, no report dialog there. */
let inPopout = false;

export function markPopout() {
  inPopout = true;
}

/** This page is the "Yayın" window's. */
export function isPopout(): boolean {
  return inPopout;
}

/** A sharer's name on this page (the "Yayın" window has no friend list: the launcher names them). */
export function useShareName(id: string): string {
  const face = useFace(id);
  const names = useShare().names;
  return inPopout ? (names[id] ?? face.name) : face.name;
}

/**
 * A crew room's "Kim paylaşabilir" for its edit form (outside a room as well): offered while the
 * service shares screens.
 */
export function useCrewShareBy(room: string): { offered: boolean; by: ShareBy } {
  const offered = useVoiceUi((s) => s.state.share?.service ?? false);
  const mods = useVoiceUi((s) => s.state.share?.crewMods.includes(room) ?? false);
  return { offered, by: mods ? "mods" : "all" };
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

/** Every change of the page's streams, their quality or what arrives of them. */
export function subscribeStreams(listener: () => void): () => void {
  engineListeners.add(listener);
  let bound = engine;
  let stop = bound?.subscribe(listener);
  // A new engine (a new room) takes the listener over.
  const follow = () => {
    if (engine === bound) return;
    stop?.();
    bound = engine;
    stop = bound?.subscribe(listener);
  };
  engineListeners.add(follow);
  return () => {
    engineListeners.delete(listener);
    engineListeners.delete(follow);
    stop?.();
  };
}

/** A watched share's picture and sound on this page, when it came. */
export function useShareStream(person: string, n: number): MediaStream | null {
  return useSyncExternalStore(subscribeStreams, () => engine?.stream(person, n) ?? null, noStream);
}

/** A watched share's quality on this page (the preview's `weak=<person>` makes one weak). */
export function useShareQuality(person: string, n: number): ShareQuality {
  const quality = useSyncExternalStore(
    subscribeStreams,
    () => engine?.quality(person, n) ?? "good",
    () => "good" as const,
  );
  return isTauri() ? quality : mockQuality(person);
}

/** "Yayın Bilgileri" of a watched share while it shows (the preview makes them up). */
export function useShareInfo(
  person: string,
  n: number,
  on: boolean,
  layer: "f" | "q",
): ShareInfo | null {
  useEffect(() => {
    if (!on) return;
    let release = engine?.watchInfo();
    // The engine may come after the tile (a new room): it starts keeping them then.
    const follow = () => {
      release?.();
      release = engine?.watchInfo();
    };
    engineListeners.add(follow);
    return () => {
      engineListeners.delete(follow);
      release?.();
    };
  }, [on]);
  const info = useSyncExternalStore(
    subscribeStreams,
    () => (on ? (engine?.info(person, n) ?? null) : null),
    noStream,
  );
  if (!on) return null;
  return isTauri() ? info : mockInfo(person, layer);
}
