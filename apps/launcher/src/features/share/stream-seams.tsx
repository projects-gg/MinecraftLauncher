// What the stage takes from the other SQOL parts (sqol.md §4). The player's own share's (SQOL-U2):
// its menu, its live preview (asked while the tile shows, counted with the card's), the tile its
// card's picture asks for, and what it sends ("Yayın Bilgileri" on the own tile). The room panel's
// (SQOL-U3: `useVoiceUi(s => s.dock)`, `openRoomPanel({focus: "chat"})`, `useRoomChatOffered()`,
// `useRoomChatUnread()`, `<RoomChatPeek/>`) until its branch lands: each stand-in is what the stage
// does without that part; when it lands, its line here becomes a re-export.
import { useVoiceUi } from "@/features/voice/store";
import { isTauri } from "@/lib/platform";
import { param } from "@/share/param";
import { statsLine, useOwnSharePreview, useOwnShareStats } from "./own-live";
import { useOwnShareItems } from "./own-menu";
import { hideOwnTile, showOwnTile, useOwnTile } from "./own-store";

// --- SQOL-U2 ------------------------------------------------------------------------------------

export { hideOwnTile, useOwnShareItems, useOwnTile };

// The preview's own tile (`own=1`), as a press on the card's picture would ask.
const previewOwn = Number(param("own"));
if (!isTauri() && Number.isFinite(previewOwn) && previewOwn > 0) showOwnTile(previewOwn);

/** The player's own share's newest preview frame while its tile shows (the launcher's, ≤ 2 a second). */
export function useOwnPreviewFrame(n: number): string | null {
  return useOwnSharePreview(n, true);
}

/** What the player's own share really sends ("1280×720 · 30 fps · Donanım"), null until said. */
export function useOwnShareInfo(n: number): string | null {
  const stats = useOwnShareStats(n);
  return stats ? statsLine(stats) : null;
}

// --- SQOL-U3 ------------------------------------------------------------------------------------

/** SQOL-U3: how wide the docked room panel is (px; 0 while it is closed or not docked). */
export function useStageDock(): number {
  return 0;
}

/** SQOL-U3: the room has a chat area ("Oda Sohbeti" or the friend's messages). */
export function useRoomChatOffered(): boolean {
  return param("chat") === "1";
}

/** SQOL-U3: messages the player has not seen while the panel was closed. */
export function useRoomChatUnread(): number {
  const value = Number(param("unread"));
  return Number.isFinite(value) && value > 0 ? value : 0;
}

/** SQOL-U3: "Sohbet" on the stage opens the room panel on its chat. */
export function openRoomChat(from: HTMLElement | null): void {
  useVoiceUi.getState().openPanel({ from });
}

/** SQOL-U3: up to three short previews of new messages, top right of the stage. */
export function RoomChatPeek() {
  return null;
}
