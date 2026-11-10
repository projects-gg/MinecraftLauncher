// SOC-NEXT §3.1: crew conversations with an unread message that mentions the player, for the
// small "@" beside their unread count in "Mesajlar". Kept while the launcher runs: a message that
// came live marks it, and the mark goes once nothing there is unread any more (the list's newest
// message is read for one that came while the launcher was closed). The crew page's "Sohbete Git"
// shows the same "@" (CREW-UI).
import { create } from "zustand";
import type { Conversation, SocialTeam } from "@/lib/api/social-types";
import { useSocNext } from "./flag";
import { mentionOf } from "./mentions";
import { realNameOf } from "./nickname";

export const useMentionMarks = create<{ convs: Readonly<Record<string, true>> }>()(() => ({
  convs: {},
}));

/**
 * "@" beside a crew conversation's unread count: something there is unread and a message that came
 * live mentioned the player, or the newest one (someone else's, still there) does (one that came
 * while the launcher was closed).
 */
export function useMentionShown(
  conversation: Pick<Conversation, "id" | "last"> | undefined,
  unread: boolean,
  crew: Pick<SocialTeam, "members"> | undefined,
  me: string | undefined,
): boolean {
  const next = useSocNext();
  const marked = useMentionMarks((s) => next && Boolean(conversation && s.convs[conversation.id]));
  if (!next || !unread || !conversation) return false;
  if (marked) return true;
  const last = conversation.last;
  return Boolean(
    crew &&
      last &&
      !last.mine &&
      last.deletedBy === undefined &&
      mentionOf(
        last.text,
        me,
        crew.members.map((m) => ({ id: m.id, name: realNameOf(m) })),
      ) !== "none",
  );
}

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
