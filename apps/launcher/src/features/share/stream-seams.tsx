// What the stage takes from the other SQOL parts (sqol.md §4). The player's own share's (SQOL-U2):
// its menu, its live preview (asked while the tile shows, counted with the card's), the tile its
// card's picture asks for, and what it sends ("Yayın Bilgileri" on the own tile). The room panel's
// (SQOL-U3): how wide the docked panel is, "Sohbet" opening it on its chat, whether the room has a
// chat and what is unread in it, and the chat's previews over the stage.
import { openRoomPanel } from "@/features/voice/room-chat";
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

// The room has a chat area ("Oda Sohbeti" or the call's friend's messages); what the player has not
// seen in it while the panel was closed; up to three short previews of new messages, top right of
// the stage (the panel's own stand-in steps back while the stage shows them).
export { RoomChatPeek, useRoomChatOffered, useRoomChatUnread } from "@/features/voice/room-chat";

/** How wide the docked room panel is (px; 0 while it is closed, hidden or not docked). */
export function useStageDock(): number {
  return useVoiceUi((s) => s.dock);
}

/** "Sohbet" on the stage opens the room panel on its chat (the focus goes to its field). */
export function openRoomChat(from: HTMLElement | null): void {
  openRoomPanel({ focus: "chat", from });
}
