import type { OverlayConfig } from "@/bindings";
import type { OverlayApi } from "./api";

/**
 * The overlay in the browser preview: nothing runs. `?overlay=connected` (or localStorage
 * `pl.mock.overlay`) acts as if a game's mod were connected, so the page skips the system
 * notifications its toasts stand in for.
 */
function connected(): boolean {
  try {
    return (
      (new URLSearchParams(window.location.search).get("overlay") ??
        localStorage.getItem("pl.mock.overlay")) === "connected"
    );
  } catch {
    return false;
  }
}

let config: OverlayConfig = {
  enabled: true,
  reducedMotion: false,
  notify: {
    crewCalls: true,
    crewChat: false,
    chatMutes: {},
    favorites: [],
    onlineAlerts: {},
    favoritesOnline: true,
  },
};

export const mockOverlay: OverlayApi = {
  async configure(next) {
    config = next;
  },
  async state() {
    return { config, connected: connected(), versions: ["26.1.2", "26.2", "26.3"] };
  },
  onStatus() {
    return () => {};
  },
  onEnabled() {
    return () => {};
  },
  onPeople() {
    return () => {};
  },
};
