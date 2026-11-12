// What the stage takes from the other SQOL parts (sqol.md §4), until their branches land: the room
// panel's (SQOL-U3: `useVoiceUi(s => s.dock)`, `openRoomPanel({focus: "chat"})`,
// `useRoomChatUnread()`, `<RoomChatPeek/>`) and the player's own share's (SQOL-U2:
// `useOwnShareItems(n)`, the preview frames its card asks the launcher for). Each stand-in is what
// the stage does without that part; when the part lands, its line here becomes a re-export.
import type { MenuItem } from "@pl/ui";
import { useVoiceUi } from "@/features/voice/store";
import { isTauri } from "@/lib/platform";
import { mockFrame } from "@/share/mock-scenarios";
import { param } from "@/share/param";

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

/** SQOL-U2: the rows of the player's own share's menu ("Ekranı Değiştir…", "Kalite", …). */
export function useOwnShareItems(_n: number): Array<MenuItem | null> {
  return [];
}

/** SQOL-U2: the player's own share's newest preview frame (the launcher's, ≤ 2 a second). */
export function useOwnPreviewFrame(_n: number): string | null {
  const me = useVoiceUi((s) => s.state.room?.me.id ?? null);
  return !isTauri() && me ? mockFrame(me) : null;
}
