// A conversation: its messages from this computer's history, newest at the bottom (older pages
// come when the player scrolls up, keeping their place), day lines, "Yeni mesajlar" over what was
// unread when it opened, "En yeni mesajlar" when new ones come while the player reads back, and the
// composer. Only messages that land while the conversation is open rise in; a message deleted for
// everyone turns into its quiet tombstone in place, saying who deleted it (FIX2; the player's own
// deletion shows at once). The conversation is read once it is on screen at its newest message. A
// crew's conversation also shows quiet local lines at their time (its plans, TEAM5: "Ayşe
// Cumartesi 21:00 için plan yaptı"; who came in, TEAM6: "Ayşe ekibe katıldı" with "Selam ver");
// the service's chat holds nothing about them.
//
// CHAT2: "Ayşe yazıyor" floats over the composer; a friend's message counts as read after a
// second on screen with the window in front (their side may show "Görüldü"), and "Görüldü" shows
// under the player's last message the friend has seen. A quote's press scrolls to the message (an
// older page loads first when it is not shown yet) and lights it once. Reactions show at once and
// settle with the service's answer; a card is asked for when its message comes on screen.
import { ArrowDown, CalendarBlank, HandWaving } from "@phosphor-icons/react";
import { cn, toast } from "@pl/ui";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { useEmojiPrefs } from "@/features/emoji";
import { api } from "@/lib/api";
import type { ChatMessage } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { enter, glide, leave, useSocialMotion } from "../motion";
import type { PersonPlace } from "../person-menu";
import { useSocialUi } from "../store";
import { useChat } from "./chat-store";
import { Composer, type ComposerState } from "./composer";
import { crewOf, deletionOf, peerOf, readable, sameGroup, tombstoneKind } from "./conversations";
import { useChatExtras, useLinkRules } from "./extras";
import { MessageRow, type RowActions, type RowShared } from "./message-row";
import { quickReactions, reactVerdict, withMyReaction } from "./reactions";
import { type Quoted, quotedOf } from "./reply";
import { READ_AFTER_MS, seenKey } from "./seen";
import type { ChatPerson } from "./sender-name";
import { onlineIn, useTyping, useTypingFeed, writersOf } from "./typing";
import { TypingPill } from "./typing-pill";

/** Scrolled this close to the newest message counts as "at the bottom". */
const BOTTOM_SLACK = 56;
/** Older messages are asked for this close to the top. */
const TOP_SLACK = 160;
/** A message this much older than the newest shown still rises in (clocks differ a little). */
const FRESH_SLACK_MS = 5_000;

function dayLabel(t: (key: string) => string, at: number, nowMs: number): string {
  const day = (ms: number) => new Date(new Date(ms).toDateString()).getTime();
  const diff = Math.round((day(nowMs) - day(at)) / 86_400_000);
  if (diff === 0) return t("chat.day.today");
  if (diff === 1) return t("chat.day.yesterday");
  const sameYear = new Date(at).getFullYear() === new Date(nowMs).getFullYear();
  return dateTimeFormat(
    sameYear
      ? { weekday: "long", day: "numeric", month: "long" }
      : { day: "numeric", month: "long", year: "numeric" },
  ).format(new Date(at));
}

/** A quiet line of this computer's at its time among the messages (a crew plan, TEAM5; someone
 * who came into the crew, TEAM6). */
export interface ChatLine {
  key: string;
  /** Unix ms. */
  at: number;
  text: string;
  /** Its mark: a plan's calendar (the default) or a newcomer's waving hand. */
  kind?: "plan" | "welcome";
  /** One small action beside it while it may be pressed ("Selam ver"). */
  action?: { label: string; onPress: () => void };
}

type Item = { kind: "message"; message: ChatMessage } | { kind: "line"; line: ChatLine };

const itemAt = (item: Item) => (item.kind === "message" ? item.message.at : item.line.at);
const sameDay = (a: number, b: number) => new Date(a).toDateString() === new Date(b).toDateString();

