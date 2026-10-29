// The words of a command or a reply, in the AFK surfaces' one look for secret lines (secret.tsx):
// - plain words: a field, with "Gizle" beside it that makes them secret (a password command is
//   secret at once, and says so);
// - a stored secret, wherever its words are: the kept field (padlock, its command, dots) with
//   "Değiştir" beside it, as on the desktop. Its eye shows the words for a few seconds: from this
//   computer's copy (sealed here, in the keychain) or from the machine (sealed on the site or
//   another computer: sent sealed to a one-time key, never kept);
// - a secret being typed (new, or again after "Değiştir"): the padlocked, masked field with its eye.
import { LockSimple, PencilSimple } from "@phosphor-icons/react";
import { Button, cn, useMotionPrefs } from "@pl/ui";
import { motion } from "motion/react";
import { type Ref, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { SecretField, SecretKept, splitCommand } from "../secret";
import { TokenField, type TokenSet } from "../settings/controls";
import { cloudApi } from "./api";
import { commandOf } from "./bulk";
import { looksSensitive } from "./sensitive";
import { typed, type Words } from "./settings-model";

/** How long revealed words stay on screen. */
const SHOWN_MS = 10_000;

type Reveal =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "shown"; text: string; until: number }
  | { state: "failed"; reason: "offline" | "limited" | "unreadable" };

/** How long shown words stay: a hairline that empties (transform only; still under reduced
 * motion, where it simply goes with the words). */
function Countdown() {
  const { reduce } = useMotionPrefs();
  return (
    <motion.span
      aria-hidden
      className="absolute right-0 bottom-0 left-0 h-[2px] origin-left bg-gold-400/70"
      initial={{ transform: "scaleX(1)" }}
      animate={{
        transform: reduce ? "scaleX(1)" : "scaleX(0)",
        transition: { duration: SHOWN_MS / 1000, ease: "linear" },
      }}
    />
  );
}

/**
 * A stored secret this computer has the words of (sealed here, in the keychain): the same kept
 * field as one only the machine can show, its eye showing this computer's copy (no trip). As an
 * open field with a pressed "Gizli" it was a third look for a stored line (AFK-CLEAR look 2).
 */
