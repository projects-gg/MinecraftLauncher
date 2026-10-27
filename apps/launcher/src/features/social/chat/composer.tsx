// The composer: a rounded field that grows to five lines, the send action inside it (quiet while
// empty, gold once there is something to send), the reason in place when a text cannot go (a link,
// a phone number, too long), the count near the limit, and in crew chat four quick replies that go
// with one press. Enter sends, Shift+Enter starts a new line, ↑ in an empty field edits the last
// message the player may still edit, Esc leaves an edit. When the player cannot write here (a
// moderator's mute, chat switched off, no longer friends or in the crew) one quiet line takes its
// place, the same height. An edit (from a message's menu, a right click or ↑) and "Düzenle" on a
// message that did not go put the caret at the end of the field (FIX2).
//
// CHAT2: the emoji button left of send opens the picker; an emoji goes in at the caret (the
// picker stays open) and a sticker goes at once (it closes). A reply shows a strip over the field
// ("Yanıtlanıyor: Ayşe", one line of the message, ✕; Esc does the same). Links the rules let
// through can go; for one that cannot, the reason says which rule ("Kısaltılmış bağlantılar
// gönderilemez."); a link the player writes shows its card over the field after a moment. While
// the player writes, "yazıyor" goes out (typing.ts).
//
// CHAT-PROD: the words in the field grow with the chat's text size (never shrink below 100 %); the
// send button leaves the focus in the field (the next message can be written at once); PageUp and
// PageDown scroll the messages.
import { Check, Info, LockSimple, PaperPlaneRight, PencilSimple, X } from "@phosphor-icons/react";
import type { Sticker as StickerData } from "@pl/stickers";
import { cn, IconButton } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { friendlyError } from "@/features/accounts/use-accounts";
import { EmojiPickerButton, insertText, useEmojiPrefs } from "@/features/emoji";
import { api } from "@/lib/api";
import type { ChatCard, ChatLinkRules, ChatMessage } from "@/lib/api/social-types";
import { inside, leaveOnEscape } from "../edit-escape";
import { limitOf, opensIn } from "../limits";
import { enter, leave, rise, useSocialMotion } from "../motion";
import { useSocialUi } from "../store";
import { useChat } from "./chat-store";
import { useChatExtras, useChatHasExtras, useLinkRules, useStickerSets } from "./extras";
import { holdFocus } from "./focus";
import { domainOf, ruleWords, SiteIcon, shapeOf } from "./link-ui";
import { type Quoted, ReplyStrip } from "./reply";
import {
  CHAT_COUNT_FROM,
  CHAT_TEXT_MAX,
  chatLength,
  hasText,
  messageLinks,
  messageProblem,
} from "./text";
import { useChatTextSize } from "./text-size";
import { writingZoomOf } from "./text-zoom";
import { typingSender } from "./typing";

/** What the composer may do here. */
export type ComposerState =
  | { kind: "open" }
  /** One line in the composer's place (why the player cannot write). */
  | { kind: "closed"; reason: string; icon?: "lock" | "info" };

const CHIPS = ["coming", "fiveMinutes", "where", "come"] as const;

/** The field's line height and padding: one line is 38 px, five lines 118 px. */
const LINE = 20;
const PAD = 9;
/** A link the player writes shows its card after this pause. */
const CARD_AFTER_MS = 600;

/**
 * Why a text cannot go: CHAT1's reasons (`words`: only 🖕, the word list stays the service's), a
 * link with the rule it fails (CHAT2), links from a limited account (until when, Unix seconds),
 * or none.
 */
type Problem = "phone" | "long" | "words" | { link: string } | { limited: number } | null;

/** A bare domain in the text ("youtube.com/x"): the refusal adds "https:// ile yaz.". */
const BARE = /(?<![\p{L}\p{N}@.\-/])[a-z0-9-]+(?:\.[a-z0-9-]+)*\.[a-z]{2,24}(?![\p{L}\p{N}])/iu;

/**
 * The composer's check. Without links on, any address is a link that cannot go (CHAT1). With
 * them, the links that pass may go and the rest of the text is checked without them; a link that
 * fails says why when its rule has words of its own.
 */
function problemOf(
  value: string,
  links: ChatLinkRules | undefined,
  extras: boolean,
  limitedUntil: number | null,
): Problem {
  const problem = messageProblem(value, links ?? null);
  if (problem?.reason === "link") {
    // An older service says CHAT1's sentence; with the extras, links switched off say so.
    if (problem.rule === "off") return { link: extras ? "off" : "form" };
    if (problem.rule === "form") return { link: BARE.test(value) ? "formHint" : "form" };
    return { link: problem.rule };
  }
  if (problem) return problem.reason;
  // A limited account sends no links (the service: `account_limited` with `chat_links`).
  if (limitedUntil !== null && links && messageLinks(value, links).length > 0) {
    return { limited: limitedUntil };
  }
  return null;
}

