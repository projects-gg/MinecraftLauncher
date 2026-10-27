// The words of a command or a reply. Three looks:
// - plain words: a field, with a lock that makes them secret (a password command is secret at once);
// - a secret this computer has (sealed here, kept in the keychain, or typed now): a hidden field
//   with an eye that shows it on this computer only, and a quiet "Bulutta şifreli, bu bilgisayarda da saklı.";
// - a secret only the cloud can show (sealed on the site or another computer): its preview, with
//   "Göster" (the machine sends it sealed to a one-time key; shown for a few seconds, never kept)
//   and "Değiştir" (type it again).
import { Eye, EyeSlash, LockSimple, LockSimpleOpen } from "@phosphor-icons/react";
import { Button, cn, enter, IconButton, Input, leave, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type Ref, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { deleteWholeToken, TokenText } from "../settings/controls";
import { cloudApi } from "./api";
import { hasPlaceholder } from "./placeholders";
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
      <div className="flex items-center gap-2">
        <div
          className={cn(
            "relative flex h-9 min-w-0 flex-1 items-center gap-2 overflow-hidden rounded-md border bg-obsidian-850/80 px-3",
            error ? "border-flare-400/60" : "border-white/7",
          )}
        >
          <span className="sr-only">{label}</span>
          <LockSimple size={15} weight="fill" className="shrink-0 text-ash-300" aria-hidden />
          <span className="grid min-w-0 flex-1">
            <AnimatePresence initial={false}>
              {shown ? (
                <motion.span
                  key="shown"
                  className={cn(
                    "truncate text-bone-50 [grid-area:1/1]",
                    mono ? "font-mono text-[13px]" : "text-[13.5px]",
                  )}
                  data-selectable
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1, transition: enter(0.18) }}
                  exit={{ opacity: 0, transition: leave(0.14) }}
                >
                  {reveal.text}
                </motion.span>
              ) : (
                <motion.span
                  key="hidden"
                  className="flex min-w-0 items-center gap-2 [grid-area:1/1]"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1, transition: enter(0.18) }}
                  exit={{ opacity: 0, transition: leave(0.14) }}
                >
                  {name ? (
                    <span
                      className={cn(
                        "truncate text-bone-100",
                        mono ? "font-mono text-[13px]" : "text-[13.5px]",
                      )}
                    >
                      {name}
                    </span>
                  ) : null}
                  <span aria-hidden className="font-mono text-[13px] tracking-widest text-ash-400">
                    ••••••
                  </span>
                </motion.span>
              )}
            </AnimatePresence>
          </span>
          {/* How long the words stay: a hairline that empties (transform only; still under
              reduced motion, where it simply goes with the words). */}
          {shown ? (
            <motion.span
              aria-hidden
              className="absolute right-0 bottom-0 left-0 h-[2px] origin-left bg-gold-400/70"
              initial={{ transform: "scaleX(1)" }}
              animate={{
                transform: reduce ? "scaleX(1)" : "scaleX(0)",
                transition: { duration: SHOWN_MS / 1000, ease: "linear" },
              }}
            />
          ) : null}
        </div>
        <Button
          size="sm"
          variant="ghost"
          icon={shown ? <EyeSlash size={15} /> : <Eye size={15} />}
          disabled={reveal.state === "loading"}
          onClick={() => (shown ? setReveal({ state: "idle" }) : void show())}
        >
          {shown ? t("cloudAfk.settings.secret.hide") : t("cloudAfk.settings.secret.show")}
        </Button>
        <Button size="sm" variant="ghost" onClick={onChange}>
          {t("cloudAfk.settings.secret.change")}
        </Button>
      </div>
      {error || reveal.state === "failed" ? (
        <p role="alert" className="mt-1.5 text-[12.5px] text-flare-400">
          {error ??
            (reveal.state === "failed" ? t(`cloudAfk.settings.secret.${reveal.reason}`) : null)}
        </p>
      ) : null}
    </div>
  );
}

/** A 7/24 Bulut reply's placeholders (`{u}`, `{1}`…), each piece of a split. */
const CLOUD_TOKENS = /(\{(?:u|[1-9])\})/;

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
  tokenName,
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
  /** A reply's placeholders: their names, shown as chips while the field is not typed in. */
  tokenName?: (token: string) => string;
}) {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);
  const [typing, setTyping] = useState(false);
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
  const chips = Boolean(tokenName) && !secret && !typing && hasPlaceholder(words.text);
  return (
    <div className="min-w-0 flex-1">
      {heading}
      <div className="flex items-center gap-2">
        <div className="relative min-w-0 flex-1">
          <Input
            ref={inputRef}
            id={fieldId}
            aria-label={showLabel ? undefined : label}
            value={words.text}
            placeholder={replaced.current ? t("cloudAfk.settings.secret.newValue") : placeholder}
            type={secret && !visible ? "password" : "text"}
            maxLength={256}
            spellCheck={false}
            autoComplete={secret ? "new-password" : "off"}
            data-1p-ignore=""
            data-lpignore="true"
            invalid={Boolean(error)}
            autoFocus={autoFocus || focusNext}
            onFocus={() => setTyping(true)}
            onBlur={() => setTyping(false)}
            // A placeholder goes in one press of Backspace or Delete, never half of it.
            onKeyDown={
              tokenName && !secret
                ? (event) =>
                    deleteWholeToken(event, CLOUD_TOKENS, (text) => onChange(typed(words, text)))
                : undefined
            }
            onChange={(event) => onChange(typed(words, event.target.value))}
            className={cn(
              "h-9",
              mono ? "font-mono text-[13px]" : "text-[13.5px]",
              secret && "pr-10",
              chips && "text-transparent",
            )}
          />
          {chips && tokenName ? (
            <TokenText
              text={words.text}
              split={CLOUD_TOKENS}
              name={(piece) => tokenName(piece.slice(1, -1))}
            />
          ) : null}
          {secret ? (
            <IconButton
              label={
                visible ? t("cloudAfk.settings.secret.hide") : t("cloudAfk.settings.secret.show")
              }
              icon={visible ? <EyeSlash size={16} /> : <Eye size={16} />}
              size="sm"
              className="absolute top-1/2 right-0.5 size-8 -translate-y-1/2"
              onClick={() => setVisible((v) => !v)}
            />
          ) : null}
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
            {t("cloudAfk.settings.secret.mark")}
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
