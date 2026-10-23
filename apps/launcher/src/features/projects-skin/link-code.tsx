// The link code box (skin1.md §13.3): the six digits `/discord eşle` shows in game. One real field
// (numeric, one-time code, read as "Eşleme kodu, 6 hane") drawn as six cells; pasting keeps the
// digits only and the sixth digit sends it.
// - A gold frame slides from cell to cell on the sliding markers' spring; a typed digit grows in
//   from 92% (120 ms).
// - Sending: the cells dim and a small notch turns in the last one, only while the job runs.
// - Wrong: the box shakes once (260 ms; not under reduced motion), the cells' lines turn flare,
//   the reason fades in under them, the cells empty and typing starts again from the first.
// - Linked: a gold plate with a check comes over the cells (200 ms), then the caller swaps the box
//   for the linked line.
import { Check, CircleNotch } from "@phosphor-icons/react";
import { cn, easeOutExpo, springSnappy } from "@pl/ui";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { type ClipboardEvent, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { projectsSkinApi } from "@/lib/api/projects-skin-api";
import { useProjectsSkinUi } from "./queries";
import {
  type LinkOutcome,
  linkRefusalOutcome,
  linkResultOutcome,
  type Note,
  PIN_LENGTH,
  pinDigits,
  waitWords,
} from "./rules";
import { NoteText } from "./text";

const CELL = 42;
const GAP = 8;
const SHAKE = [0, -6, 6, -4, 4, 0].map((x) => ({ transform: `translateX(${x}px)` }));
/** How long the gold plate shows before the box hands over to the linked line. */
const LINKED_HOLD_MS = 500;

type Phase = "idle" | "sending" | "linked";

export function LinkCode({
  accountId,
  disabled = false,
  retryAt,
  autoFocus = false,
  describedBy,
  onSuccess,
  onLinked,
  onOther,
}: {
  /** The launcher account to link (its name goes with the code). */
  accountId: string;
  /** The service cannot be reached: shown, not usable. */
  disabled?: boolean;
  /** Link tries are used up until then (Unix seconds). */
  retryAt?: number | null;
  autoFocus?: boolean;
  /** The id of the words that say what to type. */
  describedBy?: string;
  /** The link is made: the gold plate shows now (the caller keeps the box until `onLinked`). */
  onSuccess?: () => void;
  /** After the gold plate: the caller shows the linked line. */
  onLinked: (name: string) => void;
  /** The Discord account turned out to be linked to another game account. */
  onOther: (name: string) => void;
}) {
  const { t } = useTranslation();
  const calm = useCalm();
  const reduced = useReducedMotion();
  const errorId = useId();
  const input = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const [value, setValue] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<Note | null>(null);
  const [focused, setFocused] = useState(false);
  const holdCodeBox = useProjectsSkinUi((s) => s.holdCodeBox);
  // The tries this box used up itself: closed at once, whether or not the caller knows the time
  // (the account center's row does not; the panel learns it only when it reads again).
  const [ownRetryAt, setOwnRetryAt] = useState<number | null>(null);
  const closesAt = Math.max(retryAt ?? 0, ownRetryAt ?? 0) || null;
  // One timer reopens the box when the tries come back (nothing ticks meanwhile).
  const [, reopen] = useState(0);
  useEffect(() => {
    if (closesAt == null) return;
    const wait = closesAt * 1000 - Date.now();
    if (wait <= 0) return;
    const timer = window.setTimeout(() => reopen((n) => n + 1), Math.min(wait + 250, 2 ** 31 - 1));
    return () => window.clearTimeout(timer);
  }, [closesAt]);
  const nowSec = Math.floor(Date.now() / 1000);
  const triesClosed = closesAt != null && closesAt > nowSec;
  const shownError: Note | null =
    error ??
    (triesClosed ? { key: "tries", tone: "error", wait: waitWords(closesAt - nowSec) } : null);
  const usable = !disabled && !triesClosed && phase === "idle";

  // The latest callbacks, for answers that arrive after a render.
  const latest = useRef({ onSuccess, onLinked, onOther });
  latest.current = { onSuccess, onLinked, onOther };
  const mounted = useRef(true);
  useEffect(
    () => () => {
      mounted.current = false;
    },
    [],
  );

  /** A refused code: the box shakes once and starts again; `null` when the closed box says why. */
  const fail = (note: Note | null) => {
    setPhase("idle");
    setError(note);
    setValue("");
    if (!reduced) box.current?.animate(SHAKE, { duration: 260, easing: "ease-out" });
    requestAnimationFrame(() => input.current?.focus());
  };

  const settle = (outcome: LinkOutcome) => {
    if (!mounted.current) return;
    if (outcome.kind === "error") return fail(outcome.note);
    if (outcome.kind === "other") {
      setPhase("idle");
      setValue("");
      latest.current.onOther(outcome.name);
      return;
    }
    setPhase("linked");
    latest.current.onSuccess?.();
    window.setTimeout(() => {
      if (mounted.current) latest.current.onLinked(outcome.name);
    }, LINKED_HOLD_MS);
  };

  const send = async (pin: string) => {
    setPhase("sending");
    setError(null);
    // While the job runs, its result is this box's to tell (no toast).
    const release = holdCodeBox();
    let stop: (() => void) | null = null;
    try {
      const sent = await projectsSkinApi.link(accountId, pin);
      if (sent.status === "refused") {
        release();
        if (sent.refusal.code === "tries" && mounted.current) {
          // Closed until the tries come back; the wait is said in place of an error, so the
          // box opens again by itself, clean.
          setOwnRetryAt(Math.floor(Date.now() / 1000) + Math.max(1, sent.refusal.retryAfter ?? 60));
          fail(null);
          return;
        }
        settle(linkRefusalOutcome(sent.refusal, Math.floor(Date.now() / 1000)));
        return;
      }
      const jobId = sent.job.id;
      stop = projectsSkinApi.onChange((change) => {
        if (change.kind !== "job" || change.job.id !== jobId) return;
        stop?.();
        // The toast listener has seen the change by now (it subscribed first).
        release();
        settle(linkResultOutcome(change.job));
      });
    } catch {
      stop?.();
      release();
      settle({ kind: "error", note: { key: "linkFailed", tone: "error" } });
    }
  };

  const take = (raw: string) => {
    if (!usable) return;
    const digits = pinDigits(raw);
    setValue(digits);
    if (digits) setError(null);
    if (digits.length === PIN_LENGTH) void send(digits);
  };

  const onPaste = (event: ClipboardEvent<HTMLInputElement>) => {
    event.preventDefault();
    take(event.clipboardData.getData("text"));
  };

  const cursor = Math.min(value.length, PIN_LENGTH - 1);
  const linked = phase === "linked";
  const sending = phase === "sending";

  return (
    <div className={cn("transition-opacity duration-150", disabled && "opacity-50")}>
      <div ref={box} className="relative w-fit">
        <div className="flex" style={{ gap: GAP }}>
          {Array.from({ length: PIN_LENGTH }, (_, index) => {
            const digit = value[index];
            return (
              <div
                // biome-ignore lint/suspicious/noArrayIndexKey: the cells are fixed, in order
                key={index}
                aria-hidden
                className={cn(
                  "relative grid place-items-center rounded-[8px] bg-obsidian-800/90 ring-1 ring-white/12 transition-opacity duration-150",
                  sending && "opacity-55",
                )}
                style={{ width: CELL, height: 50 }}
              >
                <span
                  className={cn(
                    "pointer-events-none absolute inset-0 rounded-[8px] ring-1 ring-flare-400/80 transition-opacity duration-150",
                    shownError && !value ? "opacity-100" : "opacity-0",
                  )}
                />
                <AnimatePresence initial={false}>
                  {digit ? (
                    <motion.span
                      // biome-ignore lint/suspicious/noArrayIndexKey: a cell's digit, re-keyed when it changes
                      key={`${index}-${digit}`}
                      className="absolute inset-0 grid place-items-center font-display text-[22px] font-semibold text-bone-50 tabular-nums"
                      initial={calm({ opacity: 0, scale: 0.92 })}
                      animate={calm({ opacity: 1, scale: 1 })}
                      exit={{ opacity: 0, transition: { duration: 0.08 } }}
                      transition={{ duration: 0.12, ease: easeOutExpo }}
                    >
                      {digit}
                    </motion.span>
                  ) : null}
                </AnimatePresence>
                {sending && index === PIN_LENGTH - 1 ? (
                  <CircleNotch
                    size={15}
                    weight="bold"
                    className="absolute right-1 bottom-1 animate-spin text-gold-300 motion-reduce:animate-none"
                  />
                ) : null}
              </div>
            );
          })}
        </div>
        <motion.span
          aria-hidden
          className="pointer-events-none absolute top-0 left-0 rounded-[8px] ring-2 ring-gold-300"
          style={{ width: CELL, height: 50 }}
          initial={false}
          animate={{
            x: cursor * (CELL + GAP),
            opacity: focused && usable && !shownError ? 1 : 0,
          }}
          transition={{ x: reduced ? { duration: 0 } : springSnappy, opacity: { duration: 0.15 } }}
        />
        <input
          ref={input}
          // biome-ignore lint/a11y/noAutofocus: the player opened the box to type the code
          autoFocus={autoFocus}
          value={value}
          onChange={(event) => take(event.target.value)}
          onPaste={onPaste}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          inputMode="numeric"
          autoComplete="one-time-code"
          maxLength={PIN_LENGTH}
          pattern="[0-9]*"
          spellCheck={false}
          disabled={!usable}
          aria-label={t("projectsSkin.link.codeLabel")}
          aria-invalid={shownError ? true : undefined}
          aria-describedby={[describedBy, shownError ? errorId : null].filter(Boolean).join(" ")}
          className="absolute inset-0 h-full w-full cursor-text text-transparent caret-transparent opacity-0 outline-none disabled:cursor-default"
        />
        <AnimatePresence>
          {linked ? (
            <motion.div
              key="linked"
              aria-hidden
              className="absolute inset-0 grid place-items-center rounded-[8px] bg-gold-500"
              initial={calm({ opacity: 0, scale: 0.96 })}
              animate={calm({ opacity: 1, scale: 1 })}
              transition={{ duration: 0.2, ease: easeOutExpo }}
            >
              <Check size={26} weight="bold" className="text-[#1A0F05]" />
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>
      <div className="min-h-[25px] pt-2 text-[12.5px] leading-snug" aria-live="polite">
        <AnimatePresence initial={false} mode="popLayout">
          {sending ? (
            <motion.p
              key="sending"
              className="text-ash-300"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.15, delay: 0.06 } }}
              exit={{ opacity: 0, transition: { duration: 0.1 } }}
            >
              {t("projectsSkin.link.linking")}
            </motion.p>
          ) : shownError ? (
            <motion.p
              key={`error-${shownError.key}`}
              id={errorId}
              role="alert"
              className="max-w-[46ch] text-flare-400"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.15, delay: 0.06 } }}
              exit={{ opacity: 0, transition: { duration: 0.1 } }}
            >
              <NoteText t={t} note={shownError} />
            </motion.p>
          ) : null}
        </AnimatePresence>
      </div>
    </div>
  );
}
