// The single-key voice shortcuts rest while the player writes (SQOL §1.3, R's `voice_typing`): a
// mute key without a modifier ("M") muted the microphone in the middle of a chat message
// (key-picker.tsx warns of it). While a text field of this window has the focus and the window has
// the keyboard, the launcher holds the shortcuts that write; once the field lets go, or the player
// goes to the game, they work again. Only in a room, and only changes are sent.
import { useEffect } from "react";
import { roomChatApi } from "./room-chat/api";
import { useVoiceUi } from "./store";

const TEXT_INPUTS = new Set(["text", "search", "email", "url", "tel", "password", "number"]);

/** Whether the element takes typed words (a field, a text area, editable text). */
export function writesText(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) return false;
  if (element.isContentEditable) return true;
  if (element instanceof HTMLTextAreaElement) return !element.readOnly && !element.disabled;
  if (element instanceof HTMLInputElement) {
    return TEXT_INPUTS.has(element.type) && !element.readOnly && !element.disabled;
  }
  return false;
}

export function useVoiceTyping() {
  const inRoom = useVoiceUi((s) => s.state.room !== null);
  useEffect(() => {
    if (!inRoom) return;
    let typing = false;
    let timer: number | undefined;
    const check = () => {
      window.clearTimeout(timer);
      const now = document.hasFocus() && writesText(document.activeElement);
      if (now === typing) return;
      typing = now;
      void roomChatApi.typing(now);
    };
    // Between two fields the focus is on the page's body for a moment: look once it has settled.
    const later = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(check, 0);
    };
    check();
    document.addEventListener("focusin", check);
    document.addEventListener("focusout", later);
    window.addEventListener("blur", check);
    window.addEventListener("focus", check);
    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("focusin", check);
      document.removeEventListener("focusout", later);
      window.removeEventListener("blur", check);
      window.removeEventListener("focus", check);
      if (typing) void roomChatApi.typing(false);
    };
  }, [inRoom]);
}
