// A direct conversation in the friends panel: the header carries the friend (picture, presence,
// the crew tag they wear, TEAM6), "Discord'dan mesajlaş" as Discord's own mark alone (its words the
// tooltip; the app when it is installed) and the conversation's menu (mute here, clear the history
// here, report); the body is the conversation.
import { BellSlash, Broom, CaretLeft, DotsThree, Flag, SpeakerHigh } from "@phosphor-icons/react";
import { Button, ContentIcon, cn, Dialog, IconButton, Menu, type MenuItem, toast } from "@pl/ui";
import { Dialog as RadixDialog } from "radix-ui";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import discordSymbol from "@/assets/brand/discord-symbol-white.png";
import { api } from "@/lib/api";
import type { ChatMessage, Friend } from "@/lib/api/social-types";
import { currentLanguage } from "@/lib/i18n";
import { BadgeChip, useShownBadge } from "../crew-badge";
import { untilText } from "../crew-notify";
import { openInDiscord } from "../discord-link";
import { MUTE_PRESETS, muteUntilFor } from "../mute";
import { PersonMark } from "../person-mark";
import { PersonContextMenu } from "../person-menu";
import { PresenceText } from "../presence-text";
import { SocialAvatar } from "../social-face";
import { useSocialPrefs, useSocialUi } from "../store";
import type { ComposerState } from "./composer";
import { Conversation, useUnreadAtOpen } from "./conversation";
import { chatMutedNow, peerOf } from "./conversations";
import { messageLine } from "./preview";
import type { ChatPerson } from "./sender-name";

/** Discord's mark in its own colour, drawn from the official symbol as a mask. */
export function DiscordGlyph({ height = 11, className }: { height?: number; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn("inline-block shrink-0 bg-[#5865F2]", className)}
      style={{
        width: Math.round(height * 1.32),
        height,
        maskImage: `url(${discordSymbol})`,
        maskSize: "contain",
        maskRepeat: "no-repeat",
        maskPosition: "center",
      }}
    />
  );
}

/** Who wrote a message, as the lists know them (the player, friends, crewmates), with the crew tag
 * they wear (TEAM6). */
export function usePeople(): (id: string) => ChatPerson {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const friends = useSocialUi((s) => s.view.friends);
  const crews = useSocialUi((s) => s.view.crews);
  // One function while the lists stay the same (the rows it reaches are memoized).
  return useCallback(
    (id: string) => {
      if (me && id === me.id) return me;
      const friend = friends.find((f) => f.id === id);
      if (friend) return friend;
      for (const crew of crews) {
        const member = crew.members.find((m) => m.id === id);
        if (member) return member;
      }
      return { name: t("chat.list.someone"), avatar: "discord" };
    },
    [me, friends, crews, t],
  );
}

/** A sentence that starts with a time ("yarın 14:30'a kadar …") starts with a capital. */
function sentence(text: string): string {
  const locale = currentLanguage() === "tr" ? "tr-TR" : "en-US";
  return text.charAt(0).toLocaleUpperCase(locale) + text.slice(1);
}

/** Why the player cannot write in a conversation now, or open. */
export function useComposerState(conv: string, writable: boolean, crew: boolean): ComposerState {
  const { t } = useTranslation();
  const chat = useSocialUi((s) => s.view.chat);
  const mutedUntil = chat?.mutedUntil;
  // A moderator's mute ends by itself: the composer opens then (one timer, no polling).
  const [, setEnded] = useState(0);
  useEffect(() => {
    if (!mutedUntil) return;
    const wait = mutedUntil * 1000 - Date.now();
    if (wait <= 0) return;
    const timer = window.setTimeout(
      () => setEnded((n) => n + 1),
      Math.min(wait + 500, 2_000_000_000),
    );
    return () => window.clearTimeout(timer);
  }, [mutedUntil]);
  const now = Date.now();
  if (!chat?.on) return { kind: "closed", reason: t("chat.composer.off"), icon: "info" };
  if (chat.mutedUntil && chat.mutedUntil * 1000 > now) {
    return {
      kind: "closed",
      reason: sentence(
        t("chat.composer.muted", { until: untilText(t, chat.mutedUntil * 1000, now) }),
      ),
    };
  }
  if (!writable) {
    return {
      kind: "closed",
      reason: crew ? t("chat.composer.notInCrew") : t("chat.composer.notFriends"),
    };
  }
  void conv;
  return { kind: "open" };
}

