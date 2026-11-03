// SOC-NEXT §3.9 in an open conversation: one marked unread here counts one message when it opens
// ("Yeni mesajlar" goes over its newest message), and the mark goes once the conversation was on
// screen at its newest message for as long as a read takes (the window in front).
import { useEffect } from "react";
import { READ_AFTER_MS } from "../chat/seen";
import { useSocialUi } from "../store";
import { useSocNext } from "./flag";
import { useSocNextPrefs } from "./prefs";

/** 1 while the player's own "Okunmadı say" holds for the conversation, else 0. */
export function useMarkedUnread(conv: string | null): number {
  const next = useSocNext();
  const mark = useSocNextPrefs((s) => (next && conv ? s.unreadMarks[conv] : undefined));
  const lastAt = useSocialUi((s) =>
    mark === undefined
      ? undefined
      : s.view.chat?.conversations.find((c) => c.id === conv)?.last?.at,
  );
  return mark !== undefined && lastAt !== undefined && lastAt <= mark ? 1 : 0;
}

/** The mark goes once the conversation is read on screen (`live`: open, at its newest message). */
export function useMarkSeen(conv: string, live: boolean) {
  const mark = useSocNextPrefs((s) => s.unreadMarks[conv]);
  useEffect(() => {
    if (!live || mark === undefined) return;
    const clear = () => {
      if (document.hasFocus()) useSocNextPrefs.getState().clearUnread(conv);
    };
    let timer = window.setTimeout(clear, READ_AFTER_MS);
    const again = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(clear, READ_AFTER_MS);
    };
    window.addEventListener("focus", again);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("focus", again);
    };
  }, [live, mark, conv]);
}
