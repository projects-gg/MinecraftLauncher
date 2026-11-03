// What the friends entries (the title bar's pill, the rail's entry) count as waiting, with SOC-NEXT:
// a direct conversation with someone ignored waits for no one (as one muted here), and one marked
// unread here counts one.
import { useMemo } from "react";
import type { ChatMutes } from "../chat/conversations";
import { directConversation } from "../chat/conversations";
import { useSocialPrefs, useSocialUi } from "../store";
import { useSocNext } from "./flag";
import { ignoredDirectMutes } from "./ignore";
import { useSocNextPrefs } from "./prefs";

const NO_MARKS: Readonly<Record<string, number>> = {};

export function useWaitPrefs(): { mutes: ChatMutes; marks: Readonly<Record<string, number>> } {
  const next = useSocNext();
  const chatMutes = useSocialPrefs((s) => s.chatMutes);
  const ignored = useSocNextPrefs((s) => s.ignored);
  const marks = useSocNextPrefs((s) => s.unreadMarks);
  const me = useSocialUi((s) => s.view.me?.id);
  const mutes = useMemo(
    () =>
      next && ignored.length
        ? { ...chatMutes, ...ignoredDirectMutes(me, ignored, directConversation) }
        : chatMutes,
    [next, ignored, chatMutes, me],
  );
  return { mutes, marks: next ? marks : NO_MARKS };
}