/** The quiet line over the composer: messages reach only those online, or the connection waits. */
export function useComposerNotice(): string | null {
  const { t } = useTranslation();
  const mailbox = useSocialUi((s) => s.view.chat?.mailbox ?? true);
  const connection = useSocialUi((s) => s.view.connection);
  if (
    connection === "reconnecting" ||
    connection === "unavailable" ||
    connection === "connecting"
  ) {
    return t("chat.composer.waiting");
  }
  return mailbox ? null : t("chat.composer.mailbox");
}

/** The friend of the open direct conversation, as the lists know them. */
function useChatPeer(): { conv: string | null; peer: Friend | undefined; title?: string } {
  const conv = useSocialUi((s) => s.chatConv);
  const me = useSocialUi((s) => s.view.me?.id ?? "");
  const friends = useSocialUi((s) => s.view.friends);
  const title = useSocialUi(
    (s) => s.view.chat?.conversations.find((c) => c.id === s.chatConv)?.title,
  );
  const id = conv ? peerOf(conv, me) : null;
  return { conv, peer: friends.find((f) => f.id === id), title };
}

/** "Sessize al" on one conversation (this computer): the crew mute's choices. */
function muteItems(t: (key: string) => string, conv: string): MenuItem[] {
  const set = useSocialPrefs.getState().setChatMute;
  return MUTE_PRESETS.map((preset) => ({
    label: t(`teams.mute.presets.${preset}`),
    onSelect: () => {
      const until = muteUntilFor(preset, Date.now());
      set(conv, until === null ? null : until * 1000);
    },
  }));
}

export function ChatHeader() {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me?.id ?? "");
  const back = useSocialUi((s) => s.goTo);
  const from = useSocialUi((s) => s.chatFrom);
  const openReport = useSocialUi((s) => s.openReport);
  const { conv, peer, title } = useChatPeer();
  const mutes = useSocialPrefs((s) => s.chatMutes);
  const setMute = useSocialPrefs((s) => s.setChatMute);
  const [clearing, setClearing] = useState(false);
  const badge = useShownBadge(peer?.badge);
  if (!conv) return null;
  const muted = chatMutedNow(conv, mutes, Date.now());
  const name = peer?.name ?? title ?? t("chat.list.someone");
  const items: Array<MenuItem | null> = [
    muted
      ? {
          label: t("chat.actions.unmute"),
          icon: <SpeakerHigh size={16} />,
          onSelect: () => setMute(conv, undefined),
        }
      : {
          label: t("chat.actions.mute"),
          icon: <BellSlash size={16} />,
          onSelect: () => {},
          items: muteItems(t, conv),
        },
    {
      label: t("chat.actions.clear"),
      icon: <Broom size={16} />,
      onSelect: () => setClearing(true),
    },
    null,
    ...(peer
      ? [
          {
            label: t("reports.action"),
            icon: <Flag size={16} />,
            danger: true,
            onSelect: () =>
              openReport({
                target: { kind: "player", id: peer.id },
                name: peer.name,
                person: peer,
              }),
          },
        ]
      : []),
  ];
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      <IconButton
        label={t("social.actions.back")}
        icon={<CaretLeft size={18} weight="bold" />}
        className="-ml-2"
        onClick={() => back(from === "chat" ? "list" : from)}
      />
      {/* The friend's own menu on a right click (FIX2), as wherever they show. */}
      <PersonContextMenu
        person={peer ?? { id: peerOf(conv, me) ?? "", name }}
        place={{ inConversation: true, from: "chat" }}
      >
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <SocialAvatar
            source={peer ?? { name, avatar: "discord" }}
            size={32}
            state={peer?.state}
            dnd={peer?.dnd}
          />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-1.5">
              {/* The tag they wear (TEAM6): a button of its own, outside the title's words. */}
              {badge && peer ? <BadgeChip badge={badge} wearer={peer.id} mode="button" /> : null}
              <RadixDialog.Title className="flex min-w-0 items-center gap-1.5 truncate text-[15px] leading-tight font-semibold text-bone-50">
                <span className="flex min-w-0 items-center gap-1">
                  <span className="truncate">{name}</span>
                  {/* Their mark right of the name (SOC-VERIFY), in the title's words. */}
                  <PersonMark mark={peer?.mark} />
                </span>
                {muted ? (
                  <BellSlash
                    size={13}
                    className="shrink-0 text-ash-400"
                    aria-label={t("chat.list.muted")}
                  />
                ) : null}
              </RadixDialog.Title>
            </div>
            {peer ? (
              <p className="truncate text-[12px] text-ash-300">
                <PresenceText friend={peer} t={t} />
              </p>
            ) : null}
          </div>
        </div>
      </PersonContextMenu>
      {peer ? (
        // Discord's mark alone, its words the tooltip and the name (CHAT-UX: beside the presence
        // line the words cut it, "Başka sunucuda 1…").
        <IconButton
          label={t("chat.actions.discord")}
          icon={<DiscordGlyph height={14} />}
          className="shrink-0"
          onClick={() => void openInDiscord(peer)}
        />
      ) : null}
      <Menu
        items={items}
        trigger={
          <IconButton
            label={t("chat.actions.conversation")}
            icon={<DotsThree size={20} weight="bold" />}
          />
        }
      />
      <ClearDialog conv={conv} open={clearing} onOpenChange={setClearing} />
    </div>
  );
}