function KeptHere({
  text,
  label,
  error,
  onChange,
}: {
  text: string;
  label: string;
  error: string | null;
  onChange: () => void;
}) {
  const { t } = useTranslation();
  const [shown, setShown] = useState(false);
  // Shown words go when the time is up, when the row leaves and when the sheet closes.
  useEffect(() => {
    if (!shown) return;
    const id = window.setTimeout(() => setShown(false), SHOWN_MS);
    return () => window.clearTimeout(id);
  }, [shown]);
  return (
    <div className="min-w-0 flex-1">
      <SecretKept
        label={label}
        prefix={splitCommand(text)?.command ?? null}
        invalid={Boolean(error)}
        shown={shown ? text : null}
        eye={{ busy: false, onToggle: () => setShown((on) => !on) }}
        overlay={shown ? <Countdown /> : null}
      >
        <Button size="sm" variant="ghost" icon={<PencilSimple size={14} />} onClick={onChange}>
          {t("cloudAfk.settings.secret.change")}
        </Button>
      </SecretKept>
      {error ? (
        <p role="alert" className="mt-1.5 text-[12.5px] text-flare-400">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function Stored({
  preview,
  accountId,
  rowId,
  onChange,
  error,
  label,
}: {
  preview: string;
  accountId: string;
  rowId: string;
  onChange: () => void;
  error: string | null;
  label: string;
}) {
  const { t } = useTranslation();
  const [reveal, setReveal] = useState<Reveal>({ state: "idle" });
  // Shown words go when the time is up, when the row leaves and when the sheet closes.
  useEffect(() => {
    if (reveal.state !== "shown") return;
    const id = window.setTimeout(() => setReveal({ state: "idle" }), reveal.until - Date.now());
    return () => window.clearTimeout(id);
  }, [reveal]);

  const show = async () => {
    setReveal({ state: "loading" });
    try {
      const outcome = await cloudApi.reveal(accountId, rowId);
      if (outcome.kind === "shown") {
        setReveal({ state: "shown", text: outcome.text, until: Date.now() + SHOWN_MS });
      } else {
        setReveal({
          state: "failed",
          reason:
            outcome.kind === "offline"
              ? "offline"
              : outcome.kind === "limited"
                ? "limited"
                : "unreadable",
        });
      }
    } catch {
      setReveal({ state: "failed", reason: "offline" });
    }
  };

  const shown = reveal.state === "shown";
  const name = preview.replace(/\s*\*+$/, "");
  return (
    <div className="min-w-0 flex-1">
      <SecretKept
        label={label}
        prefix={name || null}
        invalid={Boolean(error)}
        shown={shown ? reveal.text : null}
        eye={{
          busy: reveal.state === "loading",
          onToggle: () => (shown ? setReveal({ state: "idle" }) : void show()),
        }}
        overlay={shown ? <Countdown /> : null}
      >
        <Button size="sm" variant="ghost" icon={<PencilSimple size={14} />} onClick={onChange}>
          {t("cloudAfk.settings.secret.change")}
        </Button>
      </SecretKept>
      {error || reveal.state === "failed" ? (
        <p role="alert" className="mt-1.5 text-[12.5px] text-flare-400">
          {error ??
            (reveal.state === "failed" ? t(`cloudAfk.settings.secret.${reveal.reason}`) : null)}
        </p>
      ) : null}
    </div>
  );
}

export function WordsField({
  words,
  onChange,
  accountId,
  rowId,
  label,
  placeholder,
  error,
  autoFocus,
  inputRef,
  id,
  showLabel = false,
  mono = true,
  tokens,
}: {
  words: Words;
  onChange: (words: Words) => void;
  accountId: string;
  rowId: string;
  label: string;
  placeholder: string;
  error: string | null;
  autoFocus?: boolean;
  /** The words' field (a reply's placeholder chips type where its caret is). */
  inputRef?: Ref<HTMLInputElement>;
  id?: string;
  /** The label above the field (a rule's "Yanıt"), else only its accessible name. */
  showLabel?: boolean;
  /** Commands look like what is typed into the game; a reply is a sentence (SHEET-UX). */
  mono?: boolean;
  /** A reply's placeholders, shown by name as chips (`TokenField`). */
  tokens?: TokenSet;
}) {
  const { t } = useTranslation();
  // "Değiştir" on a stored secret: what to go back to with "Vazgeç".
  const replaced = useRef<Words | null>(null);
  const [focusNext, setFocusNext] = useState(false);
  const generated = useId();
  const fieldId = id ?? generated;
  const stored = words.kind === "sealed" || (words.kind === "secret" && words.kept);
  const heading = showLabel ? (
    stored ? (
      <p aria-hidden className="mb-1.5 text-[13px] font-medium text-bone-100">
        {label}
      </p>
    ) : (
      <label htmlFor={fieldId} className="mb-1.5 block text-[13px] font-medium text-bone-100">
        {label}
      </label>
    )
  ) : null;

  // "Değiştir" on a stored secret: the field empties for the new words ("Eskisi kalsın" goes back).
  const replace = (before: Words) => {
    replaced.current = before;
    setFocusNext(true);
    onChange({ kind: "secret", text: "", kept: false });
  };

  if (words.kind === "sealed") {
    return (
      <div className="min-w-0 flex-1">
        {heading}
        <Stored
          preview={words.preview}
          accountId={accountId}
          rowId={rowId}
          label={label}
          error={error}
          onChange={() => replace(words)}
        />
      </div>
    );
  }
  if (words.kind === "secret" && words.kept) {
    return (
      <div className="min-w-0 flex-1">
        {heading}
        <KeptHere text={words.text} label={label} error={error} onChange={() => replace(words)} />
      </div>
    );
  }

  const secret = words.kind === "secret";
  const forced = looksSensitive(words.text);
  // Being typed again over a stored line: its command (`/login`), shown before the field.
  const before = replaced.current;
  const command =
    before?.kind === "sealed"
      ? commandOf(before.preview)
      : before?.kind === "secret"
        ? (splitCommand(before.text)?.command ?? "")
        : "";
  const withCommand = command !== "" && (words.text === "" || words.text.startsWith(`${command} `));
  return (
    <div className="min-w-0 flex-1">
      {heading}
      <div className="flex items-center gap-2">
        <div className="relative min-w-0 flex-1">
          {tokens && !secret ? (
            // A reply's placeholders stay chips with their names, typing included.
            <TokenField
              id={fieldId}
              label={showLabel ? undefined : label}
              value={words.text}
              tokens={tokens}
              placeholder={placeholder}
              invalid={Boolean(error)}
              inputRef={inputRef}
              autoFocus={autoFocus || focusNext}
              autoComplete="off"
              onChange={(text) => onChange(typed(words, text))}
            />
          ) : (
            <SecretField
              ref={inputRef}
              id={fieldId}
              aria-label={showLabel ? undefined : label}
              value={withCommand ? words.text.slice(command.length + 1) : words.text}
              secret={secret}
              // "Değiştir" on a kept `/login` line: the command stays in front, only the password is
              // typed again, as on the desktop and in "Ayarları kopyala" (a whole command typed
              // over it is taken as it is).
              command={withCommand ? command : null}
              mono={mono}
              placeholder={
                replaced.current
                  ? command
                    ? t("afkSettings.login.newSecret")
                    : t("cloudAfk.settings.secret.newValue")
                  : placeholder
              }
              maxLength={256 - (withCommand ? command.length + 1 : 0)}
              invalid={Boolean(error)}
              autoFocus={autoFocus || focusNext}
              onChange={(text) =>
                onChange(
                  typed(
                    words,
                    command && text !== "" && !text.startsWith("/") ? `${command} ${text}` : text,
                  ),
                )
              }
            />
          )}
        </div>
        {replaced.current ? (
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              const back = replaced.current;
              replaced.current = null;
              if (back) onChange(back);
            }}
          >
            {t("cloudAfk.settings.secret.undoChange")}
          </Button>
        ) : (
          // In words, as on the site: an unnamed padlock beside every line said nothing. Pressed
          // while the line is secret (a password command stays so); no gold, it is not the action.
          // Plain, a button like any other with a closed lock: pressing locks the line (a faint
          // one with an open lock read as disabled and as "unlock"); secret, the quiet plate with a
          // filled lock (SITE-W2B's twin, BETA-FB). The words sit right after the lock.
          <button
            type="button"
            aria-pressed={secret}
            disabled={forced && secret}
            title={t("cloudAfk.settings.secret.markHelp")}
            onClick={() =>
              onChange(
                secret
                  ? { kind: "plain", text: words.text }
                  : { kind: "secret", text: words.text, kept: false },
              )
            }
            className={cn(
              "chamfer inline-flex h-8 shrink-0 cursor-pointer items-center gap-1.5 px-2.5 text-[12.5px] font-medium transition-[background-color,color,transform,scale] duration-150 [--cut:5px] outline-none focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97] disabled:cursor-default disabled:active:scale-100",
              secret
                ? "bg-white/12 text-bone-50 enabled:hover:bg-white/16"
                : "bg-white/8 text-bone-100 hover:bg-white/12",
            )}
          >
            <LockSimple size={14} weight={secret ? "fill" : "regular"} aria-hidden />
            {secret ? t("cloudAfk.settings.secret.mark") : t("afkPanel.makeSecret")}
          </button>
        )}
      </div>
      {error ? (
        <p role="alert" className="mt-1.5 text-[12.5px] text-flare-400">
          {error}
        </p>
      ) : forced && secret && !replaced.current ? (
        // Why a line just typed went secret; a line typed again over a stored one needs no word.
        <p className="mt-1 text-[12px] text-ash-400">{t("cloudAfk.settings.secret.auto")}</p>
      ) : null}
    </div>
  );
}
