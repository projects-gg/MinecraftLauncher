// One chat message: a bubble (the player's own on the right, gold-tinted; others' on the left with
// their picture at the end of a group and, in crew chat, their name at its start), "düzenlendi",
// the time and where it stands (on its way, sent, not sent with "Tekrar dene"). Its menu (a right
// click, Shift+F10, and the toolbar's "…"): copy, edit (15 minutes), "Herkesten sil" (asked in
// place, one sentence), "Rapor et" (others' messages).
//
// A message deleted for everyone stays in its place as a tombstone (FIX2): the same bubble, quiet
// and empty-handed, saying who deleted it ("Bu mesajı sildin", "Bu mesaj silindi", "Ekip lideri
// sildi"…). No menu, no copy, edit, report or reply. The bubble morphs into it: the text fades out,
// the bubble glides to its new size (a transform-based layout animation, corners and ring kept by
// the scale correction) while its fill cross-fades to the tombstone's, and the words fade in.
// Reduced motion and Performance mode: the text fades out, then the words fade in.
//
// CHAT2: a reply carries its quote over the bubble (a press goes to the message); reactions sit
// under it as chips; the toolbar (😊+, ↩, …) comes at the bubble's corner on hover or focus, and
// the right click starts with the quick reactions. A sticker is drawn without a bubble at 112 px
// (it moves once when it arrives and when the pointer comes onto it), a message of one to three
// emoji large and without a bubble. Links that pass the rules can be pressed and the first one
// gets its card. Under the player's last message the friend has seen: "Görüldü".
import {
  ArrowBendUpLeft,
  ArrowClockwise,
  Check,
  Clock,
  Copy,
  Flag,
  LinkSimple,
  PencilSimple,
  Prohibit,
  Trash,
  WarningCircle,
} from "@phosphor-icons/react";
import { findSticker } from "@pl/stickers";
import {
  Button,
  ContextMenu,
  cn,
  easeOutExpo,
  type MenuItem,
  type MenuStripItem,
  toast,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { memo, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { bigEmojiCount, Sticker, UnknownSticker } from "@/features/emoji";
import type { ChatExtras, ChatLinkRules, ChatMessage, ChatReaction } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { copyText } from "../actions";
import { type MessageAction, messageMenu } from "../menus";
import { enter, leave, rise, swap, useSocialMotion } from "../motion";
import { PersonContextMenu, type PersonPlace } from "../person-menu";
import { SocialFace } from "../social-face";
import { LinkCard, MessageText, plainCardOf, ruleWords } from "./link-ui";
import { MessageToolbar } from "./message-toolbar";
import { ReactionChips } from "./reaction-ui";
import { REACTION_WINDOW_MS, sameEmoji, reactable as takesReactions } from "./reactions";
import { QuoteBlock, type Quoted } from "./reply";
import { type ChatPerson, SenderName } from "./sender-name";
import { messageLinks } from "./text";

/** What a row needs from its conversation. */
export interface RowPlace {
  /** Crew chat: names show over others' groups. */
  crew: boolean;
  /** The first of its sender's group (name), the last (picture, time). */
  first: boolean;
  last: boolean;
}

export interface RowActions {
  onEdit: (message: ChatMessage) => void;
  onDelete: (message: ChatMessage) => Promise<boolean>;
  onReport: (message: ChatMessage) => void;
  onRetry: (message: ChatMessage) => void;
  onDiscard: (message: ChatMessage) => void;
  /** A failed message's text goes back into the composer (link, phone, word list). */
  onRewrite: (message: ChatMessage) => void;
  /** CHAT2: "Yanıtla". */
  onReply: (message: ChatMessage) => void;
  /** CHAT2: the player's reaction, added or taken back. */
  onReact: (message: ChatMessage, emoji: string, on: boolean) => void;
  /** CHAT2: a quote was pressed: the message it quotes (by key). */
  onJump: (key: string) => void;
  /** CHAT2: the message came on screen with a link and no card yet. */
  /** Asks for the message's card; false when it could not be asked (the plain card shows). */
  onUnfurl: (message: ChatMessage) => Promise<boolean>;
}

/** What CHAT2 adds to every row of a conversation (all off for chat without the extras). */
export interface RowShared {
  /** The conversation on screen. */
  conv: string;
  extras: ChatExtras;
  links: ChatLinkRules | undefined;
  me: string | undefined;
  /** The player may write here (replies). */
  writable: boolean;
  /** The quick reactions' six. */
  quick: readonly string[];
  people: (id: string) => ChatPerson;
}

const timeFormat = () => dateTimeFormat({ hour: "2-digit", minute: "2-digit" });

/** Failures the same text would meet again: the player writes it another way. */
const REWRITE = new Set(["link", "phone", "words", "invalid"]);
/** CHAT2's failures, worded in the chat2 module. */
const CHAT2_FAILURES = new Set(["stickers", "links", "limitedLinks", "featureOff"]);

/** The bubble's corners (the panels' radius, `--radius-panel`), kept through its morph. */
const RADIUS = 10;
/** The bubble gliding to its new size: the list's entrance curve, a little longer. */
const MORPH = { duration: 0.36, ease: easeOutExpo };

/** Cards are asked for once per message and session (the backend keeps them with the message). */
const unfurled = new Set<string>();
/** Messages whose card could not be asked for this session: the plain card shows. */
const unfurlFailed = new Set<string>();

function StateMark({ message, seen }: { message: ChatMessage; seen: boolean }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  if (!message.mine) return null;
  const state = seen ? "seen" : message.state;
  const label =
    state === "seen"
      ? t("chat2.seen")
      : state === "sending"
        ? t("chat.state.sending")
        : state === "failed"
          ? t("chat.state.failed")
          : t("chat.state.sent");
  return (
    <span className="relative inline-flex h-3.5 items-center justify-end" title={label}>
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span key={state} className="inline-flex" {...swap(reduce, 0.18)}>
          {state === "seen" ? (
            <span className="text-[11px] font-medium text-ash-400">{label}</span>
          ) : state === "sending" ? (
            <Clock size={12} weight="bold" className="text-ash-400" aria-hidden />
          ) : state === "failed" ? (
            <WarningCircle size={13} weight="fill" className="text-flare-400" aria-hidden />
          ) : (
            <Check size={12} weight="bold" className="text-gold-300" aria-hidden />
          )}
        </motion.span>
      </AnimatePresence>
      {state === "seen" ? null : <span className="sr-only">{label}</span>}
    </span>
  );
}

/**
 * The bubble: its fill and ring sit on two layers behind the words, the message's and the
 * tombstone's, which cross-fade; the bubble's size change is a layout animation (transform), its
 * corners kept by the scale correction (the radius is a style for that).
 */
function Bubble({
  mine,
  failed,
  sending,
  gone,
  sizeKey,
  children,
}: {
  mine: boolean;
  failed: boolean;
  sending: boolean;
  gone: boolean;
  /** Changes whenever the words inside change (the bubble measures itself only then). */
  sizeKey: string;
  children: ReactNode;
}) {
  const { layout } = useSocialMotion();
  // One transition for both: the layout morph, and the fill's own fade.
  const layer = (shown: boolean, delay = 0) => ({
    initial: false as const,
    animate: { opacity: shown ? 1 : 0 },
    transition: {
      layout: MORPH,
      opacity: shown ? { duration: 0.26, ease: easeOutExpo, delay } : { duration: 0.16 },
    },
  });
  return (
    <motion.div
      layout={layout}
      layoutDependency={sizeKey}
      transition={{ layout: MORPH }}
      style={{ borderRadius: RADIUS }}
      className={cn(
        "relative isolate max-w-full overflow-hidden text-[13.5px] leading-[1.45] transition-opacity duration-200",
        sending && "opacity-70",
      )}
    >
      <motion.span
        aria-hidden
        layout={layout}
        layoutDependency={sizeKey}
        style={{ borderRadius: RADIUS }}
        className={cn(
          "absolute inset-0 -z-10 ring-1 ring-inset transition-[box-shadow] duration-200",
          mine
            ? "bg-[color-mix(in_oklab,var(--color-gold-500)_17%,var(--color-obsidian-800))] ring-gold-400/20"
            : "bg-obsidian-800/85 ring-white/7",
          failed && "ring-flare-400/45",
        )}
        {...layer(!gone)}
      />
      <motion.span
        aria-hidden
        layout={layout}
        layoutDependency={sizeKey}
        style={{ borderRadius: RADIUS }}
        className="absolute inset-0 -z-10 bg-obsidian-900/45 ring-1 ring-inset ring-white/10"
        {...layer(gone, 0.06)}
      />
      {children}
    </motion.div>
  );
}

/** The gold light a message takes once when the player is sent to it (a quote's press). */
function Flash({ flash }: { flash: number }) {
  const { reduce } = useSocialMotion();
  const light = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!flash || reduce || !light.current) return;
    const animation = light.current.animate(
      [{ opacity: 0 }, { opacity: 1, offset: 0.18 }, { opacity: 0 }],
      { duration: 1200, easing: "ease-out" },
    );
    return () => animation.cancel();
  }, [flash, reduce]);
  return (
    <span
      ref={light}
      aria-hidden
      className="pointer-events-none absolute inset-x-0 -inset-y-0.5 rounded-[var(--radius-panel)] bg-gold-400/10 opacity-0"
    />
  );
}

