// The room's chat in the room panel (SQOL §1.3 "Oda sohbeti v1"): chat's own rows (message-row.tsx)
// and field, newest at the bottom, the day lines and groups chat has (CHAT-UX), "Yeni Mesajlar"
// over what came while the chat was out of sight, "Son Mesajlara İn" when something comes while the
// player reads back. Words with emoji, "Küp" stickers and links the rules let through (no cards:
// the link alone, an unknown site's one-line warning before it opens); the player's own message
// and, for the room's leader and moderators, anyone's may be taken away; others' may be reported;
// any may be copied. No reactions, edits, replies or "yazıyor" (SQOL §6). People the player blocks
// never show. Only what comes while the chat is on screen rises in; screen readers hear it from the
// room chat's host (host.tsx), gathered.
import { ArrowDown } from "@phosphor-icons/react";
import { ContentIcon, cn, scrollBehavior } from "@pl/ui";
import { AnimatePresence, LayoutGroup, MotionConfig, motion } from "motion/react";
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { dayLineText, sameLocalDay } from "@/features/social/chat/chat-time";
import { DayLine, LayoutCue, Unzoomed } from "@/features/social/chat/conversation";
import { sameGroup } from "@/features/social/chat/conversations";
import { NO_EXTRAS, useChatExtras, useLinkRules } from "@/features/social/chat/extras";
import { chatKeyHere } from "@/features/social/chat/focus";
import {
  MessageRow,
  type RowActions,
  type RowPlace,
  type RowShared,
} from "@/features/social/chat/message-row";
import type { ChatPerson } from "@/features/social/chat/sender-name";
import { useChatTextSize } from "@/features/social/chat/text-size";
import {
  DEFAULT_TEXT_SIZE,
  stepTextSize,
  type TextSize,
  WHEEL_START,
  wheelZoom,
  zoomKeyOf,
  zoomOf,
} from "@/features/social/chat/text-zoom";
import { TimeTip } from "@/features/social/chat/time-tip";
import { enter, glide, leave, useSocialMotion } from "@/features/social/motion";
import type { PersonPlace } from "@/features/social/person-menu";
import { useSocialUi } from "@/features/social/store";
import type { ChatMessage } from "@/lib/api/social-types";
import type { VoiceRoomView } from "@/lib/api/voice-types";
import { formatPercent } from "@/lib/format";
import { detectOs } from "@/lib/platform";
import { deleteRoomMessage, dropRoomMessage, reportRoomMessage, retryRoomMessage } from "./actions";
import { RoomComposer } from "./composer";
import { useFieldCue } from "./field-cue";
import { useRoomPeople } from "./people";
import { asChatMessage, type RoomTombstone, tombstoneOf } from "./rules";
import { useRoomChat } from "./store";

/** Scrolled this close to the newest message counts as "at the bottom" (chat's). */
const BOTTOM_SLACK = 56;
/** PageUp and PageDown scroll this share of a screen (chat's). */
const PAGE_SHARE = 0.85;
/** "Son Mesajlara İn" glides down at most this many screens; from farther it jumps. */
const GLIDE_SCREENS = 3;

/** A sender's own menu from their face or name: the room's people menu. */
const PERSON_PLACE: PersonPlace = { from: "list" };

const TOMBSTONE_KEYS: Record<RoomTombstone, string> = {
  mine: "fix2.tombstone.mine",
  sender: "fix2.tombstone.sender",
  mod: "roomChat.tombstone.mod",
  staff: "fix2.tombstone.staff",
};

/** Where the list shows its newest message at the bottom, from the layout (chat's `newestTop`). */
function newestTop(list: HTMLElement, content: HTMLElement | null): number {
  if (!content) return list.scrollHeight;
  return Math.max(0, content.offsetTop + content.offsetHeight - list.clientHeight);
}

interface RoomItemProps {
  message: ChatMessage;
  tombstone: string | null;
  sender: ChatPerson;
  first: boolean;
  last: boolean;
  dayText: string | null;
  divider: string | null;
  deleteOpen: boolean;
  rising: boolean;
  actions: RowActions;
  shared: RowShared;
  layout: boolean;
  reduce: boolean;
}

