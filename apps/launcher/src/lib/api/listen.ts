// Tauri's `listen`, with one Tauri listener per event for the page, shared by its subscribers.
// Tauri forgets a listener's callback the moment the page lets it go, the launcher's side only a
// moment later: every payload already on its way then finds no callback ("[TAURI] Couldn't find
// callback id", BATCH-206; voice-tauri.ts keeps its own for the voice events). Here a subscriber
// that leaves only leaves the set: the Tauri listener stays while anyone listens and a short while
// after the last one left (payloads still on their way land in it and go nowhere), and a
// subscriber back within that while takes it up again (a window's focus or sight coming and going
// no longer lets go and listens again each time).
import {
  type EventCallback,
  type EventName,
  type Event as TauriEvent,
  listen as tauriListen,
  type UnlistenFn,
} from "@tauri-apps/api/event";

/** How long the Tauri listener outlives its last subscriber. */
export const LISTEN_LINGER_MS = 5_000;

interface Shared {
  subscribers: Set<(event: TauriEvent<unknown>) => void>;
  ready: Promise<UnlistenFn>;
  release: ReturnType<typeof setTimeout> | null;
}

const shared = new Map<string, Shared>();

function open(event: EventName): Shared {
  const subscribers = new Set<(event: TauriEvent<unknown>) => void>();
  const ready = tauriListen<unknown>(event, (e) => {
    for (const one of [...subscribers]) {
      try {
        one(e);
      } catch (error) {
        // One subscriber's error neither stops the others nor goes unseen.
        queueMicrotask(() => {
          throw error;
        });
      }
    }
  });
  const made: Shared = { subscribers, ready, release: null };
  shared.set(event, made);
  // A listener Tauri refused is asked for again by the next subscriber.
  ready.catch(() => {
    if (shared.get(event) === made) shared.delete(event);
  });
  return made;
}

/** Like Tauri's `listen`: `handler` gets each `event` until the returned stop is called. */
export async function listen<T>(event: EventName, handler: EventCallback<T>): Promise<UnlistenFn> {
  const mine = shared.get(event) ?? open(event);
  if (mine.release !== null) {
    clearTimeout(mine.release);
    mine.release = null;
  }
  // Its own entry: the same handler twice is two subscriptions, as in Tauri.
  const one = (e: TauriEvent<unknown>) => handler(e as TauriEvent<T>);
  mine.subscribers.add(one);
  try {
    await mine.ready;
  } catch (error) {
    mine.subscribers.delete(one);
    throw error;
  }
  let stopped = false;
  return () => {
    if (stopped) return;
    stopped = true;
    mine.subscribers.delete(one);
    if (mine.subscribers.size > 0 || mine.release !== null) return;
    mine.release = setTimeout(() => {
      mine.release = null;
      if (mine.subscribers.size > 0 || shared.get(event) !== mine) return;
      shared.delete(event);
      void mine.ready.then((stop) => stop()).catch(() => {});
    }, LISTEN_LINGER_MS);
  };
}