function MessageRowView({
  message,
  text,
  tombstone,
  sender,
  place,
  personPlace,
  actions,
  nowMs,
  shared,
  quoted,
  reactions,
  seen,
  fresh,
  flash,
}: {
  message: ChatMessage;
  /** The text shown (an edit on its way shows its new text). */
  text: string;
  /** Deleted for everyone (or by the player, waiting for the answer): what its tombstone says. */
  tombstone: string | null;
  sender: ChatPerson;
  place: RowPlace;
  /** Where the sender's own menu opens from (their face and name, a right click; FIX2). */
  personPlace?: PersonPlace;
  actions: RowActions;
  nowMs: number;
  shared: RowShared;
  /** The quote over a reply, resolved from this computer's history. */
  quoted: Quoted | null;
  /** Its reactions as shown (the player's change on its way included). */
  reactions: readonly ChatReaction[] | undefined;
  /** "Görüldü" under it. */
  seen: boolean;
  /** It came while the conversation is open (a sticker moves in once). */
  fresh: boolean;
  /** Grows when the conversation sends the player to this message: it lights once. */
  flash: number;
}) {
  const { t, i18n } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const lang = i18n.language.startsWith("en") ? "en" : "tr";
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const mine = message.mine;
  const gone = tombstone !== null;
  const sent = message.state === "sent";
  const sticker = message.sticker;
  const editable = !gone && sent && !sticker && (message.editUntil ?? 0) > nowMs;
  const deletable = !gone && sent && (message.deleteUntil ?? 0) > nowMs;
  const failed = message.state === "failed";
  const time = timeFormat().format(new Date(message.at));
  const { extras } = shared;
  const found = useMemo(
    () =>
      sticker || gone ? [] : messageLinks(text, extras.links && shared.links ? shared.links : null),
    [sticker, gone, text, extras.links, shared.links],
  );
  // The card is for the first link the rules let through. One the hub refused (the link leads
  // somewhere the rules refuse) turns that link back into plain text: "Bu bağlantı açılamaz."
  const cardLink = found.find((link) => link.ok);
  const [, setUnfurlFailedNow] = useState(0);
  const card =
    message.card && message.card.url === cardLink?.url
      ? message.card
      : !message.card && cardLink && unfurlFailed.has(message.key)
        ? plainCardOf(cardLink)
        : undefined;
  const refused = card?.state === "refused" ? card.url : null;
  const links = useMemo(
    () =>
      refused ? found.map((link) => (link.url === refused ? { ...link, ok: false } : link)) : found,
    [found, refused],
  );
  const firstLink = links.find((link) => link.ok);
  const reactable = !gone && extras.reactions && shared.writable && takesReactions(message, nowMs);
  const repliable = !gone && sent && extras.replies && shared.writable;
  const big = !gone && !sticker && !message.reply ? bigEmojiCount(text) : 0;

  // A card is asked for once, when the message comes on screen (and is younger than a week).
  const content = useRef<HTMLDivElement>(null);
  const wantsCard =
    Boolean(cardLink) && !message.card && sent && nowMs - message.at < REACTION_WINDOW_MS;
  useEffect(() => {
    const element = content.current;
    if (!wantsCard || !element || unfurled.has(message.key)) return;
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((entry) => entry.isIntersecting) || unfurled.has(message.key)) return;
      unfurled.add(message.key);
      observer.disconnect();
      void actions.onUnfurl(message).then((asked) => {
        if (asked) return;
        unfurlFailed.add(message.key);
        setUnfurlFailedNow((n) => n + 1);
      });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [wantsCard, message, actions]);

  const words: Record<MessageAction, MenuItem> = {
    react: { label: t("chat2.toolbar.react"), onSelect: () => {} },
    reply: {
      label: t("chat2.toolbar.reply"),
      icon: <ArrowBendUpLeft size={16} />,
      onSelect: () => actions.onReply(message),
    },
    copy: {
      label: t("chat.actions.copy"),
      icon: <Copy size={16} />,
      onSelect: () => {
        void copyText(message.text).then((copied) => {
          if (copied) toast.success(t("chat.actions.copied"));
        });
      },
    },
    copyLink: {
      label: t("chat2.links.copy"),
      icon: <LinkSimple size={16} />,
      onSelect: () => {
        if (!firstLink) return;
        void copyText(firstLink.url).then((copied) => {
          if (copied) toast.success(t("chat2.links.copied"));
        });
      },
    },
    edit: {
      label: t("chat.actions.edit"),
      icon: <PencilSimple size={16} />,
      onSelect: () => actions.onEdit(message),
    },
    delete: {
      label: t("chat.actions.delete"),
      icon: <Trash size={16} />,
      danger: true,
      onSelect: () => setAsking(true),
    },
    report: {
      label: t("chat.actions.report"),
      icon: <Flag size={16} />,
      onSelect: () => actions.onReport(message),
    },
  };
  const plan = gone
    ? []
    : messageMenu({
        reactable,
        repliable,
        copyable: !sticker,
        link: Boolean(firstLink),
        editable: mine && editable,
        deletable,
        report: !mine && message.id !== undefined,
      });
  // The quick reactions are the right click's top row; the rows follow.
  const items = plan
    .filter((entry) => entry?.action !== "react")
    .map((entry) => (entry ? words[entry.action] : null));
  const given = (emoji: string) => (reactions ?? []).some((r) => r.mine && sameEmoji(r.e, emoji));
  const strip: MenuStripItem[] = reactable
    ? shared.quick.map((emoji) => ({
        key: emoji,
        label: t("chat2.reactions.give", { emoji }),
        content: <span className="text-[19px] leading-none">{emoji}</span>,
        on: given(emoji),
        onSelect: () => actions.onReact(message, emoji, !given(emoji)),
      }))
    : [];

  const remove = async () => {
    setBusy(true);
    // The question goes as the tombstone comes; it comes back if the service refused.
    setAsking(false);
    const done = await actions.onDelete(message);
    setBusy(false);
    if (!done) setAsking(false);
  };

  const failure = message.failure ?? "connection";
  const failureWords =
    failure === "link" && message.failureRule
      ? t(`chat2.links.rules.${ruleWords(message.failureRule)}`)
      : CHAT2_FAILURES.has(failure)
        ? t(`chat2.failure.${failure}`)
        : t(`chat.failure.${failure}`);
  const stickerData = sticker ? findSticker(sticker) : undefined;
  const speaker = mine ? "" : `${sender.name}: `;

  // The words: the text fades out, the tombstone's line fades in (one after the other while the
  // bubble does not glide).
  const words2 = (
    <AnimatePresence mode={layout ? "popLayout" : "wait"} initial={false}>
      {gone ? (
        <motion.span
          key="gone"
          layout={layout ? "position" : false}
          className="relative flex items-center gap-1.5 px-3 py-[7px] whitespace-nowrap text-ash-300"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: { ...enter(0.22), delay: layout ? 0.12 : 0 } }}
          exit={{ opacity: 0, transition: leave(0.12) }}
        >
          <Prohibit size={14} weight="bold" className="shrink-0 text-ash-400" aria-hidden />
          <span className="sr-only">{speaker}</span>
          {tombstone}
        </motion.span>
      ) : (
        <motion.span
          key="text"
          layout={layout ? "position" : false}
          className="relative block px-3 py-[7px] break-words whitespace-pre-wrap text-bone-50 data-[theirs]:text-bone-100"
          data-theirs={mine ? undefined : true}
          data-selectable
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.2) }}
          exit={{ opacity: 0, transition: { duration: 0.1, ease: easeOutExpo } }}
        >
          <span className="sr-only">{speaker}</span>
          <MessageText text={text} links={links} />
          {cardLink && extras.links ? (
            <LinkCard
              link={cardLink}
              card={card}
              waiting={wantsCard || (unfurled.has(message.key) && !message.card)}
            />
          ) : null}
        </motion.span>
      )}
    </AnimatePresence>
  );

  // What stands in the bubble's place: a sticker, a few large emoji, or the bubble.
  const body =
    sticker && !gone ? (
      stickerData ? (
        <Sticker
          sticker={stickerData}
          size={112}
          arrive={fresh}
          alt={
            mine
              ? t("chat2.stickers.sentMine", { sticker: stickerData.name[lang] })
              : t("chat2.stickers.sent", { name: sender.name, sticker: stickerData.name[lang] })
          }
        />
      ) : (
        <UnknownSticker size={112} label={t("chat2.stickers.unknown")} />
      )
    ) : big ? (
      <p className="px-0.5 text-[32px] leading-[1.2] break-words" data-selectable>
        <span className="sr-only">{speaker}</span>
        {text}
      </p>
    ) : (
      <Bubble
        mine={mine}
        failed={failed}
        sending={message.state === "sending"}
        gone={gone}
        sizeKey={gone ? `gone:${tombstone}` : `text:${text}:${message.card?.state ?? ""}`}
      >
        {words2}
      </Bubble>
    );

  return (
    <ContextMenu items={items} strip={strip} disabled={gone}>
      <div
        data-key={message.key}
        className={cn(
          "group/msg relative flex items-end gap-2 px-1",
          mine ? "flex-row-reverse" : "flex-row",
          place.last ? "pb-2.5" : "pb-0.5",
        )}
      >
        <Flash flash={flash} />
        {mine ? null : (
          // The face sits by the group's last bubble, over the line with its time (and the
          // bubble's reactions, one 26 px line); a right click on it (or on the name) opens the
          // sender's own menu.
          <span
            className={cn(
              "flex w-6 shrink-0 self-end",
              place.last && (reactions?.length && !gone ? "mb-[41px]" : "mb-[15px]"),
            )}
          >
            {place.last ? (
              <PersonContextMenu person={{ id: message.from, ...sender }} place={personPlace}>
                <span className="flex">
                  <SocialFace source={sender} size={24} />
                </span>
              </PersonContextMenu>
            ) : null}
          </span>
        )}
        {/* The column leaves the toolbar's width free beside the bubble (92 px; with the
            face's 32 px for the others' messages): the toolbar never lies over anything. */}
        <div
          ref={content}
          className={cn(
            "relative flex min-w-0 flex-col",
            mine ? "max-w-[calc(100%-92px)] items-end" : "max-w-[calc(100%-124px)] items-start",
          )}
        >
          {place.crew && !mine && place.first ? (
            <PersonContextMenu person={{ id: message.from, ...sender }} place={personPlace}>
              <SenderName
                name={sender.name}
                badge={sender.badge}
                wearer={message.from}
                className="mb-1 ml-1 max-w-full text-[12px] font-semibold text-ash-300"
              />
            </PersonContextMenu>
          ) : null}
          {quoted && !gone ? (
            <QuoteBlock quoted={quoted} mine={mine} onJump={actions.onJump} />
          ) : null}
          {/* The bubble and, beside it in the row's free side, its toolbar. */}
          <div className="relative max-w-full">
            {body}
            {sent && !gone ? (
              <MessageToolbar
                mine={mine}
                reactable={reactable}
                repliable={repliable}
                quick={shared.quick}
                given={given}
                menu={items}
                onReact={(emoji) => actions.onReact(message, emoji, !given(emoji))}
                onReply={() => actions.onReply(message)}
              />
            ) : null}
          </div>
          {reactions?.length && !gone ? (
            <ReactionChips
              reactions={reactions}
              mine={mine}
              me={shared.me}
              people={shared.people}
              personPlace={personPlace}
              origin={message.id !== undefined ? { conv: shared.conv, id: message.id } : undefined}
              reactable={reactable}
              fresh
              onToggle={(emoji, on) => actions.onReact(message, emoji, on)}
            />
          ) : null}
          {place.last || seen || (!gone && (message.state !== "sent" || message.edited)) ? (
            <span
              className={cn(
                "mt-1 flex items-center gap-1.5 px-1 text-[11.5px] leading-none text-ash-400 tabular-nums",
                mine ? "flex-row-reverse" : "flex-row",
              )}
            >
              {mine && !gone ? <StateMark message={message} seen={seen} /> : null}
              {place.last || failed ? (
                <time dateTime={new Date(message.at).toISOString()}>{time}</time>
              ) : null}
              {message.edited && !gone ? <span>{t("chat.state.edited")}</span> : null}
            </span>
          ) : null}
          <AnimatePresence initial={false}>
            {failed ? (
              <motion.span
                key="failed"
                className="mt-1.5 flex flex-wrap items-center justify-end gap-x-2 gap-y-1 px-1"
                {...rise(reduce, 4, 0.2)}
              >
                <span className="text-[12px] text-flare-300">{failureWords}</span>
                {REWRITE.has(failure) && !sticker ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7"
                    onClick={() => actions.onRewrite(message)}
                  >
                    {t("chat.actions.edit")}
                  </Button>
                ) : (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7"
                    icon={<ArrowClockwise size={13} />}
                    onClick={() => actions.onRetry(message)}
                  >
                    {t("chat.actions.retry")}
                  </Button>
                )}
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-7"
                  onClick={() => actions.onDiscard(message)}
                >
                  {t("chat.actions.discard")}
                </Button>
              </motion.span>
            ) : null}
          </AnimatePresence>
          <AnimatePresence initial={false}>
            {asking && !gone ? (
              <motion.div
                key="ask"
                className={cn(
                  "mt-1.5 flex flex-col gap-2 px-1",
                  mine ? "items-end" : "items-start",
                )}
                initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-3px)" }}
                animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.2) }}
                exit={{ opacity: 0, transition: leave(0.14) }}
              >
                <p className="text-[12.5px] text-bone-100">{t("chat.actions.deleteAsk")}</p>
                <span className="flex gap-1.5">
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7"
                    onClick={() => setAsking(false)}
                  >
                    {t("chat.actions.cancel")}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    className="h-7"
                    disabled={busy}
                    onClick={() => void remove()}
                  >
                    {t("chat.actions.delete")}
                  </Button>
                </span>
              </motion.div>
            ) : null}
          </AnimatePresence>
        </div>
      </div>
    </ContextMenu>
  );
}

export const MessageRow = memo(MessageRowView);
