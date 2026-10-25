// Browser previews: the one live stream the 7/24 Bulut mock and the AFK+ mock share (the real app's
// `cloud-afk-live` event).
import type { CloudLiveEvent } from "@/bindings";

const listeners = new Set<(event: CloudLiveEvent) => void>();

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
};
