// The room chat's field (SQOL §1.3), chat's own in look and rules: a rounded field that grows to
// five lines, the emoji and "Küp" picker beside the send action (quiet while empty, gold once there
// is something to send), the same checks before anything goes (300 characters, five lines, phone
// numbers, the link rules, a limited account's links) with the same words under the field, the
// count near the limit. Enter sends, Shift+Enter starts a new line, PageUp and PageDown scroll the
// messages. No edits, replies or "yazıyor" here (SQOL §6). While the player cannot write (a
// moderator's chat mute, chat switched off) one quiet line takes its place, the same height.
import { Info, LockSimple, PaperPlaneRight } from "@phosphor-icons/react";
import { cn } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type FormEvent,
  type KeyboardEvent,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
} from "react";
import { useTranslation } from "react-i18next";
import { EmojiPickerButton, insertText, useEmojiPrefs } from "@/features/emoji";
import { useComposerState } from "@/features/social/chat/chat-page";
import { problemOf, problemWords } from "@/features/social/chat/composer";
import {
  useChatExtras,
  useChatHasExtras,
  useLinkRules,
  useStickerSets,
} from "@/features/social/chat/extras";
import { holdFocus } from "@/features/social/chat/focus";
import { CHAT_COUNT_FROM, CHAT_TEXT_MAX, chatLength, hasText } from "@/features/social/chat/text";
import { useChatTextSize } from "@/features/social/chat/text-size";
import { writingZoomOf } from "@/features/social/chat/text-zoom";
import { newAccount } from "@/features/social/identity";
import { limitOf } from "@/features/social/limits";
import { enter, leave, rise, useSocialMotion } from "@/features/social/motion";
import { useSocialUi } from "@/features/social/store";
import { sendRoomSticker, sendRoomText } from "./actions";
import { useRoomChat } from "./store";

/** The field's line height and padding: one line is 38 px, five lines 118 px (chat's). */
const LINE = 20;
const PAD = 9;

