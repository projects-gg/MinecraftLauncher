import { create } from "zustand";
import type { SocialNotice } from "@/lib/api/social-types";

interface OverlayState {
  /** The mod in the running game is connected to the launcher. */
  connected: boolean;
  setConnected: (connected: boolean) => void;
}

export const useOverlay = create<OverlayState>()((set) => ({
  connected: false,
  setConnected: (connected) => set({ connected }),
}));

/**
 * What the in-game overlay tells as its own toasts (`crates/pl-overlay` policy): a chat message, a
 * crew call, a "Birlikte oyna" invite, and Cloud AFK's notices it has a card for (pl-overlay
 * afk.rs; the AFK+ notices module checks the event). The rest keep the launcher's notifications.
 */
const IN_GAME: ReadonlySet<SocialNotice["kind"]> = new Set([
  "chatMessage",
  "crewCall",
  "invite",
  "afkNotice",
]);

/**
 * While the game's overlay is connected, its toast stands in for the system notification of the
 * same event: one notification, never two. (The overlay decides with the same rules,
 * `notify-rules.test.ts`.)
 */
export function shownInGame(kind: SocialNotice["kind"]): boolean {
  return useOverlay.getState().connected && IN_GAME.has(kind);
}
