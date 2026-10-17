import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { InstanceSummary } from "@/lib/api";

export type LibrarySort = "lastPlayed" | "name" | "playTime" | "created";

interface LibraryViewState {
  sort: LibrarySort;
  /** Keys of collapsed sections (`official`, `ungrouped`, `group:<name>`). */
  collapsed: string[];
  setSort: (sort: LibrarySort) => void;
  toggleSection: (key: string) => void;
}

/** How the library is arranged; remembered per computer. */
export const useLibraryView = create<LibraryViewState>()(
  persist(
    (set) => ({
      sort: "lastPlayed",
      collapsed: [],
      setSort: (sort) => set({ sort }),
      toggleSection: (key) =>
        set((state) => ({
          collapsed: state.collapsed.includes(key)
            ? state.collapsed.filter((k) => k !== key)
            : [...state.collapsed, key],
        })),
    }),
    { name: "pl.library", storage: createJSONStorage(() => localStorage) },
  ),
);

const time = (iso: string | undefined) => (iso ? new Date(iso).getTime() || 0 : 0);
const byName = (a: InstanceSummary, b: InstanceSummary) => a.name.localeCompare(b.name, "tr");

/** Sorted copy. Ties (and never played instances) fall back to the name. */
export function sortInstances(list: InstanceSummary[], sort: LibrarySort): InstanceSummary[] {
  const sorted = [...list];
  switch (sort) {
    case "name":
      return sorted.sort(byName);
    case "playTime":
      return sorted.sort((a, b) => b.playTimeSecs - a.playTimeSecs || byName(a, b));
    case "created":
      return sorted.sort((a, b) => time(b.createdAt) - time(a.createdAt) || byName(a, b));
    default:
      return sorted.sort((a, b) => time(b.lastPlayedAt) - time(a.lastPlayedAt) || byName(a, b));
  }
}
