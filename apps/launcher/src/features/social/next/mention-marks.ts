// SOC-NEXT §3.1: crew conversations with an unread message that mentions the player, for the
// small "@" beside their unread count in "Mesajlar". Kept while the launcher runs: a message that
// came live marks it, and the mark goes once nothing there is unread any more (the list's newest
// message is read for one that came while the launcher was closed).
import { create } from "zustand";
import type { Conversation } from "@/lib/api/social-types";

export const useMentionMarks = create<{ convs: Readonly<Record<string, true>> }>()(() => ({
  convs: {},
}));

export function markMention(conv: string) {
  const { convs } = useMentionMarks.getState();
  if (!convs[conv]) useMentionMarks.setState({ convs: { ...convs, [conv]: true } });
}

/** Marks of conversations with nothing unread go. */
export function pruneMentions(conversations: readonly Pick<Conversation, "id" | "unread">[]) {
  const { convs } = useMentionMarks.getState();
  const ids = Object.keys(convs);
  if (!ids.length) return;
  const keep = ids.filter((id) => conversations.some((conv) => conv.id === id && conv.unread > 0));
  if (keep.length === ids.length) return;
  useMentionMarks.setState({ convs: Object.fromEntries(keep.map((id) => [id, true as const])) });
}
