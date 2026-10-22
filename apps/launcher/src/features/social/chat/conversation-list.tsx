// "Mesajlar": the player's conversations, the newest first: a friend's picture with presence or a
// crew's mark, the name, the last message ("Sen: …" for the player's own), when, and how many are
// unread (gold). A conversation muted here carries a struck bell; one whose message did not go says
// so. A press opens it (a friend's on the chat page, a crew's on its crew page, "Sohbet"). Its
// right click (Shift+F10, the menu key) is the friend's own menu, or the crew's "Sohbeti aç" and
// "Ekibi aç" (FIX2). CHAT2: a sticker reads "Çıkartma: GG"; while someone writes there, the
// last line is a quiet "yazıyor…" (screen readers are not told). A friend's row carries the crew
// tag they wear before the name (TEAM6).
import { BellSlash, ChatsCircle, Prohibit, UsersThree, WarningCircle } from "@phosphor-icons/react";
import { ContextMenu, cn, EmptyState, type MenuItem } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import type { ReactElement } from "react";
import { useTranslation } from "react-i18next";
import type { Conversation, SocialTeam } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { BadgedName, badgeRowLabel, useShownBadge } from "../crew-badge";
import { openMenuFromKey } from "../menu-key";
import { enter, glide, leave, useSocialMotion } from "../motion";
import { PersonContextMenu, type PersonRef } from "../person-menu";
import { SocialAvatar } from "../social-face";
import { useSocialPrefs, useSocialUi } from "../store";
import { CrewMark } from "../team-emblem";
import { chatMutedNow, conversationFace, peerOf, tombstoneKind } from "./conversations";
import { useChatExtras } from "./extras";
import { stickerName } from "./preview";
import { onlineIn, useTyping, useTypingFeed, writersOf } from "./typing";

/** "14:05" today, "Dün", a weekday this week, else the date. */
function whenLabel(t: (key: string) => string, at: number, nowMs: number): string {
  const day = (ms: number) => new Date(new Date(ms).toDateString()).getTime();
  const diff = Math.round((day(nowMs) - day(at)) / 86_400_000);
  if (diff <= 0) return dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(new Date(at));
  if (diff === 1) return t("chat.day.yesterday");
  if (diff < 7) return dateTimeFormat({ weekday: "short" }).format(new Date(at));
  return dateTimeFormat({ day: "numeric", month: "short" }).format(new Date(at));
}

/** A crew conversation's right click: its chat (the row's press) and its crew page. */
function CrewRowMenu({
  conv,
  crew,
  children,
}: {
  conv: string;
  crew: SocialTeam;
  children: ReactElement;
}) {
  const { t } = useTranslation();
  const items: MenuItem[] = [
    {
      label: t("fix2.menu.crewChat"),
      icon: <ChatsCircle size={16} />,
      onSelect: () => useSocialUi.getState().openChat(conv, { from: "list" }),
    },
    {
      label: t("teams.open"),
      icon: <UsersThree size={16} />,
      onSelect: () => useSocialUi.getState().openTeam(undefined, crew.id),
    },
  ];
  return <ContextMenu items={items}>{children}</ContextMenu>;
}

/** The row's right click: the friend's menu (the same as in the list), or the crew's. */
function RowMenu({
  conversation,
  crew,
  person,
  children,
}: {
  conversation: Conversation;
  crew?: SocialTeam;
  person?: PersonRef;
  children: ReactElement;
}) {
  if (crew) {
    return (
      <CrewRowMenu conv={conversation.id} crew={crew}>
        {children}
      </CrewRowMenu>
    );
  }
  if (!person?.id) return children;
  return (
    <PersonContextMenu person={person} place={{ from: "list" }}>
      {children}
    </PersonContextMenu>
  );
}

