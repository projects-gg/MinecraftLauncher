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
  emoji: true,
  reducedMotion: false,
  notify: {
    crewCalls: true,
    crewChat: false,
    chatMutes: {},
    favorites: [],
    onlineAlerts: {},
    favoritesOnline: true,
    favoritesProjects: true,
    joinRequests: true,
  },
};

/** What a connected newer mod shows as its own cards (OVL-SOC2). */
const MOCK_KINDS = [
  "afkNotice",
  "chatMessage",
  "crewCall",
  "crewPlan",
  "friendAdded",
  "friendOnProjects",
  "friendOnline",
  "invite",
  "joinRequest",
  "request",
  "requestAccepted",
  "teamInvite",
];

export const mockOverlay: OverlayApi = {
  async configure(next) {
    config = next;
  },
  async state() {
    const on = connected();
    return {
      config,
      connected: on,
      kinds: on ? MOCK_KINDS : [],
      versions: ["26.1.2", "26.2", "26.3"],
    };
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
  onEmoji() {
    return () => {};
  },
};
