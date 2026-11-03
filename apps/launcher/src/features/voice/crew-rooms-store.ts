// VOICE-ROOMS: what the crew page keeps of its rooms while a change is on its way: rooms being
// deleted (gone from the list at once; the delete goes when their "Geri al" toast ends, as a mod
// pack's file does, docs/design.md "Instance tabs") and the order a drag or Alt+↑/↓ gave until the
// friends service's list says it too. Memory only.
import { create } from "zustand";

interface CrewRoomsUi {
  /** Room ids the player is deleting (never shown again unless taken back). */
  hidden: readonly string[];
  hide: (room: string) => void;
  show: (room: string) => void;
  /** Crew → the rooms' order ("Genel" aside) the player gave, until the service's list follows. */
  order: Readonly<Record<string, readonly string[]>>;
  setOrder: (crew: string, order: readonly string[] | null) => void;
}

export const useCrewRoomsUi = create<CrewRoomsUi>()((set) => ({
  hidden: [],
  hide: (room) => set((s) => (s.hidden.includes(room) ? s : { hidden: [...s.hidden, room] })),
  show: (room) => set((s) => ({ hidden: s.hidden.filter((id) => id !== room) })),
  order: {},
  setOrder: (crew, order) =>
    set((s) => {
      const next = { ...s.order };
      if (order === null) delete next[crew];
      else next[crew] = order;
      return { order: next };
    }),
}));

/**
 * The rooms ("Genel" aside) in the order the player gave, while it waits for the service: ids the
 * list no longer has drop out, new ones come at the end.
 */
export function withPendingOrder<T extends { id: string }>(
  rooms: readonly T[],
  pending: readonly string[] | undefined,
): T[] {
  if (!pending) return [...rooms];
  const byId = new Map(rooms.map((room) => [room.id, room]));
  const first = pending.map((id) => byId.get(id)).filter((room): room is T => room !== undefined);
  return [...first, ...rooms.filter((room) => !pending.includes(room.id))];
}