/** A link's card over the field while the player writes it (what the others will see). */
function ComposerCard({ card }: { card: ChatCard }) {
  const { reduce } = useSocialMotion();
  return (
    <motion.div
      className="mb-1.5 flex items-center gap-2 overflow-hidden rounded-[8px] bg-obsidian-800/70 py-1.5 pr-2 pl-2.5 ring-1 ring-white/6 ring-inset"
      initial={{ opacity: 0, transform: reduce ? "none" : "translateY(3px)" }}
      animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.2) }}
      exit={{ opacity: 0, transition: leave(0.14) }}
    >
      {card.image ? (
        <img
          src={card.image}
          alt=""
          draggable={false}
          className={cn(
            "h-8 shrink-0 rounded-[4px] object-cover",
            shapeOf(card) === "banner" ? "w-14" : "w-8",
          )}
        />
      ) : null}
      <span className="min-w-0 flex-1 text-[12px] leading-[1.35]">
        <span className="flex min-w-0 items-center gap-1.5 text-ash-400">
          <SiteIcon icon={card.icon} />
          <span className="truncate">{card.site}</span>
        </span>
        <span className="block truncate font-semibold text-bone-100">
          {card.title ?? domainOf(new URL(card.url).hostname)}
        </span>
      </span>
    </motion.div>
  );
}

/** Cards of links the player wrote, kept for the session (asked once each). */
const writtenCards = new Map<string, ChatCard | null>();