export interface ConversationProps {
  conv: string;
  crew: boolean;
  /** Who wrote what: the player's friends and crewmates by id (with the tag they wear). */
  people: (id: string) => ChatPerson;
  composer: ComposerState;
  /** A quiet line over the composer (mailbox off, waiting for the connection). */
  notice: string | null;
  /** How many were unread when it opened (the line goes over the first of them). */
  unreadAtOpen: number;
  /** The conversation shows in the panel (its newest messages can be read). */
  onScreen: boolean;
  onReport: (message: ChatMessage) => void;
  /** What shows while there are no messages yet. */
  empty: ReactNode;
  /** Local lines among the messages, at their time (a crew's plans). */
  lines?: readonly ChatLine[];
}

export function Conversation({
  conv,
  crew,
  people,
  composer,
  notice,
  unreadAtOpen,
  onScreen,
  onReport,
  empty,
  lines,
}: ConversationProps) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const thread = useChat((s) => s.threads[conv]);
  const edits = useChat((s) => s.edits);
  const deleting = useChat((s) => s.deleting);
  const reacting = useChat((s) => s.reacting);
  const replyingKey = useChat((s) => s.replying[conv]);
  const setReplying = useChat((s) => s.setReplying);
  const startReacting = useChat((s) => s.startReacting);
  const endReacting = useChat((s) => s.endReacting);
  const extras = useChatExtras();
  const linkRules = useLinkRules();
  const recentEmoji = useEmojiPrefs((s) => s.recentEmoji);
  useTypingFeed();
  const load = useChat((s) => s.load);
  const loadOlder = useChat((s) => s.loadOlder);
  const setDeleting = useChat((s) => s.setDeleting);
  const setDraft = useChat((s) => s.setDraft);
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  const [away, setAway] = useState(false);
  const [newer, setNewer] = useState(false);
  const [editing, setEditing] = useState<ChatMessage | null>(null);
  const [focusRequest, setFocusRequest] = useState(0);
  const [now, setNow] = useState(() => Date.now());
  const open = useSocialUi((s) => s.open);
  const me = useSocialUi((s) => s.view.me?.id);
  const shareRead = useSocialUi((s) => s.view.me?.privacy.shareRead !== false);
  const peerRead = useSocialUi(
    (s) => s.view.chat?.conversations.find((c) => c.id === conv)?.peerRead,
  );
  const writable = composer.kind === "open";
  // Who is online now (a writer who went offline stops "yazıyor").
  const friends = useSocialUi((s) => s.view.friends);
  const crews = useSocialUi((s) => s.view.crews);
  // Whose deletion a crew chat's tombstone tells: its leader's or a moderator's.
  const team = useSocialUi((s) => {
    const id = crewOf(conv);
    return id ? s.view.crews.find((c) => c.id === id) : undefined;
  });
  // Where a sender's own menu opens (their face and name): the crew's chat, or the conversation
  // with them on screen (FIX2).
  const personPlace = useMemo<PersonPlace>(
    () => (crew ? { crew: team, from: "team" } : { inConversation: true, from: "chat" }),
    [crew, team],
  );

  useEffect(() => {
    void load(conv);
  }, [conv, load]);

  // Edit and delete windows close while the conversation is open: once a minute is enough.
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const messages = useMemo(() => thread?.messages ?? [], [thread?.messages]);

  // A message being edited that turned into a tombstone (deleted on another device, or by a
  // moderator) ends the edit.
  useEffect(() => {
    if (!editing) return;
    const current = messages.find((m) => m.key === editing.key);
    if (!current || current.deleted || deleting[editing.key]) setEditing(null);
  }, [editing, messages, deleting]);
  // The lines join the messages at their time; one older than the first message shown waits for
  // the older page it belongs to.
  const openedAt = useRef(Date.now());
  const items = useMemo<Item[]>(() => {
    const out: Item[] = messages.map((message) => ({ kind: "message", message }));
    if (!lines?.length || !thread?.loaded) return out;
    const first = messages[0]?.at ?? Number.NEGATIVE_INFINITY;
    for (const line of lines) {
      if (thread.more && line.at < first) continue;
      out.push({ kind: "line", line });
    }
    return out.sort((a, b) => itemAt(a) - itemAt(b));
  }, [messages, lines, thread?.loaded, thread?.more]);

  // Only messages that come while the conversation is open rise in: the page it opens with, and
  // older pages (older than anything shown), are simply there.
  const known = useRef<{ keys: Set<string>; newest: number } | null>(null);
  const fresh = (message: ChatMessage) =>
    known.current !== null &&
    !known.current.keys.has(message.key) &&
    message.at >= known.current.newest - FRESH_SLACK_MS;
  useEffect(() => {
    if (!thread?.loaded) return;
    known.current ??= { keys: new Set(), newest: 0 };
    for (const message of messages) {
      known.current.keys.add(message.key);
      known.current.newest = Math.max(known.current.newest, message.at);
    }
  }, [thread?.loaded, messages]);

  // "Yeni mesajlar" goes over the first message that was unread when the conversation opened.
  const [dividerKey, setDividerKey] = useState<string | null>(null);
  const dividerSet = useRef(false);
  useEffect(() => {
    if (dividerSet.current || !thread?.loaded) return;
    dividerSet.current = true;
    if (unreadAtOpen <= 0) return;
    // Tombstones are never unread.
    const theirs = messages.filter((m) => !m.mine && readable(m));
    const first = theirs[Math.max(0, theirs.length - unreadAtOpen)];
    setDividerKey(first?.key ?? null);
  }, [thread?.loaded, messages, unreadAtOpen]);

  // Kept at the newest message while the player is there; an older page keeps the place read.
  const lastKey = messages.at(-1)?.key;
  const firstKey = messages[0]?.key;
  const before = useRef<{ first?: string; height: number; top: number }>({ height: 0, top: 0 });
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the messages change
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const previous = before.current;
    if (previous.first && firstKey !== previous.first && element.scrollHeight > previous.height) {
      // An older page came in above: stay on the same messages.
      element.scrollTop = previous.top + (element.scrollHeight - previous.height);
    } else if (atBottom.current) {
      element.scrollTop = element.scrollHeight;
    }
    before.current = { first: firstKey, height: element.scrollHeight, top: element.scrollTop };
  }, [firstKey, lastKey, messages.length]);

  // The composer growing, the panel resizing or the messages growing (a day line, "Yeni mesajlar",
  // a longer edit) keep the newest message in view.
  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      if (atBottom.current) element.scrollTop = element.scrollHeight;
    });
    observer.observe(element);
    if (content.current) observer.observe(content.current);
    return () => observer.disconnect();
  }, []);

  // A new message while reading back: say so instead of jumping.
  const seenLast = useRef(lastKey);
  useEffect(() => {
    if (lastKey && lastKey !== seenLast.current && !atBottom.current) setNewer(true);
    seenLast.current = lastKey;
  }, [lastKey]);

  // Read once on screen at the newest message (the panel open and the window in front).
  const unread = useSocialUi(
    (s) => s.view.chat?.conversations.find((c) => c.id === conv)?.unread ?? 0,
  );
  useEffect(() => {
    if (!onScreen || !open || unread === 0 || away || !thread?.loaded) return;
    const after = extras.reads ? READ_AFTER_MS : 250;
    const mark = () => {
      if (document.hasFocus()) void api.social.chatRead(conv);
    };
    let timer = window.setTimeout(mark, after);
    // The window came to front: counted from now (a quick look in passing is not a read).
    const again = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(mark, after);
    };
    window.addEventListener("focus", again);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("focus", again);
    };
  }, [onScreen, open, unread, away, thread?.loaded, conv, extras.reads]);

  const onScroll = () => {
    const element = scroller.current;
    if (!element) return;
    const bottom = element.scrollHeight - element.scrollTop - element.clientHeight < BOTTOM_SLACK;
    atBottom.current = bottom;
    if (bottom === away) setAway(!bottom);
    if (bottom && newer) setNewer(false);
    if (element.scrollTop < TOP_SLACK && thread?.more && !thread.older) {
      before.current = { first: firstKey, height: element.scrollHeight, top: element.scrollTop };
      void loadOlder(conv);
    }
    before.current.top = element.scrollTop;
  };

  const [flash, setFlash] = useState<{ key: string; n: number } | null>(null);
  const jumping = useRef<{ key: string; pages: number } | null>(null);
  const jumpTo = useCallback(
    (key: string) => {
      const element = scroller.current?.querySelector<HTMLElement>(
        `[data-key="${CSS.escape(key)}"]`,
      );
      if (element) {
        jumping.current = null;
        element.scrollIntoView({ block: "center", behavior: reduce ? "auto" : "smooth" });
        setFlash((f) => ({ key, n: (f?.n ?? 0) + 1 }));
        return;
      }
      // Not shown yet: an older page (at most ten of them) and try again.
      const state = useChat.getState().threads[conv];
      const pages = jumping.current?.key === key ? jumping.current.pages : 0;
      if (!state?.more || pages >= 10) return;
      jumping.current = { key, pages: pages + 1 };
      void loadOlder(conv);
    },
    [conv, loadOlder, reduce],
  );
  // biome-ignore lint/correctness/useExhaustiveDependencies: tries again when a page came
  useEffect(() => {
    const waiting = jumping.current;
    if (waiting && !thread?.older) jumpTo(waiting.key);
  }, [messages, thread?.older, jumpTo]);

  const toBottom = useCallback(
    (smooth: boolean) => {
      const element = scroller.current;
      if (!element) return;
      atBottom.current = true;
      setAway(false);
      setNewer(false);
      element.scrollTo({
        top: element.scrollHeight,
        behavior: smooth && !reduce ? "smooth" : "auto",
      });
    },
    [reduce],
  );

  const actions = useMemo<RowActions>(
    () => ({
      onEdit: (message) => setEditing(message),
      onDelete: async (message) => {
        if (message.id === undefined) return false;
        // Its tombstone shows at once (the bubble says the player deleted it, no toast); the
        // message comes back if the service refuses.
        setDeleting(message.key, true);
        try {
          await api.social.chatDelete(conv, message.id);
          announce(t("chat.actions.deleted"));
          return true;
        } catch (error) {
          setDeleting(message.key, false);
          toast.error(friendlyError(error, t("chat.failure.connection")).message);
          return false;
        }
      },
      onReport,
      onRetry: (message) => {
        void api.social.chatRetry(message.key).catch((error) => {
          toast.error(friendlyError(error, t("chat.failure.connection")).message);
        });
      },
      onDiscard: (message) => void api.social.chatDiscard(message.key),
      onReply: (message) => {
        setEditing(null);
        setReplying(conv, message.key);
        setFocusRequest((n) => n + 1);
      },
      onReact: (message, emoji, on) => {
        if (message.id === undefined || !me || !extras.reactions || !writable) return;
        const shown = useChat.getState().reacting[message.key] ?? message.reactions;
        if (on) {
          const verdict = reactVerdict(shown, emoji);
          if (verdict === "mine" || verdict === "kinds" || verdict === "refused") {
            toast(t(`chat2.reactions.${verdict === "mine" ? "full" : verdict}`));
            return;
          }
          useEmojiPrefs.getState().usedEmoji(emoji);
        }
        // Shown at once; once every change on it is answered the message's own reactions show
        // (the service's, or the old ones when it refused).
        startReacting(message.key, withMyReaction(shown, emoji, me, on));
        void api.social
          .chatReact(conv, message.id, emoji, on)
          .catch((error) => toast.error(friendlyError(error, t("chat.failure.connection")).message))
          .finally(() => endReacting(message.key));
      },
      onJump: (key) => jumpTo(key),
      onUnfurl: (message) =>
        api.social.chatUnfurl(conv, message.key).then(
          () => true,
          () => false,
        ),
      // The text goes back to the field to be written another way: the caret waits at its end.
      onRewrite: (message) => {
        setEditing(null);
        setDraft(conv, message.text);
        setFocusRequest((n) => n + 1);
        void api.social.chatDiscard(message.key);
      },
    }),
    [
      conv,
      onReport,
      setDraft,
      setDeleting,
      setReplying,
      startReacting,
      endReacting,
      jumpTo,
      me,
      t,
      extras.reactions,
      writable,
    ],
  );

  const shared = useMemo<RowShared>(
    () => ({
      conv,
      extras,
      links: linkRules,
      me,
      writable,
      quick: quickReactions(recentEmoji),
      people,
    }),
    [conv, extras, linkRules, me, writable, recentEmoji, people],
  );
  const byId = useMemo(() => {
    const map = new Map<number, ChatMessage>();
    for (const message of messages) if (message.id !== undefined) map.set(message.id, message);
    return map;
  }, [messages]);
  const quotes = useMemo(() => {
    const map = new Map<string, Quoted>();
    const nameOf = (id: string) => (id === me ? t("chat2.reply.you") : people(id).name);
    for (const message of messages) {
      if (message.reply) map.set(message.key, quotedOf(message.reply, byId, nameOf));
    }
    return map;
  }, [messages, byId, me, people, t]);
  const seen = !crew && extras.reads && shareRead ? seenKey(messages, peerRead) : null;
  const writersHere = useTyping((s) => s.writers[conv]);
  const online = onlineIn(friends, crews);
  const writers = extras.typing
    ? writersOf({ [conv]: writersHere ?? {} }, conv, online).filter((id) => id !== me)
    : [];
  // "Yazıyor…" goes only while someone else there is online.
  const crewId = crewOf(conv);
  const othersOnline = crewId
    ? crews.some((c) => c.id === crewId && c.members.some((m) => !m.me && m.state !== "offline"))
    : friends.some((f) => f.id === peerOf(conv, me ?? "") && f.state !== "offline");
  // A reply waits only while replies are on (Studio may switch them off meanwhile).
  const replyTo =
    extras.replies && replyingKey ? messages.find((m) => m.key === replyingKey) : undefined;
  useEffect(() => {
    if (!extras.replies && replyingKey) setReplying(conv, null);
  }, [extras.replies, replyingKey, conv, setReplying]);
  const replyQuote = useMemo<Quoted | null>(() => {
    if (!replyTo) return null;
    return {
      name: replyTo.mine ? t("chat2.reply.you") : people(replyTo.from).name,
      text: replyTo.text,
      sticker: replyTo.sticker,
      deleted: Boolean(replyTo.deleted),
      missing: false,
      key: replyTo.key,
    };
  }, [replyTo, people, t]);

  const editLast = () => {
    const mine = [...messages]
      .reverse()
      .find((m) => m.mine && readable(m) && !deleting[m.key] && (m.editUntil ?? 0) > Date.now());
    if (mine) setEditing(mine);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <motion.div
        ref={scroller}
        layoutScroll
        onScroll={onScroll}
        className="relative min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain pt-2 [scrollbar-gutter:stable]"
      >
        {/* With "yazıyor…" on, the list keeps a line free under its newest message for it. */}
        <div
          ref={content}
          className={cn("flex min-h-full flex-col justify-end", extras.typing && "pb-7")}
        >
          {thread?.loaded && !items.length ? (
            <div className="flex flex-1 items-center justify-center px-6 py-10">{empty}</div>
          ) : null}
          <LayoutGroup>
            <ol role="log" aria-label={t("chat.composer.label")} className="flex flex-col">
              <AnimatePresence initial={false}>
                {items.map((item, index) => {
                  const before = items[index - 1];
                  const after = items[index + 1];
                  const day = !before || !sameDay(itemAt(before), itemAt(item));
                  const dayLine = day ? (
                    <p className="flex justify-center px-2 pt-3 pb-2">
                      <span className="text-[12px] font-medium text-ash-400">
                        {dayLabel(t, itemAt(item), now)}
                      </span>
                    </p>
                  ) : null;
                  if (item.kind === "line") {
                    const { line } = item;
                    return (
                      <motion.li
                        key={line.key}
                        layout={layout ? "position" : false}
                        transition={{ layout: glide }}
                        initial={line.at >= openedAt.current ? { opacity: 0 } : false}
                        animate={{ opacity: 1, transition: enter(0.24) }}
                        exit={{ opacity: 0, transition: leave(0.16) }}
                      >
                        {dayLine}
                        <div
                          className={cn(
                            "flex items-center justify-center gap-1.5 px-6 text-center text-[12px] text-ash-400",
                            // Its "Selam ver" comes and goes inside the line's own height.
                            line.kind === "welcome" ? "min-h-8 py-1" : "py-1.5",
                          )}
                        >
                          {line.kind === "welcome" ? (
                            <HandWaving
                              size={13}
                              weight="duotone"
                              className="shrink-0 text-gold-300"
                              aria-hidden
                            />
                          ) : (
                            <CalendarBlank
                              size={13}
                              weight="duotone"
                              className="shrink-0 text-gold-300"
                              aria-hidden
                            />
                          )}
                          <span>{line.text}</span>
                          <AnimatePresence initial={false}>
                            {line.action ? (
                              <motion.span
                                key="action"
                                className="ml-1 flex"
                                initial={{ opacity: 0 }}
                                animate={{ opacity: 1, transition: enter(0.2) }}
                                exit={{ opacity: 0, transition: leave(0.16) }}
                              >
                                <LineAction action={line.action} />
                              </motion.span>
                            ) : null}
                          </AnimatePresence>
                        </div>
                      </motion.li>
                    );
                  }
                  const { message } = item;
                  // A line between two messages starts a new group.
                  const previous = before?.kind === "message" ? before.message : undefined;
                  const next = after?.kind === "message" ? after.message : undefined;
                  const rising = fresh(message);
                  const deletion = deletionOf(message, deleting, me);
                  return (
                    <motion.li
                      key={message.key}
                      layout={layout ? "position" : false}
                      transition={{ layout: glide }}
                      initial={
                        rising
                          ? {
                              opacity: 0,
                              // Under reduced motion a fade only: the same transform as the end
                              // ("none" would let motion start the scale at 0).
                              transform: reduce
                                ? "translateY(0px) scale(1)"
                                : message.mine
                                  ? "translateY(10px) scale(0.98)"
                                  : "translateY(6px)",
                            }
                          : false
                      }
                      animate={{
                        opacity: 1,
                        transform: "translateY(0px) scale(1)",
                        transition: enter(message.mine ? 0.24 : 0.22),
                      }}
                      exit={{ opacity: 0, transition: leave(0.16) }}
                      style={{ transformOrigin: message.mine ? "bottom right" : "bottom left" }}
                    >
                      {dayLine}
                      {message.key === dividerKey ? (
                        <p className="mx-2 my-2 flex items-center gap-2.5 text-[11.5px] font-semibold text-gold-300">
                          <span aria-hidden className="h-px flex-1 bg-gold-400/35" />
                          {t("chat.divider")}
                          <span aria-hidden className="h-px flex-1 bg-gold-400/35" />
                        </p>
                      ) : null}
                      <MessageRow
                        message={message}
                        text={edits[message.key] ?? message.text}
                        tombstone={
                          deletion
                            ? t(
                                `fix2.tombstone.${tombstoneKind(deletion.by, message.from, me, team)}`,
                              )
                            : null
                        }
                        sender={people(message.from)}
                        personPlace={personPlace}
                        place={{
                          crew,
                          first: day || !sameGroup(previous, message),
                          last: !next || !sameGroup(message, next),
                        }}
                        actions={actions}
                        nowMs={now}
                        shared={shared}
                        quoted={quotes.get(message.key) ?? null}
                        reactions={reacting[message.key] ?? message.reactions}
                        seen={message.key === seen}
                        fresh={rising}
                        flash={flash?.key === message.key ? flash.n : 0}
                      />
                    </motion.li>
                  );
                })}
              </AnimatePresence>
            </ol>
          </LayoutGroup>
        </div>
      </motion.div>
      <div className="relative">
        <AnimatePresence initial={false}>
          {away && newer ? (
            <motion.button
              key="newest"
              type="button"
              onClick={() => toBottom(true)}
              className="absolute -top-11 left-1/2 z-10 flex h-8 -translate-x-1/2 cursor-pointer items-center gap-1.5 rounded-md bg-obsidian-800/95 px-3 text-[12.5px] font-medium text-bone-100 shadow-[0_10px_30px_rgb(34_6_6/0.5)] ring-1 ring-gold-400/30 outline-none transition-[background-color,transform,scale] duration-150 hover:bg-obsidian-700 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97]"
              initial={{ opacity: 0, y: reduce ? 0 : 6 }}
              animate={{ opacity: 1, y: 0, transition: enter(0.22) }}
              exit={{ opacity: 0, transition: leave(0.14) }}
            >
              <ArrowDown size={14} weight="bold" className="text-gold-300" aria-hidden />
              {t("chat.actions.newest")}
            </motion.button>
          ) : null}
        </AnimatePresence>
        <TypingPill writers={writers} people={people} />
        <Composer
          conv={conv}
          crew={crew}
          state={composer}
          editing={editing}
          notice={notice}
          focusRequest={focusRequest}
          onCancelEdit={() => setEditing(null)}
          onEditLast={editLast}
          onSent={() => toBottom(false)}
          replyTo={replyTo?.id !== undefined ? replyTo : null}
          replyQuote={replyQuote}
          onCancelReply={() => setReplying(conv, null)}
          othersOnline={othersOnline}
        />
      </div>
    </div>
  );
}

