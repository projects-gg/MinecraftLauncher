// "ProBot AI ile kalıp kur" in a 7/24 Bulut rule, as on afk.projects.gg (AFKWEB-OWN): the player
// says in their own words which messages the rule should answer, and a box under the rule shows
// what ProBot AI made of it (one sentence, messages it answers and leaves alone, the pattern folded
// away) with "Kullan" and "Tekrar dene". A pattern it made reads as its sentence in the rule's field;
// the rule stores only the pattern, so this computer keeps the sentence (a per-viewer convenience:
// elsewhere it reads as "Özel kalıp"). Offered only while the service says its model can answer
// (`aiPatterns`); otherwise a rule is a plain phrase or a pattern written by hand, as before.
import { X } from "@phosphor-icons/react";
import { Button, enter, IconButton, Input, leave, SwapLabel, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { isFriendly } from "../errors";
import { PatternFold, TextAction, type TriggerAssist } from "../settings/controls";
import type { PatternSuggestion } from "./api";

/** A description, in characters (the service's limit). */
export const PATTERN_TEXT_MAX = 200;

// ---- The sentence of a pattern ProBot AI made, kept on this computer --------------------------

const STORE = "pl.afk.patternSentences";
const STORE_MAX = 100;

type Saved = Record<string, { summary: string; at: number }>;

function hashOf(text: string): string {
  let hash = 5381;
  for (const char of text) hash = ((hash << 5) + hash + (char.codePointAt(0) ?? 0)) | 0;
  return (hash >>> 0).toString(36);
}

function keyOf(accountId: string, ruleId: string, pattern: string): string {
  return `${accountId}:${ruleId}:${hashOf(pattern)}`;
}

function readSaved(): Saved {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(STORE) ?? "{}");
    return value && typeof value === "object" && !Array.isArray(value) ? (value as Saved) : {};
  } catch {
    return {};
  }
}

/** The sentence ProBot AI gave this rule's pattern here; null when the pattern is not one it made. */
export function savedSentence(accountId: string, ruleId: string, pattern: string): string | null {
  const entry = readSaved()[keyOf(accountId, ruleId, pattern)];
  return typeof entry?.summary === "string" ? entry.summary : null;
}

export function saveSentence(
  accountId: string,
  ruleId: string,
  pattern: string,
  summary: string,
): void {
  try {
    const saved = readSaved();
    saved[keyOf(accountId, ruleId, pattern)] = { summary, at: Date.now() };
    const kept = Object.entries(saved)
      .sort((a, b) => b[1].at - a[1].at)
      .slice(0, STORE_MAX);
    localStorage.setItem(STORE, JSON.stringify(Object.fromEntries(kept)));
  } catch {
    // Storage may be unavailable: the sentence lasts as long as the sheet.
  }
}

// ---- The box -----------------------------------------------------------------------------------

