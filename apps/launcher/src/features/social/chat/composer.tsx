// The composer: a rounded field that grows to five lines, the send action inside it (quiet while
// empty, gold once there is something to send), the reason in place when a text cannot go (a link,
// a phone number, too long), the count near the limit, and in crew chat four quick replies that go
// with one press. Enter sends, Shift+Enter starts a new line, ↑ in an empty field edits the last
// message the player may still edit, Esc leaves an edit. When the player cannot write here (a
// moderator's mute, chat switched off, no longer friends or in the crew) one quiet line takes its
// place, the same height. An edit (from a message's menu, a right click or ↑) and "Düzenle" on a
// message that did not go put the caret at the end of the field (FIX2).
import { Check, Info, LockSimple, PaperPlaneRight, PencilSimple, X } from "@phosphor-icons/react";
import { cn, IconButton } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type FormEvent,
  type KeyboardEvent,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { ChatMessage } from "@/lib/api/social-types";
import { inside, leaveOnEscape } from "../edit-escape";
import { enter, leave, rise, useSocialMotion } from "../motion";
import { useChat } from "./chat-store";
import { holdFocus } from "./focus";
import { CHAT_COUNT_FROM, CHAT_TEXT_MAX, chatLength, hasText, textProblem } from "./text";

/** What the composer may do here. */
export type ComposerState =
  | { kind: "open" }
  /** One line in the composer's place (why the player cannot write). */
  | { kind: "closed"; reason: string; icon?: "lock" | "info" };

const CHIPS = ["coming", "fiveMinutes", "where", "come"] as const;

/** The field's line height and padding: one line is 38 px, five lines 118 px. */
const LINE = 20;
const PAD = 9;

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
   * came back to it). */
  focusRequest?: number;
  onCancelEdit: () => void;
  /** ↑ in an empty field: the last message the player may still edit, if any. */
  onEditLast: () => void;
  /** Something was sent: the list goes to its newest message. */
  onSent: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const draft = useChat((s) => s.drafts[conv] ?? "");
  const setDraft = useChat((s) => s.setDraft);
  const setEdit = useChat((s) => s.setEdit);
  const [editText, setEditText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null);
  const text = editing ? editText : draft;

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

  // A failed message's text came back to the field: the caret waits at its end.
  useEffect(() => {
    if (!focusRequest || !field.current) return;
    return holdFocus(field.current);
  }, [focusRequest]);

  // Esc during an edit (in the field, on "Vazgeç" or on the save button) leaves the edit, never
  // the panel around it (FIX2: the friends panel closed instead; edit-escape.ts). The field keeps
  // the focus: "Vazgeç" goes away with the edit.
  const cancelEdit = useRef(onCancelEdit);
  cancelEdit.current = onCancelEdit;
  useEffect(() => {
    if (!editing) return;
    return leaveOnEscape(
      (target) => inside(field.current?.form, target),
      () => {
        cancelEdit.current();
        field.current?.focus({ preventScroll: true });
      },
    );
  }, [editing]);

  // The field grows with its lines (up to five), measured before paint.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measured when the text changes
  useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    const max = LINE * 5 + PAD * 2;
    element.style.height = `${Math.min(max, Math.max(LINE + PAD * 2, element.scrollHeight))}px`;
  }, [text]);

  const problem = textProblem(text);
  const length = chatLength(text);
  // An edit with its text unchanged has nothing to save.
  const ready = hasText(text) && !problem && !busy && (!editing || text.trim() !== editing.text);

  const change = (value: string) => {
    setError(null);
    if (editing) setEditText(value);
    else setDraft(conv, value);
  };

  const send = async (value: string, fromChip = false) => {
    if (!hasText(value) || textProblem(value)) return;
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
    if (!fromChip) setDraft(conv, "");
    onSent();
    try {
      await api.social.chatSend(conv, value);
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
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (ready) void send(text);
  };

  const reason =
    error ??
    (problem === "link"
      ? t("chat.composer.link")
      : problem === "phone"
        ? t("chat.composer.phone")
        : problem === "long"
          ? t("chat.composer.long")
          : null);
  const showCount = length >= CHAT_COUNT_FROM;

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
      {crew && !editing ? (
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
        ) : null}
      </AnimatePresence>
      <div
        className={cn(
          "flex items-end gap-1 rounded-[var(--radius-panel)] border bg-obsidian-800/80 pr-[3px] transition-[border-color,box-shadow] duration-150",
          "border-white/10 hover:border-white/18 focus-within:border-gold-400/60 focus-within:ring-2 focus-within:ring-gold-400/20",
          reason &&
            "border-flare-400/55 focus-within:border-flare-400/70 focus-within:ring-flare-400/20",
        )}
      >
        <textarea
          ref={field}
          rows={1}
          value={text}
          onChange={(event) => change(event.target.value)}
          onKeyDown={onKeyDown}
          aria-label={t("chat.composer.label")}
          aria-invalid={reason ? true : undefined}
          aria-describedby={reason || showCount ? `composer-${conv}` : undefined}
          placeholder={crew ? t("chat.composer.placeholderCrew") : t("chat.composer.placeholder")}
          maxLength={4_000}
          className="min-w-0 flex-1 resize-none bg-transparent py-[9px] pl-3.5 text-[13.5px] leading-5 text-bone-50 outline-none placeholder:text-ash-500 [scrollbar-width:thin]"
          style={{ height: LINE + PAD * 2 }}
        />
        <button
          type="submit"
          disabled={!ready}
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
      <div
        id={`composer-${conv}`}
        className="flex min-h-[20px] items-start justify-between gap-3 px-1 pt-1.5 text-[12px]"
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