export function Composer({
  conv,
  crew,
  state,
  editing,
  notice,
  focusRequest = 0,
  onCancelEdit,
  onEditLast,
  onSent,
  onPage,
  status = null,
  replyTo = null,
  replyQuote = null,
  onCancelReply = () => {},
  othersOnline = true,
}: {
  conv: string;
  /** Crew chat: the quick replies show. */
  crew: boolean;
  state: ComposerState;
  /** The player's message being edited, if any. */
  editing: ChatMessage | null;
  /** A quiet line over the field (the mailbox is off, the connection is waiting). */
  notice: string | null;
  /** Grows when the field should take the focus, the caret at the end (a failed message's text
   * came back to it, a reply started). */
  focusRequest?: number;
  onCancelEdit: () => void;
  /** ↑ in an empty field: the last message the player may still edit, if any. */
  onEditLast: () => void;
  /** Something was sent: the list goes to its newest message. */
  onSent: () => void;
  /** PageUp (-1) or PageDown (1) in the field: the messages scroll a screen. */
  onPage?: (direction: 1 | -1) => void;
  /** A quiet line under the field while nothing is wrong (the text size as it changes). */
  status?: ReactNode;
  /** CHAT2: someone else in the conversation is online ("yazıyor…" goes only then). */
  othersOnline?: boolean;
  /** CHAT2: the message the next one answers (sent: it has its id). */
  replyTo?: ChatMessage | null;
  replyQuote?: Quoted | null;
  onCancelReply?: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const draft = useChat((s) => s.drafts[conv] ?? "");
  const setDraft = useChat((s) => s.setDraft);
  const setEdit = useChat((s) => s.setEdit);
  const extras = useChatExtras();
  const links = useLinkRules();
  const stickerSets = useStickerSets();
  const [editText, setEditText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null);
  const selection = useRef({ start: 0, end: 0 });
  const text = editing ? editText : draft;
  const typing = useMemo(() => typingSender(conv), [conv]);
  const hasExtras = useChatHasExtras();
  // "Yazıyor bilgisi" works both ways: off, the player's own writing is not told either.
  const shareTyping = useSocialUi((s) => s.view.me?.privacy.shareTyping !== false);
  const limitedLinks = useSocialUi(
    (s) => limitOf(s.view.me, "chatLinks", Date.now())?.until ?? null,
  );
  // The field's words follow the chat's text size (a zoom: its lines, padding and height in its
  // own pixels stay as measured below).
  const writingZoom = useChatTextSize((s) => writingZoomOf(s.size));
  // A reply waits only while the player writes a new message (an edit keeps its own link).
  const reply = editing ? null : replyTo;

  // An edit starts with the message's text, the caret at its end, however it started: the menu
  // that started it may close after this and must not take the focus away (focus.ts). An edit
  // that ends leaves nothing of its text behind.
  useEffect(() => {
    if (!editing) {
      setEditText("");
      return;
    }
    setEditText(editing.text);
    setError(null);
    return field.current ? holdFocus(field.current) : undefined;
  }, [editing]);

  // A failed message's text came back to the field, or a reply started: the caret waits at its
  // end.
  useEffect(() => {
    if (!focusRequest || !field.current) return;
    return holdFocus(field.current);
  }, [focusRequest]);

  // Esc during an edit (in the field, on "Vazgeç" or on the save button) leaves the edit, never
  // the panel around it (FIX2: the friends panel closed instead; edit-escape.ts). The field keeps
  // the focus: "Vazgeç" goes away with the edit. Esc while replying takes the reply away.
  const cancelEdit = useRef(onCancelEdit);
  cancelEdit.current = onCancelEdit;
  const cancelReply = useRef(onCancelReply);
  cancelReply.current = onCancelReply;
  useEffect(() => {
    if (!editing && !reply) return;
    return leaveOnEscape(
      (target) => inside(field.current?.form, target),
      () => {
        if (editing) cancelEdit.current();
        else cancelReply.current();
        field.current?.focus({ preventScroll: true });
      },
    );
  }, [editing, reply]);

  // The field grows with its lines (up to five), measured before paint; a bigger text size wraps
  // the words sooner.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measured when the text or its size changes
  useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    const max = LINE * 5 + PAD * 2;
    element.style.height = `${Math.min(max, Math.max(LINE + PAD * 2, element.scrollHeight))}px`;
  }, [text, writingZoom]);

  const problem = problemOf(text, links, hasExtras, limitedLinks);
  const length = chatLength(text);
  // An edit with its text unchanged has nothing to save.
  const ready = hasText(text) && !problem && !busy && (!editing || text.trim() !== editing.text);

  // The card of the link being written, after a pause (only links that may go).
  const [card, setCard] = useState<ChatCard | null>(null);
  const firstLink = !problem && links ? messageLinks(text, links).find((l) => l.ok) : undefined;
  const cardUrl = firstLink?.url;
  useEffect(() => {
    if (!cardUrl || editing) {
      setCard(null);
      return;
    }
    if (writtenCards.has(cardUrl)) {
      setCard(writtenCards.get(cardUrl) ?? null);
      return;
    }
    let alive = true;
    const timer = window.setTimeout(() => {
      void api.social
        .chatCard(cardUrl)
        .catch(() => null)
        .then((found) => {
          writtenCards.set(cardUrl, found);
          if (alive) setCard(found);
        });
    }, CARD_AFTER_MS);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [cardUrl, editing]);

  const remember = () => {
    const element = field.current;
    if (element) selection.current = { start: element.selectionStart, end: element.selectionEnd };
  };

  const change = (value: string) => {
    setError(null);
    if (editing) setEditText(value);
    else setDraft(conv, value);
    typing.wrote({
      text: value,
      focused: document.activeElement === field.current,
      editing: Boolean(editing),
      on: extras.typing && shareTyping && othersOnline,
    });
  };

  // An emoji goes in at the caret (over a selection), the caret after it; the picker stays open.
  const putEmoji = (emoji: string) => {
    const current = editing ? editText : draft;
    const { start, end } = selection.current;
    const next = insertText(current, start, end, emoji, CHAT_TEXT_MAX);
    if (!next) return;
    change(next.value);
    selection.current = { start: next.caret, end: next.caret };
    window.requestAnimationFrame(() => field.current?.setSelectionRange(next.caret, next.caret));
  };

  const sendSticker = async (sticker: StickerData) => {
    setError(null);
    const answering = reply?.id;
    onCancelReply();
    onSent();
    typing.sent();
    try {
      await api.social.chatSendSticker(conv, sticker.ref, answering);
    } catch (failure) {
      setError(friendlyError(failure, t("chat.failure.connection")).message);
    }
  };

  const send = async (value: string, fromChip = false) => {
    if (!hasText(value) || problemOf(value, links, hasExtras, limitedLinks)) return;
    setError(null);
    if (editing) {
      if (editing.id === undefined) return;
      const id = editing.id;
      setBusy(true);
      // Shown at once; the service's answer settles it (or puts the old text back).
      setEdit(editing.key, value);
      onCancelEdit();
      try {
        await api.social.chatEdit(conv, id, value);
      } catch (failure) {
        setEdit(editing.key, null);
        setError(friendlyError(failure, t("chat.failure.connection")).message);
      } finally {
        setBusy(false);
      }
      return;
    }
    const answering = fromChip ? undefined : reply?.id;
    if (!fromChip) {
      setDraft(conv, "");
      onCancelReply();
    }
    onSent();
    typing.sent();
    try {
      await api.social.chatSend(conv, value, answering);
    } catch (failure) {
      // Nothing went: the text comes back to the field with the reason.
      if (!fromChip) setDraft(conv, value);
      setError(friendlyError(failure, t("chat.failure.connection")).message);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      if (ready) void send(text);
      return;
    }
    if (event.key === "ArrowUp" && !editing && !text) {
      event.preventDefault();
      onEditLast();
      return;
    }
    const pageKey = event.key === "PageUp" ? -1 : event.key === "PageDown" ? 1 : 0;
    if (pageKey && onPage && !event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey) {
      event.preventDefault();
      onPage(pageKey);
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (ready) void send(text);
    // Sent with the button: the field keeps the focus for the next message.
    field.current?.focus({ preventScroll: true });
  };

  const reason =
    error ??
    (problem === null
      ? null
      : problem === "phone"
        ? t("chat.composer.phone")
        : problem === "long"
          ? t("chat.composer.long")
          : problem === "words"
            ? t("chat.failure.words")
            : "limited" in problem
              ? t("chat2.links.limited", { when: opensIn(t, problem.limited, Date.now()) })
              : hasExtras || problem.link !== "form"
                ? t(`chat2.links.rules.${ruleWords(problem.link)}`)
                : t("chat.composer.link"));
  const showCount = length >= CHAT_COUNT_FROM;
  const stickers = extras.stickers && stickerSets.includes("pl1") && !editing;

  if (state.kind === "closed") {
    return (
      <div className="shrink-0 px-1 pt-2 pb-3">
        <motion.p
          key={state.reason}
          className="flex min-h-[38px] items-center gap-2.5 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3.5 py-2 text-[13px] text-ash-300 ring-1 ring-white/8"
          role="status"
          {...rise(reduce, 3, 0.2)}
        >
          {state.icon === "info" ? (
            <Info size={16} className="shrink-0 text-ash-400" aria-hidden />
          ) : (
            <LockSimple size={16} className="shrink-0 text-ash-400" aria-hidden />
          )}
          {state.reason}
        </motion.p>
      </div>
    );
  }

  return (
    <form className="shrink-0 px-1 pt-2 pb-3" onSubmit={submit}>
      <AnimatePresence initial={false}>
        {notice ? (
          <motion.p
            key={notice}
            className="mb-2 flex items-center gap-2 px-1 text-[12px] text-ash-300"
            role="status"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            <Info size={14} className="shrink-0 text-ash-400" aria-hidden />
            {notice}
          </motion.p>
        ) : null}
      </AnimatePresence>
      {crew && !editing && !reply ? (
        <fieldset
          aria-label={t("chat.chips.label")}
          className={cn(
            "m-0 mb-2 flex min-w-0 flex-wrap gap-1.5 border-0 p-0 transition-opacity duration-150",
            draft ? "pointer-events-none opacity-0" : "opacity-100",
          )}
          aria-hidden={draft ? true : undefined}
        >
          {CHIPS.map((chip) => (
            <button
              key={chip}
              type="button"
              tabIndex={draft ? -1 : undefined}
              onClick={() => void send(t(`chat.chips.${chip}`), true)}
              className="h-7 cursor-pointer rounded-md bg-white/[0.06] px-2.5 text-[12.5px] font-medium text-ash-300 outline-none ring-1 ring-white/8 transition-[background-color,color,transform,scale] duration-150 hover:bg-white/10 hover:text-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.96]"
            >
              {t(`chat.chips.${chip}`)}
            </button>
          ))}
        </fieldset>
      ) : null}
      <AnimatePresence initial={false}>
        {editing ? (
          <motion.div
            key="editing"
            className="mb-1.5 flex items-center gap-2 px-1 text-[12px] text-gold-300"
            initial={{ opacity: 0, transform: reduce ? "none" : "translateY(3px)" }}
            animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            <PencilSimple size={13} weight="bold" aria-hidden />
            <span className="flex-1">{t("chat.composer.editing")}</span>
            <IconButton
              label={t("chat.actions.cancel")}
              icon={<X size={14} />}
              size="sm"
              className="-my-1"
              onClick={onCancelEdit}
            />
          </motion.div>
        ) : reply && replyQuote ? (
          <ReplyStrip key={`reply:${reply.key}`} quoted={replyQuote} onCancel={onCancelReply} />
        ) : null}
      </AnimatePresence>
      <AnimatePresence initial={false}>
        {card && !editing ? (
          card.state === "refused" ? (
            // The link leads somewhere the rules refuse: the others would see plain text.
            <motion.p
              key={card.url}
              className="mb-1.5 px-1 text-[12px] text-ash-400"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: enter(0.2) }}
              exit={{ opacity: 0, transition: leave(0.14) }}
            >
              {t("chat2.links.refused")}
            </motion.p>
          ) : (
            <ComposerCard key={card.url} card={card} />
          )
        ) : null}
      </AnimatePresence>
      <div
        className={cn(
          "flex items-end gap-0.5 rounded-[var(--radius-panel)] border bg-obsidian-800/80 pr-[3px] transition-[border-color,box-shadow] duration-150",
          "border-white/10 hover:border-white/18 focus-within:border-gold-400/60 focus-within:ring-2 focus-within:ring-gold-400/20",
          reason &&
            "border-flare-400/55 focus-within:border-flare-400/70 focus-within:ring-flare-400/20",
        )}
      >
        <textarea
          ref={field}
          rows={1}
          value={text}
          onChange={(event) => {
            change(event.target.value);
            remember();
          }}
          onKeyDown={onKeyDown}
          onSelect={remember}
          onKeyUp={remember}
          onClick={remember}
          onBlur={remember}
          aria-label={t("chat.composer.label")}
          aria-invalid={reason ? true : undefined}
          aria-describedby={reason || showCount ? `composer-${conv}` : undefined}
          placeholder={crew ? t("chat.composer.placeholderCrew") : t("chat.composer.placeholder")}
          maxLength={4_000}
          className="min-w-0 flex-1 resize-none bg-transparent py-[9px] pl-3.5 text-[13.5px] leading-5 text-bone-50 outline-none placeholder:text-ash-500 [scrollbar-width:thin]"
          style={{ height: LINE + PAD * 2, zoom: writingZoom }}
        />
        <span className="mb-[3px] flex">
          <EmojiPickerButton
            onEmoji={(emoji) => {
              putEmoji(emoji);
              useEmojiPrefs.getState().usedEmoji(emoji);
            }}
            onSticker={stickers ? (sticker) => void sendSticker(sticker) : undefined}
            returnFocus={() => field.current}
          />
        </span>
        <button
          type="submit"
          disabled={!ready}
          // A press leaves the focus (and the caret) in the field.
          onMouseDown={(event) => event.preventDefault()}
          aria-label={editing ? t("chat.actions.save") : t("chat.actions.send")}
          className={cn(
            "group/send relative mb-[3px] inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-md outline-none",
            "transition-[transform,scale,color] duration-150 ease-out active:scale-[0.94] focus-visible:ring-2 focus-visible:ring-gold-300",
            ready ? "text-gold-ink" : "cursor-default text-ash-500",
          )}
        >
          {/* The gold face fades in once there is something to send (a gradient cannot fade by
              itself). */}
          <span
            aria-hidden
            className={cn(
              "absolute inset-0 rounded-md bg-[linear-gradient(180deg,var(--color-gold-300),var(--color-gold-500)_55%,var(--color-gold-600))] transition-[opacity,filter] duration-150 group-hover/send:brightness-110",
              ready ? "opacity-100" : "opacity-0",
            )}
          />
          <span className="relative flex">
            {editing ? (
              <Check size={17} weight="bold" aria-hidden />
            ) : (
              <PaperPlaneRight size={17} weight={ready ? "fill" : "regular"} aria-hidden />
            )}
          </span>
        </button>
      </div>
      {/* As tall as one line of words, empty or not: a reason or the text size coming and going
          moves nothing. */}
      <div
        id={`composer-${conv}`}
        className="flex min-h-[24px] items-start justify-between gap-3 px-1 pt-1.5 text-[12px]"
      >
        <span className="relative min-w-0 flex-1">
          {reason ? null : <span className="absolute inset-x-0 top-0">{status}</span>}
          <AnimatePresence initial={false}>
            {reason ? (
              <motion.span
                key={reason}
                className="block text-flare-300"
                role="alert"
                initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-2px)" }}
                animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.18) }}
                exit={{ opacity: 0, transition: leave(0.12) }}
              >
                {reason}
              </motion.span>
            ) : null}
          </AnimatePresence>
        </span>
        <span
          className={cn(
            "shrink-0 tabular-nums transition-opacity duration-150",
            showCount ? "opacity-100" : "opacity-0",
            length > CHAT_TEXT_MAX ? "text-flare-300" : "text-ash-400",
          )}
          aria-hidden={!showCount}
        >
          {t("chat.composer.count", { count: length })}
        </span>
      </div>
    </form>
  );
}
