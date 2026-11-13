// What the room panel's chat has waiting, for the places that open it (SQOL §1.3 "Okunmamış"): the
// title bar's room pill and the stage's "Sohbet" (SQOL-U1).
import { useShareStage } from "@/features/share/store";
import { leaveFullscreen } from "@/features/share/stream-actions";
import { useSocialUi } from "@/features/social/store";
import { useVoiceUi } from "../store";
import { usePanelChat } from "./panel-chat";
import { useRoomChat } from "./store";

/**
 * Opens the room panel (beside the stage while the stage is on the page); `focus: "chat"` puts the
 * focus in its chat's field. `from` gets the focus back when it closes (the focused element when
 * none is given). A full-screen stage covers the panel: it goes back to the page first (SQOL-U1's
 * own way out), so "Sohbet" and a preview pressed there open the panel docked beside it.
 */
export function openRoomPanel(options?: { focus?: "chat"; from?: HTMLElement | null }): void {
  if (useShareStage.getState().mode === "full") leaveFullscreen();
  const from =
    options?.from ??
    (document.activeElement instanceof HTMLElement ? document.activeElement : null);
  useVoiceUi.getState().openPanel({ from, ...(options?.focus ? { focus: options.focus } : {}) });
}

/**
 * The room panel has a chat now: the room's own, or a call's friend's conversation (the stage's
 * "Sohbet" shows only then).
 */
export function useRoomChatOffered(): boolean {
  const room = useVoiceUi((s) => s.state.room);
  return usePanelChat(room).kind !== "none";
}

/** The room's own chat's unread messages (none while the panel shows another chat). */
export function useRoomOwnUnread(): number {
  const room = useVoiceUi((s) => s.state.room);
  const choice = usePanelChat(room);
  const unread = useRoomChat((s) => (room && s.room === room.id ? s.unread : 0));
  return choice.kind === "room" ? unread : 0;
}

/**
 * The unread messages of the chat the room panel shows: the room's own, or (a call with a friend)
 * the conversation with that friend. The stage's "Sohbet" shows it.
 */
export function useRoomChatUnread(): number {
  const room = useVoiceUi((s) => s.state.room);
  const choice = usePanelChat(room);
  const own = useRoomChat((s) => (room && s.room === room.id ? s.unread : 0));
  const direct = useSocialUi((s) =>
    choice.kind === "dm"
      ? (s.view.chat?.conversations.find((c) => c.id === choice.conv)?.unread ?? 0)
      : 0,
  );
  return choice.kind === "dm" ? direct : choice.kind === "room" ? own : 0;
}
