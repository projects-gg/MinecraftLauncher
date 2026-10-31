// A conversation: its messages from this computer's history, newest at the bottom (older pages
// come when the player scrolls up, keeping their place), day lines, "Yeni mesajlar" over what was
// unread when it opened, "Son mesajlara in" when new ones come while the player reads back, and the
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
//
// CHAT-PROD (the owner, 27 Oct 2026: "production ready"): the text size (text-zoom.ts) follows
// Ctrl + wheel over the conversation and Ctrl + "+" / "-" / "0" while the focus is in its panel,
// and "Yazı boyutu" in the friends settings. It is a CSS zoom on the message list, set outside
// React (no row renders again, motion sees no layout change; motion measures in the list's own
// pixels), keeping the place read; the plate at the top says the size (zoom-hud.tsx). Opened with
// more unread messages than fit, the conversation starts at "Yeni mesajlar"; read far back, "En
// yeni mesajlar" takes the player down; PageUp and PageDown in the composer scroll the messages.
// Opened to be written in (openChat), its field takes the focus. Screen readers hear the messages
// that arrive while it is open, not the older pages that load above. A row draws again only when
// what it shows changed (a long history stayed smooth: before, every new message, presence change
// and minute drew all of them again).
//
// CHAT-UX (the owner, 31 Oct 2026: day lines as in Discord): a day line says "Bugün", "Dün", "3 gün
// önce", then the date (chat-time.ts); a group is one sender's messages up to five minutes apart on
// one day, broken by a day line, "Yeni mesajlar" or a local line, its time under its last message;
// the pointer resting on a message shows its full time (time-tip.tsx). The clock the rows read
// moves when the list changes, when the conversation or the window comes back on screen and when
// a row's window closes: nothing wakes up for the day lines. CHAT-UX2: reading back, the day of
// the rows under the top edge floats there on a quiet plate (floating-day.ts), gone at the newest
// messages and over a day line that shows; read at a scroll, a fade only.
import { ArrowDown, CalendarBlank, HandWaving } from "@phosphor-icons/react";
import { cn, scrollBehavior, toast } from "@pl/ui";
import { AnimatePresence, LayoutGroup, MotionConfig, motion } from "motion/react";
import {
  memo,
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
import type { ChatMessage, ChatReaction, SocialTeam } from "@/lib/api/social-types";
import { formatPercent } from "@/lib/format";
import { detectOs } from "@/lib/platform";
import { useWindowAway } from "@/lib/window-away";
import { enter, glide, leave, useSocialMotion } from "../motion";
import type { PersonPlace } from "../person-menu";
import { useSocialUi } from "../store";
import { useChat } from "./chat-store";
import { dayLineText, sameLocalDay } from "./chat-time";
import { Composer, type ComposerState } from "./composer";
import { crewOf, deletionOf, peerOf, readable, sameGroup, tombstoneKind } from "./conversations";
import { firstRowUnder, floatingDayAt } from "./floating-day";
import { useChatExtras, useLinkRules } from "./extras";
import { MessageRow, type RowActions, type RowShared, rowWindows } from "./message-row";
import { messageLine } from "./preview";
import { quickReactions, REACTION_WINDOW_MS, reactVerdict, withMyReaction } from "./reactions";
import { type Quoted, quotedOf } from "./reply";
import { READ_AFTER_MS, seenKey } from "./seen";
import type { ChatPerson } from "./sender-name";
import { useChatTextSize } from "./text-size";
import {
  DEFAULT_TEXT_SIZE,
  stepTextSize,
  type TextSize,
  WHEEL_START,
  wheelZoom,
  zoomKeyOf,
  zoomOf,
} from "./text-zoom";
import { TimeTip } from "./time-tip";
import { onlineIn, useTyping, useTypingFeed, writersOf } from "./typing";
import { TypingPill } from "./typing-pill";
import { ZoomHud, type ZoomHudHandle } from "./zoom-hud";

/** Scrolled this close to the newest message counts as "at the bottom". */
const BOTTOM_SLACK = 56;
/** Older messages are asked for this close to the top. */
const TOP_SLACK = 160;
/** A message this much older than the newest shown still rises in (clocks differ a little). */
const FRESH_SLACK_MS = 5_000;
/** Read back farther than this many screens, "Son mesajlara in" shows without a new message. */
const FAR_SCREENS = 1.5;
/** "Son mesajlara in" glides down at most this many screens; from farther it jumps. */
const GLIDE_SCREENS = 3;
/** PageUp and PageDown scroll this share of a screen (a line of the last one stays in view). */
const PAGE_SHARE = 0.85;
/** Opened at "Yeni mesajlar", the line sits this far under the top (px of the list). */
const DIVIDER_GAP = 8;
/** More messages than this arriving at once are told as a count, not one by one. */
const TOLD_ONE_BY_ONE = 3;

/** The row across the middle of the list, and where its top is (a share of the list's height). */
function middleRow(list: HTMLElement): { row: HTMLElement; at: number } | null {
  const view = list.getBoundingClientRect();
  if (view.height <= 0) return null;
  const middle = view.top + view.height / 2;
  for (const row of list.querySelectorAll<HTMLElement>("ol[role='log'] > li")) {
    const box = row.getBoundingClientRect();
    if (box.bottom >= middle) return { row, at: (box.top - view.top) / view.height };
  }
  return null;
}

/**
 * Where the list shows its newest message at the bottom, from the layout. `scrollHeight` also
 * counts what a transform pushes out (a message rising in from 6–10 px lower, the light on a
 * quoted message): scrolled there, the whole list sank back a few pixels while every new message
 * rose in.
 */
function newestTop(list: HTMLElement, content: HTMLElement | null): number {
  if (!content) return list.scrollHeight;
  return Math.max(0, content.offsetTop + content.offsetHeight - list.clientHeight);
}

/** Scrolls `list` so `anchor`'s row is back where it was on screen (`zoom`: the list's zoom). */
function keepRow(list: HTMLElement, anchor: { row: HTMLElement; at: number }, zoom: number) {
  const view = list.getBoundingClientRect();
  const box = anchor.row.getBoundingClientRect();
  // Rectangles are on-screen pixels, the list scrolls in its own.
  list.scrollTop += (box.top - (view.top + anchor.at * view.height)) / zoom;
}

/** What of a crew its members' menus and tombstones read (roles, moderators, who is blocked). */
function crewKey(crew: SocialTeam | undefined): string {
  if (!crew) return "";
  const members = crew.members.map(
    (m) => `${m.id}:${m.role}:${m.moderator ? 1 : 0}:${m.blocked ? 1 : 0}`,
  );
  return `${crew.id}|${crew.role}|${members.join(",")}`;
}

/** The next moment a row's time changes what it may do (ms), or Infinity when none will. */
function nextChange(messages: readonly ChatMessage[], nowMs: number): number {
  let next = Number.POSITIVE_INFINITY;
  for (const m of messages) {
    for (const at of [m.editUntil, m.deleteUntil, m.at + REACTION_WINDOW_MS]) {
      if (at !== undefined && at > nowMs && at < next) next = at;
    }
  }
  return next;
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

/** The empty state keeps its own size whatever the text size: its pixel icon stays on whole
 * pixels. */
function Unzoomed({ children }: { children: ReactNode }) {
  return <div className="[zoom:calc(1/var(--chat-zoom,1))]">{children}</div>;
}

/** The day over a row, when it starts one. */
function DayLine({ text }: { text: string }) {
  return (
    <p className="flex justify-center px-2 pt-3 pb-2">
      <span className="text-[12px] font-medium text-ash-400">{text}</span>
    </p>
  );
}

/** One message in the list: its row with what goes over it (its day, "Yeni mesajlar"). */
interface MessageItemProps {
  message: ChatMessage;
  text: string;
  tombstone: string | null;
  sender: ChatPerson;
  personPlace: PersonPlace;
  crew: boolean;
  first: boolean;
  last: boolean;
  dayText: string | null;
  /** "Yeni mesajlar" goes over it (its words). */
  divider: string | null;
  actions: RowActions;
  shared: RowShared;
  quoted: Quoted | null;
  reactions: readonly ChatReaction[] | undefined;
  seen: boolean;
  rising: boolean;
  flash: number;
  editOpen: boolean;
  deleteOpen: boolean;
  reactOpen: boolean;
  weekOpen: boolean;
  layout: boolean;
  reduce: boolean;
}

/** The sender as drawn: a new snapshot of the same friend draws nothing again. */
function samePerson(a: ChatPerson, b: ChatPerson): boolean {
  return (
    a === b ||
    (a.name === b.name &&
      a.avatar === b.avatar &&
      a.avatarUrl === b.avatarUrl &&
      a.mark === b.mark &&
      (a.badge === b.badge || JSON.stringify(a.badge) === JSON.stringify(b.badge)))
  );
}

/** A quote as drawn (the list of quotes is built again with every change of the messages). */
function sameQuote(a: Quoted | null, b: Quoted | null): boolean {
  return (
    a === b ||
    (a !== null &&
      b !== null &&
      a.name === b.name &&
      a.text === b.text &&
      a.sticker === b.sticker &&
      a.deleted === b.deleted &&
      a.missing === b.missing &&
      a.key === b.key)
  );
}

function sameMessageItem(a: MessageItemProps, b: MessageItemProps): boolean {
  for (const key of Object.keys(a) as Array<keyof MessageItemProps>) {
    if (key === "sender") {
      if (!samePerson(a.sender, b.sender)) return false;
    } else if (key === "quoted") {
      if (!sameQuote(a.quoted, b.quoted)) return false;
    } else if (a[key] !== b[key]) {
      return false;
    }
  }
  return true;
}

const MessageItem = memo(function MessageItem({
  message,
  text,
  tombstone,
  sender,
  personPlace,
  crew,
  first,
  last,
  dayText,
  divider,
  actions,
  shared,
  quoted,
  reactions,
  seen,
  rising,
  flash,
  editOpen,
  deleteOpen,
  reactOpen,
  weekOpen,
  layout,
  reduce,
}: MessageItemProps) {
  const place = useMemo(() => ({ crew, first, last }), [crew, first, last]);
  return (
    <motion.li
      // Its day, for the line that floats while the player reads back (CHAT-UX2).
      data-day-at={message.at}
      data-day-start={dayText ? "" : undefined}
      layout={layout ? "position" : false}
      transition={{ layout: glide }}
      initial={
        rising
          ? {
              opacity: 0,
              // Under reduced motion a fade only: the same transform as the end ("none" would let
              // motion start the scale at 0).
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
        // Bone words between quiet lines: gold is never a sentence (QA-TASTE).
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
        text={text}
        tombstone={tombstone}
        sender={sender}
        personPlace={personPlace}
        place={place}
        actions={actions}
        editOpen={editOpen}
        deleteOpen={deleteOpen}
        reactOpen={reactOpen}
        weekOpen={weekOpen}
        shared={shared}
        quoted={quoted}
        reactions={reactions}
        seen={seen}
        fresh={rising}
        flash={flash}
      />
    </motion.li>
  );
}, sameMessageItem);

/**
 * When the list changes (a message comes, goes, grows), this empty mark updates with it, and in a
 * LayoutGroup one member's update has every row note its place first: the rows that move glide,
 * though only the rows whose words changed draw again. It sits still at the list's top corner.
 */
const LayoutCue = memo(function LayoutCue({ cue, layout }: { cue: unknown; layout: boolean }) {
  void cue;
  if (!layout) return null;
  return (
    <motion.span layout aria-hidden className="pointer-events-none absolute top-0 left-0 size-0" />
  );
});

/** One of this computer's quiet lines in the list (a plan, a newcomer). */
const LineItem = memo(function LineItem({
  line,
  dayText,
  rising,
  layout,
}: {
  line: ChatLine;
  dayText: string | null;
  rising: boolean;
  layout: boolean;
}) {
  return (
    <motion.li
      data-day-at={line.at}
      data-day-start={dayText ? "" : undefined}
      layout={layout ? "position" : false}
      transition={{ layout: glide }}
      initial={rising ? { opacity: 0 } : false}
      animate={{ opacity: 1, transition: enter(0.24) }}
      exit={{ opacity: 0, transition: leave(0.16) }}
    >
      {dayText ? <DayLine text={dayText} /> : null}
      <div
        className={cn(
          "flex items-center justify-center gap-1.5 px-6 text-center text-[12px] text-ash-400",
          // Its "Selam ver" comes and goes inside the line's own height.
          line.kind === "welcome" ? "min-h-8 py-1" : "py-1.5",
        )}
      >
        {line.kind === "welcome" ? (
          <HandWaving size={13} weight="duotone" className="shrink-0 text-gold-300" aria-hidden />
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
});

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
  const { t, i18n } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const thread = useChat((s) => s.threads[conv]);
  const edits = useChat((s) => s.edits);
  const deleting = useChat((s) => s.deleting);
  const reacting = useChat((s) => s.reacting);
  const replyingKey = useChat((s) => s.replying[conv]);
  const setReplying = useChat((s) => s.setReplying);
  const startReacting = useChat((s) => s.startReacting);
  const endReacting = useChat((s) => s.endReacting);
  const liveExtras = useChatExtras();
  const liveLinkRules = useLinkRules();
  const recentEmoji = useEmojiPrefs((s) => s.recentEmoji);
  useTypingFeed();
  const load = useChat((s) => s.load);
  const loadOlder = useChat((s) => s.loadOlder);
  const setDeleting = useChat((s) => s.setDeleting);
  const setDraft = useChat((s) => s.setDraft);
  const root = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const hud = useRef<ZoomHudHandle>(null);
  const atBottom = useRef(true);
  const [away, setAway] = useState(false);
  const [far, setFar] = useState(false);
  const [newer, setNewer] = useState(false);
  const [editing, setEditing] = useState<ChatMessage | null>(null);
  const [focusRequest, setFocusRequest] = useState(0);
  // A row's window closed (below): the clock is read again.
  const [tick, setTick] = useState(0);
  const windowAway = useWindowAway();
  const open = useSocialUi((s) => s.open);
  const me = useSocialUi((s) => s.view.me?.id);
  const shareRead = useSocialUi((s) => s.view.me?.privacy.shareRead !== false);
  const peerRead = useSocialUi(
    (s) => s.view.chat?.conversations.find((c) => c.id === conv)?.peerRead,
  );
  const focusAsked = useSocialUi((s) => s.chatFocus);
  const writable = composer.kind === "open";
  // Who is online now (a writer who went offline stops "yazıyor").
  const friends = useSocialUi((s) => s.view.friends);
  const crews = useSocialUi((s) => s.view.crews);
  // Whose deletion a crew chat's tombstone tells: its leader's or a moderator's.
  const liveTeam = useSocialUi((s) => {
    const id = crewOf(conv);
    return id ? s.view.crews.find((c) => c.id === id) : undefined;
  });

  // Every snapshot of the friends service brings new objects: what the rows read of it stays the
  // same object while it says the same, so a presence change or a new unread count draws no row.
  const teamKey = crewKey(liveTeam);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the key says when the crew changed
  const team = useMemo(() => liveTeam, [teamKey]);
  const extrasKey = JSON.stringify(liveExtras);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the key says when they changed
  const extras = useMemo(() => liveExtras, [extrasKey]);
  const linksKey = JSON.stringify(liveLinkRules ?? null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the key says when they changed
  const linkRules = useMemo(() => liveLinkRules, [linksKey]);
  const peopleNow = useRef(people);
  peopleNow.current = people;
  const stablePeople = useCallback((id: string) => peopleNow.current(id), []);
  const reportNow = useRef(onReport);
  reportNow.current = onReport;

  // Where a sender's own menu opens (their face and name): the crew's chat, or the conversation
  // with them on screen (FIX2).
  const personPlace = useMemo<PersonPlace>(
    () => (crew ? { crew: team, from: "team" } : { inConversation: true, from: "chat" }),
    [crew, team],
  );

  // Kept at the newest message while the player is there; an older page keeps the place read.
  const before = useRef<{ first?: string; height: number; top: number }>({ height: 0, top: 0 });

  // The text size: a zoom on the list, applied before the first paint and at every change.
  const zoom = useRef(1);
  // Motion measures and moves the rows in the list's own pixels.
  const pagePoint = useCallback(
    (point: { x: number; y: number }) => ({ x: point.x / zoom.current, y: point.y / zoom.current }),
    [],
  );
  useLayoutEffect(() => {
    const apply = (size: number) => {
      const element = scroller.current;
      const next = zoomOf(size);
      if (!element || next === zoom.current) return;
      const anchor = atBottom.current ? null : middleRow(element);
      zoom.current = next;
      element.style.zoom = next === 1 ? "" : String(next);
      // What keeps its own size inside (the toolbar, the empty state) reads the zoom from here.
      element.style.setProperty("--chat-zoom", String(next));
      if (atBottom.current) element.scrollTop = newestTop(element, content.current);
      else if (anchor) keepRow(element, anchor, next);
      before.current = { ...before.current, height: element.scrollHeight, top: element.scrollTop };
    };
    apply(useChatTextSize.getState().size);
    return useChatTextSize.subscribe((state, previous) => {
      if (state.size !== previous.size) apply(state.size);
    });
  }, []);

  // Ctrl + wheel over the conversation, Ctrl + "+" / "-" / "0" while the focus is in its panel
  // (not in a menu or picker over it): the size steps, the plate shows, screen readers hear it.
  // Screen readers hear where a quick run of steps (a spun wheel) ended, once.
  const sizeTold = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(sizeTold.current), []);
  const resize = useCallback(
    (to: TextSize) => {
      const store = useChatTextSize.getState();
      if (store.size !== to) store.setSize(to);
      hud.current?.show();
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
    if (!box || !onScreen) return;
    const os = detectOs();
    const current = () => useChatTextSize.getState().size;
    let wheel = WHEEL_START;
    const onWheel = (event: WheelEvent) => {
      // A touchpad's pinch arrives as a wheel with Ctrl, on every system.
      if (!event.ctrlKey) return;
      event.preventDefault();
      const turn = wheelZoom(wheel, event.deltaY, event.deltaMode, event.timeStamp);
      wheel = turn.state;
      if (turn.steps) resize(stepTextSize(current(), turn.steps));
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const key = zoomKeyOf(event, os);
      if (!key) return;
      const panel = box.closest<HTMLElement>("[role='dialog']") ?? box;
      const target = event.target;
      if (!(target instanceof Node) || (target !== document.body && !panel.contains(target))) {
        return;
      }
      event.preventDefault();
      resize(key === "reset" ? DEFAULT_TEXT_SIZE : stepTextSize(current(), key === "in" ? 1 : -1));
    };
    box.addEventListener("wheel", onWheel, { passive: false });
    document.addEventListener("keydown", onKey);
    return () => {
      box.removeEventListener("wheel", onWheel);
      document.removeEventListener("keydown", onKey);
    };
  }, [onScreen, resize]);

  useEffect(() => {
    void load(conv);
  }, [conv, load]);

  // Opened to be written in: the field takes the focus (a crew's tab picked by hand keeps it).
  useEffect(() => {
    if (focusAsked !== conv || !onScreen) return;
    if (useSocialUi.getState().takeChatFocus(conv) && writable) setFocusRequest((n) => n + 1);
  }, [focusAsked, conv, onScreen, writable]);

  const messages = useMemo(() => thread?.messages ?? [], [thread?.messages]);

  // The clock the rows read: what a row may do (edit for 15 minutes, delete and react for a week)
  // and the day lines ("Bugün", "Dün", "3 gün önce"). It is read again when the list changes, when
  // the conversation or the window comes back on screen, and at the next moment a row's window
  // closes; never every minute (a long list drew all its rows again each minute), and never for
  // the day lines alone (a list left open past midnight says "Dün" from its next change).
  // biome-ignore lint/correctness/useExhaustiveDependencies: these say when to read the clock again
  const now = useMemo(() => Date.now(), [messages, onScreen, windowAway, tick]);
  useEffect(() => {
    const next = nextChange(messages, now);
    if (next === Number.POSITIVE_INFINITY) return;
    const timer = window.setTimeout(
      () => setTick((n) => n + 1),
      Math.min(Math.max(next - Date.now() + 50, 1_000), 2_000_000_000),
    );
    return () => window.clearTimeout(timer);
  }, [messages, now]);

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
  // older pages (older than anything shown), are simply there. Screen readers hear the others'
  // new ones (the list itself is not a live region: older pages loading above would be read out).
  const known = useRef<{ keys: Set<string>; newest: number } | null>(null);
  const fresh = (message: ChatMessage) =>
    known.current !== null &&
    !known.current.keys.has(message.key) &&
    message.at >= known.current.newest - FRESH_SLACK_MS;
  useEffect(() => {
    if (!thread?.loaded) return;
    const opened = known.current !== null;
    const state = known.current ?? { keys: new Set<string>(), newest: 0 };
    known.current = state;
    const told: ChatMessage[] = [];
    const newest = state.newest;
    for (const message of messages) {
      if (
        opened &&
        !state.keys.has(message.key) &&
        !message.mine &&
        readable(message) &&
        message.at >= newest - FRESH_SLACK_MS
      ) {
        told.push(message);
      }
      state.keys.add(message.key);
      state.newest = Math.max(state.newest, message.at);
    }
    if (told.length > TOLD_ONE_BY_ONE) {
      announce(t("chat.waiting", { count: told.length }));
    } else {
      for (const message of told) {
        announce(
          t("chat.notices.direct", {
            name: peopleNow.current(message.from).name,
            text: messageLine(message, t, i18n.language),
          }),
        );
      }
    }
  }, [thread?.loaded, messages, t, i18n.language]);

  // "Yeni mesajlar" goes over the first message that was unread when the conversation opened,
  // before the first paint (the list may open at it, below).
  const [dividerKey, setDividerKey] = useState<string | null>(null);
  const dividerSet = useRef(false);
  useLayoutEffect(() => {
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the messages change
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const previous = before.current;
    if (previous.first && firstKey !== previous.first && element.scrollHeight > previous.height) {
      // An older page came in above: stay on the same messages.
      element.scrollTop = previous.top + (element.scrollHeight - previous.height);
    } else if (atBottom.current) {
      element.scrollTop = newestTop(element, content.current);
    }
    before.current = { first: firstKey, height: element.scrollHeight, top: element.scrollTop };
  }, [firstKey, lastKey, messages.length]);

  // More came while the player was away than fits under "Yeni mesajlar": the conversation opens
  // at the line, the first of them at the top (it is read once the player reaches the newest).
  const openedAtDivider = useRef(false);
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!dividerKey || openedAtDivider.current || !element) return;
    openedAtDivider.current = true;
    const line = element.querySelector<HTMLElement>("[data-divider]");
    if (!line) return;
    const view = element.getBoundingClientRect();
    const box = line.getBoundingClientRect();
    if (box.top >= view.top) return;
    element.scrollTop += (box.top - view.top) / zoom.current - DIVIDER_GAP;
    atBottom.current = false;
    before.current.top = element.scrollTop;
    setAway(true);
  }, [dividerKey]);

  // The composer growing, the panel resizing or the messages growing (a day line, "Yeni mesajlar",
  // a longer edit) keep the newest message in view.
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

  // CHAT-UX2: reading back, the day of the first row under the list's top edge floats there
  // (floating-day.ts): read from the rows' places at a scroll and when the list changes.
  const [floatingAt, setFloatingAt] = useState<number | null>(null);
  const readFloating = useCallback(() => {
    const element = scroller.current;
    if (!element) return;
    const rows = element.querySelectorAll<HTMLElement>("ol[role='log'] > li");
    const edge = element.getBoundingClientRect().top;
    const bottomOf = (i: number) => rows[i]?.getBoundingClientRect().bottom ?? 0;
    const row = rows[firstRowUnder(rows.length, bottomOf, edge)];
    const at = Number(row?.dataset.dayAt);
    setFloatingAt(
      floatingDayAt(
        row && Number.isFinite(at)
          ? { top: row.getBoundingClientRect().top, at, start: row.dataset.dayStart !== undefined }
          : null,
        edge,
        !atBottom.current,
      ),
    );
  }, []);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the rows moved when the list changed
  useLayoutEffect(() => readFloating(), [items, readFloating]);

  const onScroll = () => {
    const element = scroller.current;
    if (!element) return;
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
    const bottom = distance < BOTTOM_SLACK;
    atBottom.current = bottom;
    if (bottom === away) setAway(!bottom);
    const farBack = distance > element.clientHeight * FAR_SCREENS;
    if (farBack !== far) setFar(farBack);
    if (bottom && newer) setNewer(false);
    if (element.scrollTop < TOP_SLACK && thread?.more && !thread.older) {
      before.current = { first: firstKey, height: element.scrollHeight, top: element.scrollTop };
      void loadOlder(conv);
    }
    before.current.top = element.scrollTop;
    readFloating();
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
      const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
      atBottom.current = true;
      setAway(false);
      setFar(false);
      setNewer(false);
      // From far back it jumps: a long glide would only blur past what the player left.
      const glides = smooth && !reduce && distance <= element.clientHeight * GLIDE_SCREENS;
      element.scrollTo({
        top: newestTop(element, content.current),
        behavior: glides ? "smooth" : "auto",
      });
    },
    [reduce],
  );

  // PageUp and PageDown in the composer scroll the messages, as in other chat apps.
  const page = useCallback((direction: 1 | -1) => {
    const element = scroller.current;
    if (!element) return;
    element.scrollBy({
      top: direction * element.clientHeight * PAGE_SHARE,
      behavior: scrollBehavior(),
    });
  }, []);

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
      onReport: (message) => reportNow.current(message),
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
      people: stablePeople,
    }),
    [conv, extras, linkRules, me, writable, recentEmoji, stablePeople],
  );
  const byId = useMemo(() => {
    const map = new Map<number, ChatMessage>();
    for (const message of messages) if (message.id !== undefined) map.set(message.id, message);
    return map;
  }, [messages]);
  // What the rows show of their senders: a new snapshot of the same people (a presence, an unread
  // count) says the same, and the list does not draw again.
  const peopleKey = useMemo(() => {
    const ids = [...new Set(messages.map((m) => m.from))];
    return ids
      .map((id) => {
        const person = people(id);
        return `${id}:${person.name}:${person.avatar ?? ""}:${person.avatarUrl ?? ""}:${person.mark ?? ""}:${JSON.stringify(person.badge ?? null)}`;
      })
      .join("|");
  }, [messages, people]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: peopleKey says when the names changed
  const quotes = useMemo(() => {
    const map = new Map<string, Quoted>();
    const nameOf = (id: string) => (id === me ? t("chat2.reply.you") : people(id).name);
    for (const message of messages) {
      if (message.reply) map.set(message.key, quotedOf(message.reply, byId, nameOf));
    }
    return map;
  }, [messages, byId, me, peopleKey, t]);
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

  const dividerWords = t("chat.divider");
  const dayWords = useMemo(
    () => ({ today: t("chat.day.today"), yesterday: t("chat.day.yesterday") }),
    [t],
  );

  // The list draws again only when what its rows show changed: typing, a scroll, a presence or an
  // unread count elsewhere leave it alone (its AnimatePresence went through every row each time).
  // biome-ignore lint/correctness/useExhaustiveDependencies: peopleKey says when the senders changed
  const list = useMemo(
    () => (
      <LayoutGroup>
        <LayoutCue cue={items} layout={layout} />
        <ol
          role="log"
          aria-live="off"
          aria-label={t("chat.tabs.messages")}
          className="flex flex-col"
        >
          {/* Not `presenceAffectsLayout`: it handed every row a new context at each render,
            and every row's motion drew again (LayoutCue makes them glide instead). */}
          <AnimatePresence initial={false} presenceAffectsLayout={false}>
            {items.map((item, index) => {
              const previousItem = items[index - 1];
              const nextItem = items[index + 1];
              const day = !previousItem || !sameLocalDay(itemAt(previousItem), itemAt(item));
              const dayText = day ? dayLineText(itemAt(item), now, dayWords) : null;
              if (item.kind === "line") {
                return (
                  <LineItem
                    key={item.line.key}
                    line={item.line}
                    dayText={dayText}
                    rising={item.line.at >= openedAt.current}
                    layout={layout}
                  />
                );
              }
              const { message } = item;
              // A line between two messages starts a new group, as "Yeni mesajlar" does.
              const previous = previousItem?.kind === "message" ? previousItem.message : undefined;
              const next = nextItem?.kind === "message" ? nextItem.message : undefined;
              const divided = message.key === dividerKey;
              const deletion = deletionOf(message, deleting, me);
              const windows = rowWindows(message, now);
              return (
                <MessageItem
                  key={message.key}
                  message={message}
                  text={edits[message.key] ?? message.text}
                  tombstone={
                    deletion
                      ? t(`fix2.tombstone.${tombstoneKind(deletion.by, message.from, me, team)}`)
                      : null
                  }
                  sender={people(message.from)}
                  personPlace={personPlace}
                  crew={crew}
                  first={day || divided || !sameGroup(previous, message)}
                  last={!next || next.key === dividerKey || !sameGroup(message, next)}
                  dayText={dayText}
                  divider={divided ? dividerWords : null}
                  actions={actions}
                  shared={shared}
                  quoted={quotes.get(message.key) ?? null}
                  reactions={reacting[message.key] ?? message.reactions}
                  seen={message.key === seen}
                  rising={fresh(message)}
                  flash={flash?.key === message.key ? flash.n : 0}
                  editOpen={windows.editOpen}
                  deleteOpen={windows.deleteOpen}
                  reactOpen={windows.reactOpen}
                  weekOpen={windows.weekOpen}
                  layout={layout}
                  reduce={reduce}
                />
              );
            })}
          </AnimatePresence>
        </ol>
      </LayoutGroup>
    ),
    [
      items,
      edits,
      deleting,
      reacting,
      now,
      dividerKey,
      dividerWords,
      dayWords,
      quotes,
      seen,
      flash,
      personPlace,
      actions,
      shared,
      layout,
      reduce,
      me,
      team,
      t,
      crew,
      peopleKey,
    ],
  );

  return (
    <div ref={root} className="relative flex min-h-0 flex-1 flex-col">
      <MotionConfig transformPagePoint={pagePoint}>
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
              <div className="flex flex-1 items-center justify-center px-6 py-10">
                <Unzoomed>{empty}</Unzoomed>
              </div>
            ) : null}
            {list}
          </div>
        </motion.div>
      </MotionConfig>
      {/* CHAT-UX2: reading back, the day under the top edge on a quiet plate over the list (its
          own size at any text size; a fade, nothing moves). */}
      <AnimatePresence initial={false}>
        {floatingAt !== null ? (
          <motion.div
            key="day"
            aria-hidden
            className="pointer-events-none absolute top-2 left-1/2 z-10 -translate-x-1/2 rounded-md bg-obsidian-800/95 px-2.5 py-1 text-[12px] font-medium whitespace-nowrap text-ash-300 shadow-[0_10px_30px_rgb(34_6_6/0.5)] ring-1 ring-white/8"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.16) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            {dayLineText(floatingAt, now, dayWords)}
          </motion.div>
        ) : null}
      </AnimatePresence>
      <TimeTip list={scroller} cue={items} />
      <div className="relative">
        <AnimatePresence initial={false}>
          {away && (newer || far) ? (
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
          onPage={page}
          status={<ZoomHud ref={hud} onReset={() => resize(DEFAULT_TEXT_SIZE)} />}
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