export function RoomComposer({
  focusRequest,
  onSent,
  onPage,
}: {
  /** Grows when the field should take the focus (the panel opened for the chat, "Düzenle"). */
  focusRequest: number;
  /** Something was sent: the list goes to its newest message. */
  onSent: () => void;
  /** PageUp (-1) or PageDown (1) in the field: the messages scroll a screen. */
  onPage: (direction: 1 | -1) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const draft = useRoomChat((s) => s.draft);
  const setDraft = useRoomChat((s) => s.setDraft);
  const extras = useChatExtras();
  const links = useLinkRules();
  const stickerSets = useStickerSets();
  const hasExtras = useChatHasExtras();
  const limitedLinks = useSocialUi(
    (s) => limitOf(s.view.me, "chatLinks", Date.now())?.until ?? null,
  );
  const accountNew = useSocialUi((s) => newAccount(s.view.me));
  // A moderator's chat mute or chat switched off close it, with chat's own words.
  const state = useComposerState("", true, false);
  const writingZoom = useChatTextSize((s) => writingZoomOf(s.size));
  const field = useRef<HTMLTextAreaElement>(null);
  const selection = useRef({ start: 0, end: 0 });
  const lineId = useId();

  useEffect(() => {
    if (!focusRequest || !field.current) return;
    return holdFocus(field.current);
  }, [focusRequest]);

  // The field grows with its lines (up to five), measured before paint.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measured when the text or its size changes
  useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    const max = LINE * 5 + PAD * 2;
    element.style.height = `${Math.min(max, Math.max(LINE + PAD * 2, element.scrollHeight))}px`;
  }, [draft, writingZoom]);

  const problem = problemOf(draft, links, hasExtras, limitedLinks);
  const length = chatLength(draft);
  const ready = hasText(draft) && !problem;
  const stickers = extras.stickers && stickerSets.includes("pl1");

  const remember = () => {
    const element = field.current;
    if (element) selection.current = { start: element.selectionStart, end: element.selectionEnd };
  };

  // An emoji goes in at the caret (over a selection), the caret after it; the picker stays open.
  const putEmoji = (emoji: string) => {
    const { start, end } = selection.current;
    const next = insertText(draft, start, end, emoji, CHAT_TEXT_MAX);
    if (!next) return;
    setDraft(next.value);
    selection.current = { start: next.caret, end: next.caret };
    window.requestAnimationFrame(() => field.current?.setSelectionRange(next.caret, next.caret));
  };

  const sendNow = () => {
    if (!ready) return;
    const text = draft;
    setDraft("");
    onSent();
    void sendRoomText(text);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      sendNow();
      return;
    }
    const pageKey = event.key === "PageUp" ? -1 : event.key === "PageDown" ? 1 : 0;
    if (pageKey && !event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey) {
      event.preventDefault();
      onPage(pageKey);
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    sendNow();
    // Sent with the button: the field keeps the focus for the next message.
    field.current?.focus({ preventScroll: true });
  };

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

  const reason =
    problem === null
      ? null
      : problemWords(problem, t, { hasExtras, accountNew, nowMs: Date.now() });
  const showCount = length >= CHAT_COUNT_FROM;

  return (
    <form className="shrink-0 px-1 pt-2 pb-3" onSubmit={submit}>
      <div
        className={cn(
          "relative flex items-end gap-0.5 rounded-[var(--radius-panel)] border bg-obsidian-800/80 pr-[3px] transition-[border-color,box-shadow] duration-150",
          "border-white/10 hover:border-white/18 focus-within:border-gold-400/60 focus-within:ring-2 focus-within:ring-gold-400/20",
          reason &&
            "border-flare-400/55 focus-within:border-flare-400/70 focus-within:ring-flare-400/20",
        )}
      >
        <textarea
          ref={field}
          rows={1}
          value={draft}
          onChange={(event) => {
            setDraft(event.target.value);
            remember();
          }}
          onKeyDown={onKeyDown}
          onSelect={remember}
          onKeyUp={remember}
          onClick={remember}
          onBlur={remember}
          aria-label={t("roomChat.composer.label")}
          aria-invalid={reason ? true : undefined}
          aria-describedby={reason || showCount ? lineId : undefined}
          placeholder={t("roomChat.composer.placeholder")}
          maxLength={4_000}
          className="min-w-0 flex-1 resize-none bg-transparent py-[9px] pl-3.5 text-[13.5px] leading-5 text-bone-50 outline-none [scrollbar-width:thin] placeholder:text-ash-500"
          style={{ height: LINE + PAD * 2, zoom: writingZoom }}
        />
        <span className="mb-[3px] flex">
          <EmojiPickerButton
            onEmoji={(emoji) => {
              putEmoji(emoji);
              useEmojiPrefs.getState().usedEmoji(emoji);
            }}
            onSticker={
              stickers
                ? (sticker) => {
                    onSent();
                    void sendRoomSticker(sticker.ref);
                  }
                : undefined
            }
            returnFocus={() => field.current}
          />
        </span>
        <button
          type="submit"
          disabled={!ready}
          // A press leaves the focus (and the caret) in the field.
          onMouseDown={(event) => event.preventDefault()}
          aria-label={t("chat.actions.send")}
          className={cn(
            "group/send relative mb-[3px] inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-md outline-none",
            "transition-[transform,scale,color] duration-150 ease-out active:scale-[0.94] focus-visible:ring-2 focus-visible:ring-gold-300",
            ready ? "text-gold-ink" : "cursor-default text-ash-500",
          )}
        >
          {/* The gold face fades in once there is something to send. */}
          <span
            aria-hidden
            className={cn(
              "absolute inset-0 rounded-md bg-[linear-gradient(180deg,var(--color-gold-300),var(--color-gold-500)_55%,var(--color-gold-600))] transition-[opacity,filter] duration-150 group-hover/send:brightness-110",
              ready ? "opacity-100" : "opacity-0",
            )}
          />
          <span className="relative flex">
            <PaperPlaneRight size={17} weight={ready ? "fill" : "regular"} aria-hidden />
          </span>
        </button>
      </div>
      {/* As tall as one line of words, empty or not: a reason coming and going moves nothing. */}
      <div
        id={lineId}
        className="flex min-h-[24px] items-start justify-between gap-3 px-1 pt-1.5 text-[12px]"
      >
        <span className="relative min-w-0 flex-1">
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
