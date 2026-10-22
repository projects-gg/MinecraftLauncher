// A crew's "Sohbet": the crew's conversation, with names over others' groups and the quick replies
// over the composer. Its leader and moderators may delete any message (the service decides; the
// menu offers it where the view says so). The crew's plans show as quiet lines when they were made
// (TEAM5), from the plans this computer has.
import { ChatsCircle } from "@phosphor-icons/react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { ChatMessage, SocialTeam } from "@/lib/api/social-types";
import { currentLanguage } from "@/lib/i18n";
import { planWhen, withPlanNote } from "../crew-plans";
import { useSocialUi } from "../store";
import { useComposerNotice, useComposerState, usePeople } from "./chat-page";
import { type ChatLine, Conversation, useUnreadAtOpen } from "./conversation";
import { crewConversation } from "./conversations";
import { messageLine } from "./preview";

/** "Bugün 21:00 için plan yaptın": the line starts with its time, so it starts with a capital. */
function capitalized(text: string): string {
  const locale = currentLanguage() === "tr" ? "tr-TR" : "en-US";
  return text.charAt(0).toLocaleUpperCase(locale) + text.slice(1);
}

export function CrewChat({ team, onScreen }: { team: SocialTeam; onScreen: boolean }) {
  const { t, i18n } = useTranslation();
  const conv = crewConversation(team.id);
  const openReport = useSocialUi((s) => s.openReport);
  const people = usePeople();
  const composer = useComposerState(conv, true, true);
  const notice = useComposerNotice();
  const unreadAtOpen = useUnreadAtOpen(conv);
  const plans = useSocialUi((s) => s.view.plans);
  const lines = useMemo<ChatLine[]>(
    () =>
      plans
        .filter((plan) => plan.crew === team.id && plan.createdAt > 0)
        .map((plan) => {
          const when = planWhen(plan.at, Date.now(), t, true);
          return {
            key: `plan:${plan.id}`,
            at: plan.createdAt * 1000,
            text: withPlanNote(
              plan.mine
                ? capitalized(t("crewWave2.plans.chatMine", { when }))
                : t("crewWave2.plans.notice", { name: plan.byName ?? "?", when }),
              plan.note,
              t,
            ),
          };
        }),
    [plans, team.id, t],
  );
  const report = (message: ChatMessage) => {
    if (message.id === undefined) return;
    const sender = people(message.from);
    openReport({
      target: { kind: "message", conv, id: message.id },
      name: sender.name,
      person: sender,
      text: messageLine(message, t, i18n.language),
    });
  };
  return (
    <Conversation
      conv={conv}
      crew
      people={people}
      composer={composer}
      notice={notice}
      unreadAtOpen={unreadAtOpen}
      onScreen={onScreen}
      onReport={report}
      lines={lines}
      empty={
        <div className="flex flex-col items-center gap-2 text-center">
          <ChatsCircle size={28} className="text-ash-400" aria-hidden />
          <p className="text-[13px] text-ash-400">{t("chat.list.noMessages")}</p>
        </div>
      }
    />
  );
}
