// A crew's chat: the crew's conversation, with names over others' groups and the quick replies
// over the composer. Its leader and moderators may delete any message (the service decides; the
// menu offers it where the view says so). The crew's plans show as quiet lines when they were made
// (TEAM5), from the plans this computer has; so do newcomers while this launcher was connected
// (TEAM6 "Hoş geldin": "Ayşe ekibe katıldı" with "Selam ver", which sends the "Selam" sticker, or
// "Hoş geldin! 👋" without stickers, once and for ten minutes; "Ekibe katıldın" for the player).
//
// CREW-UI: it fills the panel's chat page, as a friend's conversation does ("Sohbete Git" on the
// crew page, a row in "Mesajlar", a notice). Its header: back, the crew's mark and name, how many
// are online; the search (SOC-NEXT) and the crew's bell. A right click there is the crew's own menu.
import { BellSlash, CaretLeft, MagnifyingGlass } from "@phosphor-icons/react";
import { ContentIcon, ContextMenu, IconButton, toast } from "@pl/ui";
import { Dialog as RadixDialog } from "radix-ui";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { ChatMessage, CrewJoin, PinLine, SocialTeam } from "@/lib/api/social-types";
import { currentLanguage } from "@/lib/i18n";
import { canGreet, nextGreetEnd } from "../badges";
import { CrewNotifyButton, useCrewMute } from "../crew-notify";
import { planWhen, withPlanNote } from "../crew-plans";
import { useSocNext } from "../next/flag";
import { openChatSearch } from "../next/search-bar";
import { usePinsOn } from "../note/pins";
import { useSocialPrefs, useSocialUi } from "../store";
import { CrewMark } from "../team-emblem";
import { useCrewItems } from "../team-section";
import { useComposerNotice, useComposerState, usePeople } from "./chat-page";
import { type ChatLine, Conversation, useUnreadAtOpen } from "./conversation";
import { chatMutedNow, crewConversation, crewOf } from "./conversations";
import { noteBackToCrew } from "./crew-chat-entry";
import { messageLine } from "./preview";

/** The crew whose chat the chat page shows (undefined for a friend's, or a crew the player left). */
export function useChatCrew(): SocialTeam | undefined {
  return useSocialUi((s) => {
    const id = s.chatConv ? crewOf(s.chatConv) : null;
    return id ? s.view.crews.find((crew) => crew.id === id) : undefined;
  });
}

export function CrewChatHeader({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const back = useSocialUi((s) => s.goTo);
  const from = useSocialUi((s) => s.chatFrom);
  const conv = crewConversation(team.id);
  const mutedHere = useSocialPrefs((s) => chatMutedNow(conv, s.chatMutes, Date.now()));
  const crewMute = useCrewMute(team.id);
  const next = useSocNext();
  // The crew's menu, as wherever it shows (FIX2); this is its chat already.
  const items = useCrewItems(team, { inChat: true });
  const online = team.members.filter((m) => !m.me && !m.blocked && m.state !== "offline").length;
  // The bell beside says the crew's own silence; the name's struck bell is for this conversation
  // muted on this computer alone (the conversation's menu in "Mesajlar").
  const bellMuted = crewMute.muted || (Boolean(crewMute.target) && crewMute.allMuted);
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      <IconButton
        label={t("social.actions.back")}
        icon={<CaretLeft size={18} weight="bold" />}
        className="-ml-2"
        onClick={() => {
          if (from === "team") noteBackToCrew(conv);
          back(from === "chat" ? "list" : from);
        }}
      />
      <ContextMenu items={items}>
        <div className="flex min-w-0 flex-1 items-center gap-2.5">
          <CrewMark look={team} size={32} />
          <div className="min-w-0 flex-1">
            <RadixDialog.Title className="flex min-w-0 items-center gap-1.5 text-[15px] leading-tight font-semibold text-bone-50">
              <span className="truncate">{team.name}</span>
              {mutedHere && !bellMuted ? (
                <BellSlash
                  size={13}
                  className="shrink-0 text-ash-400"
                  aria-label={t("chat.list.muted")}
                />
              ) : null}
            </RadixDialog.Title>
            <p className="truncate text-[12px] text-ash-300">
              {online ? t("teams.online", { count: online }) : t("social.account.noneOnline")}
            </p>
          </div>
        </div>
      </ContextMenu>
      {next ? (
        <IconButton
          label={t("socNext.search.open")}
          icon={<MagnifyingGlass size={18} />}
          className="shrink-0"
          onClick={() => openChatSearch(conv)}
        />
      ) : null}
      <CrewNotifyButton crew={team.id} />
    </div>
  );
}

/** The chat page with a crew's chat on it. */
export function CrewChatPage() {
  const team = useChatCrew();
  // A crew's chat opened over another one starts afresh: its own unread line, place and newest
  // message.
  return team ? <CrewChat key={team.id} team={team} onScreen /> : null;
}

const NO_JOINS: CrewJoin[] = [];
const NO_PIN_LINES: PinLine[] = [];

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
  // SOC-NOTE: "Kaan_07 bir mesajı sabitledi." of this run.
  const pinsOn = usePinsOn();
  const allPinLines = useSocialUi((s) => s.view.chat?.socNote?.pinLines ?? NO_PIN_LINES);
  const me = useSocialUi((s) => s.view.me?.id);
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
    const pinLines = pinsOn
      ? allPinLines
          .filter((line) => line.conv === conv && !shut(line.by))
          .map(
            (line): ChatLine => ({
              key: `pin:${line.id}:${line.at}`,
              at: line.at,
              kind: "pin",
              text:
                line.by === me
                  ? t("socNote.pins.pinnedByYou")
                  : t("socNote.pins.pinned", { name: people(line.by).name }),
            }),
          )
      : [];
    return [...planLines, ...joinLines, ...pinLines];
  }, [plans, team, t, joins, blocked, writable, tick, pinsOn, allPinLines, conv, me, people]);
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
          <ContentIcon kind="chat" quiet />
          <p className="text-[13px] text-ash-400">{t("chat.list.noMessages")}</p>
        </div>
      }
    />
  );
}
