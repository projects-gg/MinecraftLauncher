// A crew's "Sohbet": the crew's conversation, with names over others' groups and the quick replies
// over the composer. Its leader and moderators may delete any message (the service decides; the
// menu offers it where the view says so). The crew's plans show as quiet lines when they were made
// (TEAM5), from the plans this computer has; so do newcomers while this launcher was connected
// (TEAM6 "Hoş geldin": "Ayşe ekibe katıldı" with "Selam ver", which sends the "Selam" sticker, or
// "Hoş geldin! 👋" without stickers, once and for ten minutes; "Ekibe katıldın" for the player).
import { ChatsCircle } from "@phosphor-icons/react";
import { toast } from "@pl/ui";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { ChatMessage, CrewJoin, SocialTeam } from "@/lib/api/social-types";
import { currentLanguage } from "@/lib/i18n";
import { canGreet, nextGreetEnd } from "../badges";
import { planWhen, withPlanNote } from "../crew-plans";
import { useSocialUi } from "../store";
import { useComposerNotice, useComposerState, usePeople } from "./chat-page";
import { type ChatLine, Conversation, useUnreadAtOpen } from "./conversation";
import { crewConversation } from "./conversations";
import { messageLine } from "./preview";

const NO_JOINS: CrewJoin[] = [];

/** "Bugün 21:00 için plan yaptın": the line starts with its time, so it starts with a capital. */
function capitalized(text: string): string {
  const locale = currentLanguage() === "tr" ? "tr-TR" : "en-US";
  return text.charAt(0).toLocaleUpperCase(locale) + text.slice(1);
}

/** "Selam ver": goes as a normal message (the chat's own limits apply); the line keeps no button. */
function greet(join: CrewJoin, failed: string) {
  void api.social.greetNewcomer(join.crew, join.id).catch((error) => {
    toast.error(friendlyError(error, failed).message);
  });
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
  const blocked = useSocialUi((s) => s.view.blocked);
  const allJoins = useSocialUi((s) => s.view.crewJoins ?? NO_JOINS);
  const joins = useMemo(() => allJoins.filter((join) => join.crew === team.id), [allJoins, team]);
  const writable = composer.kind === "open";
  // "Selam ver" goes by itself after its ten minutes: one timer, for the next one only.
  const [tick, setTick] = useState(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new tick looks for the next end
  useEffect(() => {
    const end = nextGreetEnd(joins, Date.now());
    if (end === null) return;
    const timer = window.setTimeout(() => setTick((n) => n + 1), end - Date.now() + 50);
    return () => window.clearTimeout(timer);
  }, [joins, tick]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `tick` ends a "Selam ver" in time
  const lines = useMemo<ChatLine[]>(() => {
    const nowMs = Date.now();
    const planLines = plans
      .filter((plan) => plan.crew === team.id && plan.createdAt > 0)
      .map((plan): ChatLine => {
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
      });
    // Nobody the player blocked (they see each other offline).
    const shut = (id: string) =>
      blocked.some((b) => b.id === id) || Boolean(team.members.find((m) => m.id === id)?.blocked);
    const joinLines = joins
      .filter((join) => join.mine || !shut(join.id))
      .map(
        (join): ChatLine => ({
          key: `welcome:${join.id}:${join.at}`,
          at: join.at * 1000,
          kind: "welcome",
          text: join.mine
            ? t("team6.welcome.you")
            : t("crewWave2.requests.accepted", { name: join.name }),
          action:
            writable && canGreet(join, nowMs)
              ? {
                  label: t("team6.welcome.sayHi"),
                  onPress: () => greet(join, t("chat.failure.connection")),
                }
              : undefined,
        }),
      );
    return [...planLines, ...joinLines];
  }, [plans, team, t, joins, blocked, writable, tick]);
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
