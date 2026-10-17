// Which news the player has already seen, for the small gold dot on new ones: a watermark (news
// published before it are not new) and the articles opened since. Kept on this computer.
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { NewsItem } from "@/lib/api";

const DAY = 86_400_000;
/** News older than this is never new. */
export const NEW_FOR_MS = 14 * DAY;
/** Opened articles are remembered this long: longer than an article can be new. */
export const READ_KEPT_MS = 30 * DAY;

export interface NewsSeen {
  /** News published at or before this moment (ms) count as seen; `null` until the first start. */
  watermark: number | null;
  /** Opened articles: news id → when (ms). */
  read: Record<string, number>;
}

/**
 * A news item is new when it was published after the watermark, less than 14 days ago, and has
 * not been opened. Pinned news follows the same rule: an old pinned article is not new.
 */
export function isNewsNew(
  item: Pick<NewsItem, "id" | "publishedAt">,
  seen: NewsSeen,
  now: number,
): boolean {
  if (seen.watermark === null || item.id in seen.read) return false;
  const published = Date.parse(item.publishedAt);
  return published > seen.watermark && published > now - NEW_FOR_MS;
}

/** The newest publishing moment in the list (0 when empty). */
export function newestPublished(items: Pick<NewsItem, "publishedAt">[]): number {
  let newest = 0;
  for (const item of items) {
    const at = Date.parse(item.publishedAt);
    if (at > newest) newest = at;
  }
  return newest;
}

function withoutOld(read: Record<string, number>, now: number): Record<string, number> {
  return Object.fromEntries(Object.entries(read).filter(([, at]) => at > now - READ_KEPT_MS));
}

interface NewsSeenState extends NewsSeen {
  /** The article was opened. */
  markRead: (id: string, now?: number) => void;
  /** After a visit to the news page: everything published up to `to` counts as seen. */
  advance: (to: number, now?: number) => void;
  /** First start: news published so far is not new (a fresh install does not light up old news). */
  start: (now: number) => void;
}

export const useNewsSeen = create<NewsSeenState>()(
  persist(
    (set, get) => ({
      watermark: null,
      read: {},
      markRead: (id, now = Date.now()) => {
        if (id in get().read) return;
        set({ read: { ...withoutOld(get().read, now), [id]: now } });
      },
      advance: (to, now = Date.now()) => {
        const { watermark, read } = get();
        if (watermark !== null && to <= watermark) return;
        set({ watermark: to, read: withoutOld(read, now) });
      },
      start: (now) => {
        if (get().watermark === null) set({ watermark: now });
      },
    }),
    {
      name: "pl.newsSeen",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: ({ watermark, read }) => ({ watermark, read }),
    },
  ),
);

useNewsSeen.getState().start(Date.now());
