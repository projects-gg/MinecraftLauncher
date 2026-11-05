import { create } from "zustand";
import type { SocialNotice } from "@/lib/api/social-types";

interface OverlayState {
  /** The mod in the running game is connected to the launcher. */
  connected: boolean;
  /**
   * OVL-SOC2: the notice kinds the connected games show as their own cards (`OverlayStatus.kinds`):
   * an older mod's chat message, crew call, "Birlikte oyna" and Cloud AFK's notices; a newer one's
   * requests, crew invites, plans and friends coming online too, less the kinds the player turned
   * off in it.
   */
  kinds: readonly string[];
  setConnected: (connected: boolean, kinds?: readonly string[]) => void;
}

export const useOverlay = create<OverlayState>()((set) => ({
  connected: false,
  kinds: [],
  setConnected: (connected, kinds) => set(kinds ? { connected, kinds } : { connected }),
}));

/**
 * While the game's overlay is connected, its card stands in for the system notification of the
 * same event: one notification, never two. (The overlay decides with the same rules,
 * `notify-rules.test.ts`; which kinds it shows comes from Rust, `crates/pl-overlay` session.)
 */
export function shownInGame(kind: SocialNotice["kind"]): boolean {
  const { connected, kinds } = useOverlay.getState();
  return connected && kinds.includes(kind);
}
