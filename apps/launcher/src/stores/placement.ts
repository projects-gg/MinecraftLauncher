// Where the player keeps the shell's movable entries: in the title bar or in the side menu (the
// rail). A small registry, so another entry can get the same choice later; only the friends button
// uses it so far. The choice is made from the entry's right-click menu or in Settings → Görünüm,
// and kept on this computer (a layout choice, like the news dot and closed banners; the settings
// backup carries launcher preferences only).
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type Place = "titlebar" | "rail";

/** The entries that can move: where each starts and the places it may take. */
export const MOVABLE = {
  friends: { home: "titlebar", places: ["titlebar", "rail"] },
} as const satisfies Record<string, { home: Place; places: readonly Place[] }>;

export type MovableId = keyof typeof MOVABLE;

/** Where an entry is: the player's choice when the entry may be there, else its home. */
export function placeOf(id: MovableId, chosen: Readonly<Record<string, unknown>>): Place {
  const entry = MOVABLE[id];
  const value = chosen[id];
  return (entry.places as readonly unknown[]).includes(value) ? (value as Place) : entry.home;
}

/** The place a right-click menu offers: the next one after where the entry is now. */
export function nextPlace(id: MovableId, place: Place): Place {
  const places: readonly Place[] = MOVABLE[id].places;
  return places[(places.indexOf(place) + 1) % places.length] ?? place;
}

/** How long after a move the entry's arrival still counts as the move (and animates). */
export const MOVE_WINDOW_MS = 1500;

interface PlacementState {
  chosen: Partial<Record<MovableId, Place>>;
  /** The latest move (not kept): the entry arriving at its new place animates. */
  lastMove: { id: MovableId; at: number } | null;
  move: (id: MovableId, place: Place) => void;
}

export const usePlacement = create<PlacementState>()(
  persist(
    (set, get) => ({
      chosen: {},
      lastMove: null,
      move: (id, place) => {
        if (placeOf(id, get().chosen) === place) return;
        set((s) => ({ chosen: { ...s.chosen, [id]: place }, lastMove: { id, at: Date.now() } }));
      },
    }),
    {
      name: "pl.placement",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: ({ chosen }) => ({ chosen }),
    },
  ),
);

/** Where the entry is now. */
export function usePlace(id: MovableId): Place {
  return usePlacement((s) => placeOf(id, s.chosen));
}

/** The entry just moved (it arrives with an animation rather than simply being there). */
export function justMoved(id: MovableId, now = Date.now()): boolean {
  const move = usePlacement.getState().lastMove;
  return move?.id === id && now - move.at < MOVE_WINDOW_MS;
}

/** Finds an entry's button at a place (both may be on screen while one leaves and one arrives). */
export function entrySelector(id: MovableId, place: Place): string {
  return `[data-movable="${id}"][data-place="${place}"]`;
}