function Row({ conversation, nowMs }: { conversation: Conversation; nowMs: number }) {
  const { t, i18n } = useTranslation();
  const extras = useChatExtras();
  // Someone writes there now (a writer who went offline stops at once).
  const writersHere = useTyping((s) => s.writers[conversation.id]);
  const friends = useSocialUi((s) => s.view.friends);
  const crews = useSocialUi((s) => s.view.crews);
  const writing =
    writersHere !== undefined &&
    writersOf({ [conversation.id]: writersHere }, conversation.id, onlineIn(friends, crews))
      .length > 0;
  const view = useSocialUi((s) => s.view);
  const openChat = useSocialUi((s) => s.openChat);
  const mutes = useSocialPrefs((s) => s.chatMutes);
  const face = conversationFace(conversation, view);
  const name = face.name ?? t("chat.list.someone");
  const muted =
    chatMutedNow(conversation.id, mutes, nowMs) ||
    Boolean(face.crew?.mute.muted && (face.crew.mute.muteUntil ?? Infinity) * 1000 > nowMs);
  const unread = conversation.unread > 0;
  const last = conversation.last;
  // The newest one was deleted for everyone: its tombstone's words, never its text (FIX2).
  const gone = last?.deletedBy !== undefined;
  const preview = last
    ? gone
      ? t(
          `fix2.tombstone.${tombstoneKind(last.deletedBy ?? "", last.from, view.me?.id, face.crew)}`,
        )
      : last.sticker
        ? last.mine
          ? t("chat2.stickers.previewMine", { name: stickerName(last.sticker, t, i18n.language) })
          : t("chat2.stickers.preview", { name: stickerName(last.sticker, t, i18n.language) })
        : last.mine
          ? t("chat.list.you", { text: last.text })
          : last.text
    : t("chat.list.noMessages");
  const typingNow = extras.typing && writing && !conversation.failed;
  const badge = useShownBadge(face.person?.badge);
  const label = [
    badgeRowLabel(name, badge, t),
    unread ? t("chat.list.unread", { count: conversation.unread }) : null,
    preview,
  ]
    .filter(Boolean)
    .join(", ");
  const peer = conversation.crew
    ? undefined
    : (conversation.peer ?? peerOf(conversation.id, view.me?.id ?? ""));
  return (
    <RowMenu
      conversation={conversation}
      crew={face.crew}
      person={face.person ?? (peer ? { id: peer, name } : undefined)}
    >
      <button
        type="button"
        aria-label={label}
        onClick={() => openChat(conversation.id, { from: "list" })}
        onKeyDown={(event) => {
          openMenuFromKey(event);
        }}
        className="group/conv flex w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] py-2 pr-2.5 pl-2 text-left outline-none transition-colors duration-150 hover:bg-white/[0.045] focus-visible:ring-2 focus-visible:ring-gold-300 active:bg-white/[0.06]"
      >
        {face.crew ? (
          <CrewMark look={face.crew} size={36} />
        ) : (
          <SocialAvatar
            source={face.person ?? { name, avatar: "discord" }}
            size={36}
            state={face.person?.state}
            dnd={face.person?.dnd}
          />
        )}
        <span className="min-w-0 flex-1">
          {/* A friend's conversation: the crew tag they wear before the name (TEAM6). */}
          <BadgedName
            name={name}
            badge={face.person?.badge}
            wearer={face.person?.id ?? ""}
            nameClassName={cn(
              "text-[14px]",
              unread ? "font-semibold text-bone-50" : "font-medium text-bone-100",
            )}
          >
            {muted ? <BellSlash size={12} className="shrink-0 text-ash-400" aria-hidden /> : null}
          </BadgedName>
          <span
            className={cn(
              "flex items-center gap-1 truncate text-[12.5px]",
              unread ? "text-bone-100" : "text-ash-400",
            )}
          >
            {conversation.failed ? (
              <WarningCircle
                size={13}
                weight="fill"
                className="shrink-0 text-flare-400"
                aria-hidden
              />
            ) : gone ? (
              <Prohibit size={12} weight="bold" className="shrink-0 text-ash-400" aria-hidden />
            ) : null}
            <span className="relative grid min-w-0 flex-1">
              <span
                className={cn(
                  "col-start-1 row-start-1 truncate transition-opacity duration-150",
                  typingNow && "opacity-0",
                )}
              >
                {conversation.failed ? t("chat.list.failed") : preview}
              </span>
              <span
                aria-hidden
                className={cn(
                  "col-start-1 row-start-1 truncate text-ash-400 transition-opacity duration-150",
                  typingNow ? "opacity-100" : "opacity-0",
                )}
              >
                {t("chat2.typing.list")}
              </span>
            </span>
          </span>
        </span>
        <span className="flex shrink-0 flex-col items-end gap-1">
          {last ? (
            <span
              className={cn(
                "text-[11.5px] tabular-nums",
                unread ? "text-gold-300" : "text-ash-400",
              )}
            >
              {whenLabel(t, last.at, nowMs)}
            </span>
          ) : null}
          <AnimatePresence initial={false}>
            {unread ? (
              <motion.span
                key="unread"
                aria-hidden
                className="chamfer flex h-[18px] min-w-[18px] items-center justify-center bg-gold-500 px-1 font-display text-[11px] font-semibold text-gold-ink tabular-nums [--cut:4px]"
                initial={{ opacity: 0, transform: "scale(0.8)" }}
                animate={{ opacity: 1, transform: "scale(1)", transition: enter(0.2) }}
                exit={{ opacity: 0, transform: "scale(0.8)", transition: leave(0.14) }}
              >
                {conversation.unread > 99 ? "99+" : conversation.unread}
              </motion.span>
            ) : null}
          </AnimatePresence>
        </span>
      </button>
    </RowMenu>
  );
}

export function ConversationList() {
  const { t } = useTranslation();
  useTypingFeed();
  const { layout } = useSocialMotion();
  const conversations = useSocialUi((s) => s.view.chat?.conversations ?? []);
  const now = Date.now();
  if (!conversations.length) {
    return (
      <div className="pt-10">
        <EmptyState
          icon={<ChatsCircle size={30} />}
          title={t("chat.list.empty")}
          body={t("chat.list.emptyBody")}
        />
      </div>
    );
  }
  return (
    <ul aria-label={t("chat.list.label")} className="flex flex-col pt-1">
      <AnimatePresence initial={false}>
        {conversations.map((conversation) => (
          <motion.li
            key={conversation.id}
            layout={layout ? "position" : false}
            transition={{ layout: glide }}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.16) }}
          >
            <Row conversation={conversation} nowMs={now} />
          </motion.li>
        ))}
      </AnimatePresence>
    </ul>
  );
}