/** One message with what goes over it; it draws again only when what it shows changed. */
const RoomItem = memo(function RoomItem({
  message,
  tombstone,
  sender,
  first,
  last,
  dayText,
  divider,
  deleteOpen,
  rising,
  actions,
  shared,
  layout,
  reduce,
}: RoomItemProps) {
  const place = useMemo<RowPlace>(() => ({ crew: true, first, last }), [first, last]);
  return (
    <motion.li
      data-day-at={message.at}
      data-day-start={dayText ? "" : undefined}
      layout={layout ? "position" : false}
      transition={{ layout: glide }}
      initial={
        rising
          ? {
              opacity: 0,
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
      {dayText ? <DayLine text={dayText} /> : null}
      {divider ? (
        <p
          data-divider
          className="mx-2 my-2 flex items-center gap-2.5 text-[11.5px] font-semibold text-bone-100"
        >
          <span aria-hidden className="h-px flex-1 bg-ash-500/45" />
          {divider}
          <span aria-hidden className="h-px flex-1 bg-ash-500/45" />
        </p>
      ) : null}
      <MessageRow
        message={message}
        text={message.text}
        tombstone={tombstone}
        sender={sender}
        personPlace={PERSON_PLACE}
        place={place}
        actions={actions}
        editOpen={false}
        deleteOpen={deleteOpen}
        reactOpen={false}
        weekOpen={false}
        shared={shared}
        quoted={null}
        reactions={undefined}
        seen={false}
        fresh={rising}
        flash={0}
      />
    </motion.li>
  );
});

export function RoomChat({ room, visible }: { room: VoiceRoomView; visible: boolean }) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const all = useRoomChat((s) => s.msgs);
  const loaded = useRoomChat((s) => s.loaded);
  const dividerKey = useRoomChat((s) => s.divider);
  const blockedList = useSocialUi((s) => s.view.blocked);
  const me = useSocialUi((s) => s.view.me?.id);
  const people = useRoomPeople();
  const linkRules = useLinkRules();
  const chatExtras = useChatExtras();
  const fieldCue = useFieldCue(visible);
  const [rewriteCue, setRewriteCue] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  const [away, setAway] = useState(false);
  const [newer, setNewer] = useState(false);

  // People the player blocks never show (the room leaves them out too).
  const blocked = useMemo(() => new Set(blockedList.map((p) => p.id)), [blockedList]);
  const shownMsgs = useMemo(() => all.filter((m) => !blocked.has(m.from)), [all, blocked]);
  // Whom the player may act on in the room: their messages may be taken away too.
  const actsOn = useMemo(
    () => new Set(room.people.filter((p) => p.can.kick || p.can.mute).map((p) => p.id)),
    [room.people],
  );
  const messages = useMemo(
    () => shownMsgs.map((m) => asChatMessage(m, m.mine || actsOn.has(m.from))),
    [shownMsgs, actsOn],
  );
  const byKey = useMemo(() => new Map(shownMsgs.map((m) => [m.key, m])), [shownMsgs]);
  const byKeyNow = useRef(byKey);
  byKeyNow.current = byKey;
  const peopleNow = useRef(people);
  peopleNow.current = people;

  // Only what comes while the chat is on screen rises in: what it opened with is simply there.
  const known = useRef<Set<string> | null>(null);
  if (known.current === null && loaded) known.current = new Set(all.map((m) => m.key));
  const rising = (key: string) => known.current !== null && !known.current.has(key);

  // The clock the rows read (the day lines): when the list changes or the chat comes on screen.
  // biome-ignore lint/correctness/useExhaustiveDependencies: these say when to read the clock again
  const now = useMemo(() => Date.now(), [messages, visible]);
  const dayWords = useMemo(
    () => ({ today: t("chat.day.today"), yesterday: t("chat.day.yesterday") }),
    [t],
  );

  const actions = useMemo<RowActions>(
    () => ({
      onEdit: () => {},
      onDelete: async (message) => {
        const id = byKeyNow.current.get(message.key)?.id;
        return id !== undefined ? deleteRoomMessage(id) : false;
      },
      onReport: (message) => {
        const msg = byKeyNow.current.get(message.key);
        if (msg) reportRoomMessage(msg, peopleNow.current(msg.from));
      },
      onRetry: (message) => void retryRoomMessage(message.key),
      onDiscard: (message) => dropRoomMessage(message.key, false),
      onRewrite: (message) => {
        dropRoomMessage(message.key, true);
        setRewriteCue((n) => n + 1);
      },
      onReply: () => {},
      onReact: () => {},
      onJump: () => {},
      onUnfurl: async () => false,
    }),
    [],
  );
  const shared = useMemo<RowShared>(
    () => ({
      conv: `v:${room.id}`,
      // Words, emoji, stickers and links: no replies, reactions, "yazıyor" or read marks here.
      extras: { ...NO_EXTRAS, stickers: true, links: chatExtras.links },
      links: linkRules,
      me,
      writable: false,
      quick: [],
      people,
      cards: false,
      // The menu alone beside a bubble (no reactions or replies here): bubbles take the rest.
      toolbarRoom: 40,
    }),
    [room.id, chatExtras.links, linkRules, me, people],
  );

  // The text size chat has: a zoom on the list (CHAT-PROD), Ctrl + wheel over it and Ctrl + "+" /
  // "-" / "0" while the focus is in the room panel. Motion measures the rows in the list's pixels.
  const zoom = useRef(1);
  const pagePoint = useCallback(
    (point: { x: number; y: number }) => ({ x: point.x / zoom.current, y: point.y / zoom.current }),
    [],
  );
  useLayoutEffect(() => {
    const apply = (size: number) => {
      const element = scroller.current;
      const next = zoomOf(size);
      if (!element || next === zoom.current) return;
      zoom.current = next;
      element.style.zoom = next === 1 ? "" : String(next);
      element.style.setProperty("--chat-zoom", String(next));
      if (atBottom.current) element.scrollTop = newestTop(element, content.current);
    };
    apply(useChatTextSize.getState().size);
    return useChatTextSize.subscribe((state, previous) => {
      if (state.size !== previous.size) apply(state.size);
    });
  }, []);
  const sizeTold = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(sizeTold.current), []);
  const resize = useCallback(
    (to: TextSize) => {
      const store = useChatTextSize.getState();
      if (store.size !== to) store.setSize(to);
      window.clearTimeout(sizeTold.current);
      sizeTold.current = window.setTimeout(
        () => announce(t("chatProd.zoom.announce", { level: formatPercent(to) })),
        300,
      );
    },
    [t],
  );
  useEffect(() => {
    const box = root.current;
    if (!box || !visible) return;
    const os = detectOs();
    const current = () => useChatTextSize.getState().size;
    let wheel = WHEEL_START;
    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      const turn = wheelZoom(wheel, event.deltaY, event.deltaMode, event.timeStamp);
      wheel = turn.state;
      if (turn.steps) resize(stepTextSize(current(), turn.steps));
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const key = zoomKeyOf(event, os);
      if (!key || !chatKeyHere(box, event.target)) return;
      event.preventDefault();
      resize(key === "reset" ? DEFAULT_TEXT_SIZE : stepTextSize(current(), key === "in" ? 1 : -1));
    };
    box.addEventListener("wheel", onWheel, { passive: false });
    document.addEventListener("keydown", onKey);
    return () => {
      box.removeEventListener("wheel", onWheel);
      document.removeEventListener("keydown", onKey);
    };
  }, [visible, resize]);

  // Kept at the newest message while the player is there.
  const lastKey = messages.at(-1)?.key;
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the messages change
  useLayoutEffect(() => {
    const element = scroller.current;
    if (element && atBottom.current) element.scrollTop = newestTop(element, content.current);
  }, [lastKey, messages.length, loaded]);

  // More came while the chat was out of sight than fits: it opens at "Yeni Mesajlar".
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!dividerKey || !element) return;
    const line = element.querySelector<HTMLElement>("[data-divider]");
    if (!line) return;
    const view = element.getBoundingClientRect();
    const box = line.getBoundingClientRect();
    if (box.top >= view.top) return;
    element.scrollTop += box.top - view.top - 8;
    atBottom.current = false;
    setAway(true);
  }, [dividerKey]);

  // The field growing or the panel resizing keep the newest message in view.
  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      if (atBottom.current) element.scrollTop = newestTop(element, content.current);
    });
    observer.observe(element);
    if (content.current) observer.observe(content.current);
    return () => observer.disconnect();
  }, []);

  // Something new while reading back: say so instead of jumping.
  const seenLast = useRef(lastKey);
  useEffect(() => {
    if (lastKey && lastKey !== seenLast.current && !atBottom.current) setNewer(true);
    seenLast.current = lastKey;
  }, [lastKey]);

  const onScroll = () => {
    const element = scroller.current;
    if (!element) return;
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
    const bottom = distance < BOTTOM_SLACK;
    atBottom.current = bottom;
    if (bottom === away) setAway(!bottom);
    if (bottom && newer) setNewer(false);
  };

  const toBottom = useCallback(
    (smooth: boolean) => {
      const element = scroller.current;
      if (!element) return;
      const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
      atBottom.current = true;
      setAway(false);
      setNewer(false);
      const glides = smooth && !reduce && distance <= element.clientHeight * GLIDE_SCREENS;
      element.scrollTo({
        top: newestTop(element, content.current),
        behavior: glides ? "smooth" : "auto",
      });
    },
    [reduce],
  );

  const page = useCallback((direction: 1 | -1) => {
    const element = scroller.current;
    if (!element) return;
    element.scrollBy({
      top: direction * element.clientHeight * PAGE_SHARE,
      behavior: scrollBehavior(),
    });
  }, []);

  const dividerWords = t("chat.divider");
  return (
    <div ref={root} className="relative flex min-h-0 flex-1 flex-col">
      <MotionConfig transformPagePoint={pagePoint}>
        <motion.div
          ref={scroller}
          layoutScroll
          onScroll={onScroll}
          // A soft top edge: a message cut there reads as "more above", not as a broken row.
          className="relative min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain pt-2 [scrollbar-gutter:stable] [mask-image:linear-gradient(to_bottom,transparent,black_16px)]"
        >
          <div
            ref={content}
            className={cn(
              "flex min-h-full flex-col justify-end transition-opacity duration-200",
              !loaded && "opacity-0",
            )}
          >
            {loaded && !messages.length ? (
              <div className="flex flex-1 items-center justify-center px-6 py-8">
                <Unzoomed>
                  <div className="flex max-w-[280px] flex-col items-center gap-2 text-center">
                    <ContentIcon kind="chat" quiet />
                    <p className="text-[13px] leading-snug text-ash-400">{t("roomChat.empty")}</p>
                  </div>
                </Unzoomed>
              </div>
            ) : null}
            <LayoutGroup>
              <LayoutCue cue={messages} layout={layout} />
              <ol
                role="log"
                aria-live="off"
                aria-label={t("roomChat.title")}
                className="flex flex-col"
              >
                <AnimatePresence initial={false}>
                  {messages.map((message, index) => {
                    const previous = messages[index - 1];
                    const next = messages[index + 1];
                    const day = !previous || !sameLocalDay(previous.at, message.at);
                    const divided = message.key === dividerKey;
                    const shown = byKey.get(message.key);
                    return (
                      <RoomItem
                        key={message.key}
                        message={message}
                        tombstone={
                          shown?.deleted
                            ? t(TOMBSTONE_KEYS[tombstoneOf(shown.deleted, shown.mine)])
                            : null
                        }
                        sender={people(message.from)}
                        first={day || divided || !sameGroup(previous, message)}
                        last={!next || next.key === dividerKey || !sameGroup(message, next)}
                        dayText={day ? dayLineText(message.at, now, dayWords) : null}
                        divider={divided ? dividerWords : null}
                        deleteOpen={(message.deleteUntil ?? 0) > now}
                        rising={rising(message.key)}
                        actions={actions}
                        shared={shared}
                        layout={layout}
                        reduce={reduce}
                      />
                    );
                  })}
                </AnimatePresence>
              </ol>
            </LayoutGroup>
          </div>
        </motion.div>
      </MotionConfig>
      <TimeTip list={scroller} cue={messages} />
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
        <RoomComposer
          focusRequest={fieldCue + rewriteCue}
          onSent={() => toBottom(false)}
          onPage={page}
        />
      </div>
    </div>
  );
}