/** "Geçmişi temizle": asked once, in one sentence (it cannot be undone on this computer). */
export function ClearDialog({
  conv,
  open,
  onOpenChange,
}: {
  conv: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <ClearAsk
      open={open}
      onOpenChange={onOpenChange}
      onClear={() => {
        void api.social.chatClear(conv).then(() => toast.success(t("chat.actions.cleared")));
        onOpenChange(false);
      }}
    />
  );
}

function ClearAsk({
  open,
  onOpenChange,
  onClear,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onClear: () => void;
}) {
  const { t } = useTranslation();
  // The shared dialog: one sentence and two answers.
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("chat.actions.clear")}
      description={t("chat.actions.clearAsk")}
      closeLabel={t("common.close")}
      width={420}
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("chat.actions.cancel")}
          </Button>
          <Button variant="danger" onClick={onClear}>
            {t("chat.actions.clear")}
          </Button>
        </>
      }
    />
  );
}

export function ChatPage() {
  const { t, i18n } = useTranslation();
  const { conv, peer } = useChatPeer();
  const openReport = useSocialUi((s) => s.openReport);
  const writable = useSocialUi(
    (s) => s.view.chat?.conversations.find((c) => c.id === s.chatConv)?.writable ?? Boolean(peer),
  );
  const people = usePeople();
  const composer = useComposerState(conv ?? "", Boolean(peer) && writable, false);
  const notice = useComposerNotice();
  const unreadAtOpen = useUnreadAtOpen(conv);
  if (!conv) return null;
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
    // A conversation opened over another one (a toast's "Yanıtla") starts afresh: its own unread
    // line, place and newest message.
    <Conversation
      key={conv}
      conv={conv}
      crew={false}
      people={people}
      composer={composer}
      notice={notice}
      unreadAtOpen={unreadAtOpen}
      onScreen
      onReport={report}
      empty={
        // The crew's empty chat's quiet bubble over its line (design.md §6, CHAT-UX).
        <div className="flex flex-col items-center gap-2 text-center">
          <ContentIcon kind="chat" quiet />
          <p className="text-[13px] text-ash-400">
            {peer ? t("chat.list.noMessages") : t("chat.composer.notFriends")}
          </p>
        </div>
      }
    />
  );
}
