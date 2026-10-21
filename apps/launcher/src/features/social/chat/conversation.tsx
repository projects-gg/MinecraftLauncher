// A conversation: its messages from this computer's history, newest at the bottom (older pages
// come when the player scrolls up, keeping their place), day lines, "Yeni mesajlar" over what was
// unread when it opened, "En yeni mesajlar" when new ones come while the player reads back, and the
// composer. Only messages that land while the conversation is open rise in; a deleted one fades and
// the rest glide. The conversation is read once it is on screen at its newest message.
import { ArrowDown } from "@phosphor-icons/react";
import { toast } from "@pl/ui";
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
import { friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { ChatMessage, SocialProfile } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { enter, glide, leave, useSocialMotion } from "../motion";
import { useSocialUi } from "../store";
import { useChat } from "./chat-store";
import { Composer, type ComposerState } from "./composer";
import { newDay, sameGroup } from "./conversations";
import { MessageRow, type RowActions } from "./message-row";

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

export interface ConversationProps {
  conv: string;
  crew: boolean;
  /** Who wrote what: the player's friends and crewmates by id. */
  people: (id: string) => Pick<SocialProfile, "name" | "avatar" | "avatarUrl">;
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
}: ConversationProps) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const thread = useChat((s) => s.threads[conv]);
  const edits = useChat((s) => s.edits);
  const hidden = useChat((s) => s.hidden);
  const load = useChat((s) => s.load);
  const loadOlder = useChat((s) => s.loadOlder);
  const setHidden = useChat((s) => s.setHidden);
  const setDraft = useChat((s) => s.setDraft);
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  const [away, setAway] = useState(false);
  const [newer, setNewer] = useState(false);
  const [editing, setEditing] = useState<ChatMessage | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const open = useSocialUi((s) => s.open);

  useEffect(() => {
    void load(conv);
  }, [conv, load]);

  // Edit and delete windows close while the conversation is open: once a minute is enough.
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const messages = useMemo(
    () => (thread?.messages ?? []).filter((m) => !hidden[m.key]),
    [thread?.messages, hidden],
  );

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
    const theirs = messages.filter((m) => !m.mine);
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
    const mark = () => {
      if (document.hasFocus()) void api.social.chatRead(conv);
    };
    const timer = window.setTimeout(mark, 250);
    window.addEventListener("focus", mark);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("focus", mark);
    };
  }, [onScreen, open, unread, away, thread?.loaded, conv]);

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
        // It fades at once; it comes back if the service refuses.
        setHidden(message.key, true);
        try {
          await api.social.chatDelete(conv, message.id);
          toast.success(t("chat.actions.deleted"));
          return true;
        } catch (error) {
          setHidden(message.key, false);
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
      onRewrite: (message) => {
        setDraft(conv, message.text);
        void api.social.chatDiscard(message.key);
      },
    }),
    [conv, onReport, setDraft, setHidden, t],
  );

  const editLast = () => {
    const mine = [...messages].reverse().find((m) => m.mine && (m.editUntil ?? 0) > Date.now());
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
        <div ref={content} className="flex min-h-full flex-col justify-end">
          {thread?.loaded && !messages.length ? (
            <div className="flex flex-1 items-center justify-center px-6 py-10">{empty}</div>
          ) : null}
          <LayoutGroup>
            <ol role="log" aria-label={t("chat.composer.label")} className="flex flex-col">
              <AnimatePresence initial={false}>
                {messages.map((message, index) => {
                  const previous = messages[index - 1];
                  const next = messages[index + 1];
                  const day = newDay(previous, message);
                  const rising = fresh(message);
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
                      {day ? (
                        <p className="flex justify-center px-2 pt-3 pb-2">
                          <span className="text-[12px] font-medium text-ash-400">
                            {dayLabel(t, message.at, now)}
                          </span>
                        </p>
                      ) : null}
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
                        sender={people(message.from)}
                        place={{
                          crew,
                          first: day || !sameGroup(previous, message),
                          last: !next || !sameGroup(message, next),
                        }}
                        actions={actions}
                        nowMs={now}
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
        <Composer
          conv={conv}
          crew={crew}
          state={composer}
          editing={editing}
          notice={notice}
          onCancelEdit={() => setEditing(null)}
          onEditLast={editLast}
          onSent={() => toBottom(false)}
        />
      </div>
    </div>
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
