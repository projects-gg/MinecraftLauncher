// The event reminders the player set ("Hatırlat"), kept across restarts on this computer. The
// rules are in features/home/reminders.ts.
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { Reminders } from "@/features/home/reminders";
import type { EventItem } from "@/lib/api";

interface RemindersState {
  /** By event id. */
  items: Reminders;
  /** The first reminder's "we will tell you 5 minutes before" toast was shown. */
  explained: boolean;
  /** Sets a reminder for the event, or removes it. Returns whether one is set now. */
  toggle: (event: EventItem) => boolean;
  /** These reminders were told (or passed over while the game ran) at `at`. */
  markFired: (ids: string[], at: number) => void;
  /** The reconciled reminders (moved or ended events). */
  replace: (items: Reminders) => void;
  setExplained: () => void;
}

export const useReminders = create<RemindersState>()(
  persist(
    (set, get) => ({
      items: {},
      explained: false,
      toggle: (event) => {
        const { items } = get();
        const on = !items[event.id];
        set({
          items: on
            ? { ...items, [event.id]: { startsAt: event.startsAt, endsAt: event.endsAt } }
            : Object.fromEntries(Object.entries(items).filter(([id]) => id !== event.id)),
        });
        return on;
      },
      markFired: (ids, at) => {
        const items = { ...get().items };
        for (const id of ids) {
          const reminder = items[id];
          if (reminder) items[id] = { ...reminder, firedAt: at };
        }
        set({ items });
      },
      replace: (items) => set({ items }),
      setExplained: () => set({ explained: true }),
    }),
    {
      name: "pl.reminders",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: ({ items, explained }) => ({ items, explained }),
    },
  ),
);