/** A line's one action ("Selam ver"): small, chamfered, gold words on the quiet line. */
function LineAction({ action }: { action: NonNullable<ChatLine["action"]> }) {
  return (
    <button
      type="button"
      onClick={action.onPress}
      className="group/line relative inline-flex h-6 shrink-0 cursor-pointer items-center outline-none transition-[transform,scale] duration-150 ease-out active:scale-[0.97]"
    >
      <span
        aria-hidden
        className="chamfer-ring absolute bg-gold-300 opacity-0 transition-opacity duration-150 [--cut:5px] group-focus-visible/line:opacity-100"
      />
      <span className="chamfer relative inline-flex h-6 items-center bg-gold-500/12 px-2 text-[12px] font-semibold text-gold-200 transition-colors duration-150 [--cut:5px] group-hover/line:bg-gold-500/20">
        {action.label}
      </span>
    </button>
  );
}

/** How many messages of a conversation were unread when it came on screen. */
export function useUnreadAtOpen(conv: string | null): number {
  const unread = useSocialUi(
    (s) => s.view.chat?.conversations.find((c) => c.id === conv)?.unread ?? 0,
  );
  const [at, setAt] = useState({ conv, count: unread });
  if (at.conv !== conv) setAt({ conv, count: unread });
  return at.conv === conv ? at.count : unread;
}
