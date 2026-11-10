// "Sohbete Git" (CREW-UI; the owner: "sohbetler buton olarak sesli yazısının üstünde olsun"): the
// crew page's way into its chat, right under the crew's card and above "Sesli" (in the same place
// while voice is off). One quiet row like "Ekip bul": a chamfered mark, the words, how many are
// unread (the gold chip of the "Mesajlar" row: the same conversation, the same mark, with SOC-NEXT's
// "Okunmadı say" and its "@") and the caret of a page one step deeper. The chat opens on the
// panel's chat page, sliding in from the right; its "Geri" comes back here, and so does the focus.
// (Reading the messages ahead, on hover, was tried and left: the page then mounts with every row in
// the press's frame and its slide loses its start; light, it slides at once and the rows follow
// while it moves, as a friend's chat does.)
import { CaretRight, ChatsCircle } from "@phosphor-icons/react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { SocialTeam } from "@/lib/api/social-types";
import { useMarkedUnread } from "../next/conv-hooks";
import { useMentionShown } from "../next/mention-marks";
import { useSocialUi } from "../store";
import { crewConversation } from "./conversations";
import { UnreadChip } from "./unread-chip";

/** The crew chat the player just left with "Geri" for its crew page (the page it left is gone, so
 * the focus would fall to the panel): that page's "Sohbete Git" takes it back, once. */
let backFrom: { conv: string; at: number } | null = null;

export function noteBackToCrew(conv: string) {
  backFrom = { conv, at: Date.now() };
}

export function CrewChatEntry({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const conv = crewConversation(team.id);
  const conversation = useSocialUi((s) => s.view.chat?.conversations.find((c) => c.id === conv));
  const me = useSocialUi((s) => s.view.me?.id);
  // One marked unread here counts one (SOC-NEXT), as the tab it replaced counted.
  const marked = useMarkedUnread(conv);
  const count = conversation?.unread || marked;
  const mentioned = useMentionShown(conversation, count > 0, team, me);
  const ref = useRef<HTMLButtonElement>(null);

  // Back from this crew's chat, the focus comes back here (a later visit leaves it alone).
  useEffect(() => {
    const back = backFrom;
    if (!back || back.conv !== conv) return;
    backFrom = null;
    if (Date.now() - back.at < 2000) ref.current?.focus({ preventScroll: true });
  }, [conv]);

  const label = t("crewUi.goToChat");
  // Screen readers hear the count with its words ("Sohbete Git, 3 okunmamış mesaj").
  const named = [
    label,
    count > 0 ? t("chat.list.unread", { count }) : null,
    mentioned ? t("socNext.mention.you") : null,
  ];
  return (
    <button
      ref={ref}
      type="button"
      aria-label={count > 0 ? named.filter(Boolean).join(", ") : undefined}
      onClick={() => useSocialUi.getState().openChat(conv, { from: "team" })}
      className="group mt-3 flex w-full items-center gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/40 px-4 py-2.5 text-left ring-1 ring-white/8 outline-none transition-[background-color,box-shadow,scale] duration-150 hover:bg-white/[0.04] hover:ring-white/14 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.99]"
    >
      <span
        aria-hidden
        className="chamfer flex size-8 shrink-0 items-center justify-center bg-white/7 text-bone-100 [--cut:6px]"
      >
        <ChatsCircle size={18} />
      </span>
      <span className="min-w-0 flex-1 truncate text-[14px] leading-tight font-semibold text-bone-50">
        {label}
      </span>
      <UnreadChip count={count} mentioned={mentioned} />
      <CaretRight
        size={15}
        weight="bold"
        aria-hidden
        className="shrink-0 text-ash-400 transition-[translate,color] duration-150 group-hover:translate-x-0.5 group-hover:text-bone-100 motion-reduce:group-hover:translate-x-0"
      />
    </button>
  );
}
