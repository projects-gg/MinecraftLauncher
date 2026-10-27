// The words of a command or a reply, in the AFK surfaces' one look for secret lines (secret.tsx):
// - plain words: a field, with "Gizle" beside it that makes them secret (a password command is
//   secret at once);
// - a secret this computer has (sealed here, kept in the keychain, or typed now): the padlocked,
//   masked field with its eye (shown on this computer only), and a quiet "Bulutta şifreli, bu
//   bilgisayarda da saklı.";
// - a secret only the cloud can show (sealed on the site or another computer): the kept field, with
//   "Göster" (the machine sends it sealed to a one-time key; shown for a few seconds, never kept)
//   and "Değiştir" (type it again).
import { Eye, EyeSlash, LockSimple, LockSimpleOpen, PencilSimple } from "@phosphor-icons/react";
import { Button, cn, useMotionPrefs } from "@pl/ui";
import { motion } from "motion/react";
import { type Ref, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { SecretField, SecretKept } from "../secret";
import { TokenField, type TokenSet } from "../settings/controls";
import { cloudApi } from "./api";
import { looksSensitive } from "./sensitive";
import { typed, type Words } from "./settings-model";

/** How long revealed words stay on screen. */
const SHOWN_MS = 10_000;

type Reveal =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "shown"; text: string; until: number }
  | { state: "failed"; reason: "offline" | "limited" | "unreadable" };

function Stored({
  preview,
  accountId,
  rowId,
  onChange,
  error,
  label,
  mono,
}: {
  preview: string;
  accountId: string;
  rowId: string;
  onChange: () => void;
  error: string | null;
  label: string;
  mono: boolean;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
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
        mono={mono}
        shown={shown ? reveal.text : null}
        overlay={
          // How long the words stay: a hairline that empties (transform only; still under
          // reduced motion, where it simply goes with the words).
          shown ? (
            <motion.span
              aria-hidden
              className="absolute right-0 bottom-0 left-0 h-[2px] origin-left bg-gold-400/70"
              initial={{ transform: "scaleX(1)" }}
              animate={{
                transform: reduce ? "scaleX(1)" : "scaleX(0)",
                transition: { duration: SHOWN_MS / 1000, ease: "linear" },
              }}
            />
          ) : null
        }
      >
        <Button
          size="sm"
          variant="ghost"
          icon={shown ? <EyeSlash size={15} /> : <Eye size={15} />}
          disabled={reveal.state === "loading"}
          onClick={() => (shown ? setReveal({ state: "idle" }) : void show())}
        >
          {shown ? t("cloudAfk.settings.secret.hide") : t("cloudAfk.settings.secret.show")}
        </Button>
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
  const heading = showLabel ? (
    words.kind === "sealed" ? (
      <p aria-hidden className="mb-1.5 text-[13px] font-medium text-bone-100">
        {label}
      </p>
    ) : (
      <label htmlFor={fieldId} className="mb-1.5 block text-[13px] font-medium text-bone-100">
        {label}
      </label>
    )
  ) : null;

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
          mono={mono}
          onChange={() => {
            replaced.current = words;
            setFocusNext(true);
            onChange({ kind: "secret", text: "", kept: false });
          }}
        />
      </div>
    );
  }

  const secret = words.kind === "secret";
  const forced = looksSensitive(words.text);
  const kept = words.kind === "secret" && words.kept;
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
              value={words.text}
              secret={secret}
              mono={mono}
              placeholder={replaced.current ? t("cloudAfk.settings.secret.newValue") : placeholder}
              maxLength={256}
              invalid={Boolean(error)}
              autoFocus={autoFocus || focusNext}
              onChange={(text) => onChange(typed(words, text))}
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
                ? "bg-white/12 text-bone-50"
                : "bg-white/5 text-ash-300 hover:bg-white/9 hover:text-bone-100",
            )}
          >
            {secret ? (
              <LockSimple size={14} weight="fill" aria-hidden />
            ) : (
              <LockSimpleOpen size={14} aria-hidden />
            )}
            {secret ? t("cloudAfk.settings.secret.mark") : t("afkPanel.makeSecret")}
          </button>
        )}
      </div>
      {error ? (
        <p role="alert" className="mt-1.5 text-[12.5px] text-flare-400">
          {error}
        </p>
      ) : kept ? (
        <p className="mt-1 text-[12px] text-ash-400">{t("cloudAfk.settings.secret.kept")}</p>
      ) : forced && secret ? (
        <p className="mt-1 text-[12px] text-ash-400">{t("cloudAfk.settings.secret.auto")}</p>
      ) : null}
    </div>
  );
}