/** Example chat messages: what the rule answers in bone, what it leaves alone in ash. */
function Examples({ label, lines, dim }: { label: string; lines: string[]; dim?: boolean }) {
  return (
    <div className="min-w-0">
      <p className="text-[12px] font-medium text-ash-400">{label}</p>
      <ul className="mt-1 flex flex-col gap-0.5 text-[13px] leading-snug">
        {lines.map((line) => (
          <li
            key={line}
            data-selectable
            className={dim ? "break-words text-ash-400" : "break-words text-bone-100"}
          >
            {line}
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * ProBot AI's box under a rule: the player's words in, its sentence and examples out. "Kullan" puts
 * the trigger in the rule (saved with the sheet); "Tekrar dene" asks for a new answer instead of the
 * kept one (not for a plain phrase, which needs no model). Its refusals come worded by the command.
 */
export function PatternHelper({
  suggest,
  onUse,
  onManual,
  onClose,
}: {
  suggest: (text: string, fresh: boolean) => Promise<PatternSuggestion>;
  onUse: (suggestion: PatternSuggestion) => void;
  onManual: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const inputId = useId();
  const input = useRef<HTMLInputElement>(null);
  const [text, setText] = useState("");
  const [asked, setAsked] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<PatternSuggestion | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    input.current?.focus();
  }, []);

  const ask = async (again: boolean) => {
    const description = (again ? asked : text).replace(/\s+/g, " ").trim();
    if (!description || busy) return;
    if ([...description].length > PATTERN_TEXT_MAX) {
      setError(t("afkPanel.rule.helper.tooLong"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setResult(await suggest(description, again));
      setAsked(description);
    } catch (problem) {
      setResult(null);
      setError(isFriendly(problem) ? problem.message : t("cloudAfk.actions.failed"));
    } finally {
      setBusy(false);
    }
  };

  return (
    // A band of the rule's own card, not a box in it: only the answer sits in a well.
    <motion.div
      data-pattern-helper=""
      className="mt-3 border-t border-white/6 pt-2.5"
      initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-4px)" }}
      animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.2) }}
      exit={{ opacity: 0, transition: leave(0.18) }}
    >
      <div className="flex items-center justify-between gap-3">
        <p className="text-[13.5px] font-semibold text-bone-50">
          {t("afkPanel.rule.helper.title")}
        </p>
        <IconButton
          label={t("afkPanel.rule.helper.close")}
          icon={<X size={14} />}
          size="sm"
          onClick={onClose}
        />
      </div>
      <form
        autoComplete="off"
        className="flex flex-col gap-1.5"
        onSubmit={(event) => {
          event.preventDefault();
          void ask(false);
        }}
      >
        <label htmlFor={inputId} className="text-[12.5px] text-ash-300">
          {t("afkPanel.rule.helper.prompt")}
        </label>
        <div className="flex items-center gap-2">
          <Input
            ref={input}
            id={inputId}
            value={text}
            maxLength={PATTERN_TEXT_MAX + 40}
            placeholder={t("afkPanel.rule.helper.placeholder")}
            spellCheck={false}
            autoComplete="off"
            invalid={Boolean(error)}
            onChange={(event) => {
              setText(event.target.value);
              setError(null);
            }}
            className="h-8 min-w-0 flex-1"
          />
          <Button
            type="submit"
            variant="secondary"
            disabled={busy || !text.trim()}
            aria-busy={busy || undefined}
            className="min-w-[112px] shrink-0"
          >
            <SwapLabel
              labels={[t("afkPanel.rule.helper.ask"), t("afkPanel.rule.helper.asking")]}
              active={busy ? 1 : 0}
            />
          </Button>
        </div>
      </form>
      {error ? (
        <p role="alert" className="mt-2 text-[12.5px] leading-snug text-flare-300">
          {error}
        </p>
      ) : null}
      <AnimatePresence initial={false} mode="wait">
        {result ? (
          <motion.div
            key={`${result.match}\n${result.summary}`}
            aria-live="polite"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            <div className="mt-3 rounded-[var(--radius-panel)] bg-obsidian-950/55 p-3">
              <p className="text-[14px] leading-snug text-bone-50">{result.summary}</p>
              <div className="mt-2.5 grid grid-cols-2 gap-3">
                <Examples label={t("afkPanel.rule.helper.catches")} lines={result.matches} />
                {result.misses.length > 0 ? (
                  <Examples label={t("afkPanel.rule.helper.misses")} lines={result.misses} dim />
                ) : null}
              </div>
              {result.regex ? <PatternFold pattern={result.match} /> : null}
            </div>
            <div className="mt-2.5 flex flex-wrap items-center gap-2">
              {/* The action that needs the player, beside the sheet's one gold (its save). */}
              <Button variant="bone" onClick={() => onUse(result)} className="min-w-[88px]">
                {t("afkPanel.rule.helper.use")}
              </Button>
              {result.source === "plain" ? null : (
                <Button variant="ghost" disabled={busy} onClick={() => void ask(true)}>
                  {t("afkPanel.rule.helper.retry")}
                </Button>
              )}
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
      <div className="mt-3">
        <TextAction onClick={onManual}>{t("afkPanel.rule.writeMyself")}</TextAction>
      </div>
    </motion.div>
  );
}

// ---- A rule's trigger with ProBot AI ------------------------------------------------------------

/**
 * What a cloud rule's TriggerField takes (`assist`, undefined while ProBot AI cannot answer) and the
 * box to put under the rule's first row (as wide as the rule).
 */
export function useRuleAssist({
  accountId,
  ruleId,
  fieldId,
  pattern,
  regex,
  available,
  suggest,
  onChange,
}: {
  accountId: string;
  ruleId: string;
  /** The trigger field's id: "Kullan" and "Kapat" give it the focus back. */
  fieldId: string;
  pattern: string;
  regex: boolean;
  /** The overview's `aiPatterns`: the service's model can answer now. */
  available: boolean;
  suggest: (text: string, fresh: boolean) => Promise<PatternSuggestion>;
  onChange: (pattern: string, regex: boolean) => void;
}): { assist: TriggerAssist | undefined; box: ReactNode } {
  const [open, setOpen] = useState(false);
  // "Kalıbı kendim yazacağım": the pattern shows as itself even when ProBot AI made it.
  const [manual, setManual] = useState(false);
  const [sentence, setSentence] = useState<string | null>(null);

  // The sentence follows the pattern: one made here, or one this computer kept for it.
  useEffect(() => {
    setSentence(regex ? savedSentence(accountId, ruleId, pattern) : null);
  }, [accountId, ruleId, pattern, regex]);

  const focusField = () =>
    requestAnimationFrame(() => document.getElementById(fieldId)?.focus({ preventScroll: true }));

  const use = (suggestion: PatternSuggestion) => {
    if (suggestion.regex) saveSentence(accountId, ruleId, suggestion.match, suggestion.summary);
    setSentence(suggestion.regex ? suggestion.summary : null);
    setManual(false);
    setOpen(false);
    onChange(suggestion.match, suggestion.regex);
    focusField();
  };

  const writeMyself = () => {
    setManual(true);
    setOpen(false);
    onChange(pattern, true);
    focusField();
  };

  const assist: TriggerAssist | undefined = available
    ? { sentence: manual ? null : sentence, open, onOpen: () => setOpen(true) }
    : undefined;

  const box = (
    <AnimatePresence initial={false}>
      {available && open ? (
        <PatternHelper
          key="helper"
          suggest={suggest}
          onUse={use}
          onManual={writeMyself}
          onClose={() => {
            setOpen(false);
            focusField();
          }}
        />
      ) : null}
    </AnimatePresence>
  );

  return { assist, box };
}
