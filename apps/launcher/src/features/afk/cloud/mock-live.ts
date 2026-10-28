// Browser previews: the one live stream the 7/24 Bulut mock and the AFK+ mock share (the real app's
// `cloud-afk-live` event), and whether the mock machine is connected. `&link=down` (or AFK+'s
// `&plus=offline`) starts it away; `window.__plMockCloudLink(online)` takes it away or brings it
// back while the page is open: only a socket on screen (a console, AFK+) hears it, the page itself
// at its next read ("Yenile"), as in the app.
import type { CloudLiveEvent } from "@/bindings";
import { isTauri } from "@/lib/platform";

const listeners = new Set<(event: CloudLiveEvent) => void>();
const linkListeners = new Set<(online: boolean) => void>();

function startsOnline(): boolean {
  try {
    const search = new URLSearchParams(window.location.search);
    const hash = new URLSearchParams(window.location.hash.split("?")[1] ?? "");
    const value = (key: string) => search.get(key) ?? hash.get(key);
    return value("link") !== "down" && value("plus") !== "offline";
  } catch {
    return true;
  }
}

let online = typeof window === "undefined" ? true : startsOnline();

export const mockLive = {
  emit(event: CloudLiveEvent) {
    for (const listener of listeners) listener(event);
  },
  listen(listener: (event: CloudLiveEvent) => void): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },
  /** Whether the mock machine is connected now. */
  machineOnline: () => online,
  /** The mock machine went away or came back (the 7/24 Bulut mock tells its open sockets). */
  onLink(listener: (online: boolean) => void): () => void {
    linkListeners.add(listener);
    return () => {
      linkListeners.delete(listener);
    };
  },
};

declare global {
  interface Window {
    /** Preview helper: the mock machine goes away or comes back (see the header of mock-live.ts). */
    __plMockCloudLink?: (online: boolean) => void;
  }
}

if (typeof window !== "undefined" && !isTauri()) {
  window.__plMockCloudLink = (next) => {
    if (next === online) return;
    online = next;
    for (const listener of linkListeners) listener(next);
  };
}
