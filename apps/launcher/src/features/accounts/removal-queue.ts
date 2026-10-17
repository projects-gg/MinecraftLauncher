// Undoable removals: an account leaves the list at once and stays restorable for a few seconds.
// The toast drives the normal end of the window; a long safety timer makes sure every removal
// becomes final even if the toast never reports back.

/** How long the undo toast stays (Sonner pauses it while hovered). */
export const UNDO_MS = 6000;
/** Last resort; stays below the backend's own window (5 minutes). */
export const UNDO_SAFETY_MS = 4 * 60_000;

interface Timers {
  set: (fn: () => void, ms: number) => unknown;
  clear: (handle: unknown) => void;
}

const realTimers: Timers = {
  set: (fn, ms) => setTimeout(fn, ms),
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

export interface RemovalQueue<T> {
  /** Starts the undo window of `id`. */
  add(id: string, item: T): void;
  /** Takes `id` back out of the queue; null when its window already ended. */
  undo(id: string): T | null;
  /** Ends the window of `id` now (the toast closed): the removal becomes final. */
  settle(id: string): void;
  /** The most recent removal still waiting, for Ctrl+Z. */
  latest(): string | null;
  has(id: string): boolean;
  /** Makes every waiting removal final (the app is going away). */
  flush(): void;
}

export function createRemovalQueue<T>(
  finalize: (id: string, item: T) => void,
  options: { safetyMs?: number; timers?: Timers } = {},
): RemovalQueue<T> {
  const timers = options.timers ?? realTimers;
  const safetyMs = options.safetyMs ?? UNDO_SAFETY_MS;
  const pending = new Map<string, { item: T; timer: unknown }>();

  const take = (id: string) => {
    const entry = pending.get(id);
    if (!entry) return null;
    pending.delete(id);
    timers.clear(entry.timer);
    return entry;
  };
  const settle = (id: string) => {
    const entry = take(id);
    if (entry) finalize(id, entry.item);
  };

  return {
    add(id, item) {
      settle(id);
      pending.set(id, { item, timer: timers.set(() => settle(id), safetyMs) });
    },
    undo: (id) => take(id)?.item ?? null,
    settle,
    latest() {
      let last: string | null = null;
      for (const id of pending.keys()) last = id;
      return last;
    },
    has: (id) => pending.has(id),
    flush() {
      for (const id of [...pending.keys()]) settle(id);
    },
  };
}

export interface RecencyAccount {
  id: string;
  active: boolean;
  lastUsedAt?: string;
  addedAt?: string;
}

/**
 * The account that is active after `removedId` leaves: unchanged unless the removed one was
 * active, then the most recently played remaining one, then the most recently added, then the
 * first in the list (the backend's rule, so the optimistic update matches it).
 */
export function pickNextActive(
  accounts: readonly RecencyAccount[],
  removedId: string,
): string | null {
  const removed = accounts.find((a) => a.id === removedId);
  const rest = accounts.filter((a) => a.id !== removedId);
  if (!removed?.active) return rest.find((a) => a.active)?.id ?? null;
  const time = (iso?: string) => (iso ? new Date(iso).getTime() : Number.NEGATIVE_INFINITY);
  let best: RecencyAccount | null = null;
  for (const account of rest) {
    if (
      !best ||
      time(account.lastUsedAt) > time(best.lastUsedAt) ||
      (time(account.lastUsedAt) === time(best.lastUsedAt) &&
        time(account.addedAt) > time(best.addedAt))
    ) {
      best = account;
    }
  }
  return best?.id ?? null;
}
