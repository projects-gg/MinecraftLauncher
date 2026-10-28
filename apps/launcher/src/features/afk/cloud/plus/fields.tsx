// AFK+'s own fields: an amount of the game's money that reads the players' shorthands ("40k",
// "40 bin") and says the number in words under it, so a zero too many shows before it costs
// anything (afkplus.md §1.4), and a 24-hour time typed quickly ("0830" → "08:30"; the browser's
// own time field shows "08:00 AM" on an English Windows to a Turkish player).
import { cn } from "@pl/ui";
import { type ReactNode, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { inputClass } from "../../settings/controls";
import { readTime } from "./model";
import { amountWords, currentLang, formatAmount, parseAmount } from "./words";

/** The amount as typed: a number, nothing (an empty optional field), or text that is none. */
export type AmountValue = number | null;

function shown(value: AmountValue): string {
  return value === null ? "" : formatAmount(value);
}

export function AmountField({
  label,
  value,
  onChange,
  optional = false,
  placeholder,
  error,
  help,
  className,
  autoFocus,
  hideLabel,
  invalid = false,
  describedBy,
  lineless = false,
  extra,
  anchor,
}: {
  label: string;
  value: AmountValue;
  /** `undefined`: the text is not an amount (the caller keeps the last good value). */
  onChange: (value: AmountValue | undefined) => void;
  optional?: boolean;
  placeholder?: string;
  error?: string | null;
  help?: ReactNode;
  className?: string;
  autoFocus?: boolean;
  hideLabel?: boolean;
  /** The flare edge without a line of its own: the caller says why once, for several fields. */
  invalid?: boolean;
  /** The id of that shared line, so the field is described by it too. */
  describedBy?: string;
  /** No line under the field (the caller puts the shared line in its place). */
  lineless?: boolean;
  /** Its own line under the field's (what the limit counted, what the amount does). */
  extra?: ReactNode;
  /** The block's id, so a link elsewhere brings the field into view ("Sınırlara git"). */
  anchor?: string;
}) {
  const id = useId();
  const { t } = useTranslation();
  const [text, setText] = useState(() => shown(value));
  const [typed, setTyped] = useState(false);
  // Follow outside changes (a reset, a draft card) without fighting the player's typing.
  useEffect(() => {
    if (!typed) setText(shown(value));
  }, [value, typed]);
  const parsed = text.trim() === "" ? null : parseAmount(text);
  const bad = text.trim() !== "" && parsed === null;
  const words = parsed !== null && parsed > 0 ? amountWords(parsed, currentLang()) : null;
  const line = error ?? (bad ? t("afkPlus.form.errors.amount") : null);
  const flare = Boolean(line) || invalid;
  return (
    <div id={anchor} className={cn("flex min-w-0 flex-col gap-1.5", className)}>
      <label
        htmlFor={id}
        className={cn("text-[13px] font-medium text-bone-100", hideLabel && "sr-only")}
      >
        {label}
      </label>
      <input
        id={id}
        inputMode="numeric"
        autoComplete="off"
        spellCheck={false}
        // biome-ignore lint/a11y/noAutofocus: the field the player came to write in
        autoFocus={autoFocus}
        value={text}
        placeholder={placeholder}
        aria-invalid={flare ? true : undefined}
        aria-describedby={
          [lineless ? null : `${id}-line`, describedBy].filter(Boolean).join(" ") || undefined
        }
        onChange={(event) => {
          const next = event.target.value;
          setText(next);
          setTyped(true);
          if (next.trim() === "") onChange(optional ? null : undefined);
          else onChange(parseAmount(next) ?? undefined);
        }}
        onBlur={() => {
          setTyped(false);
          if (parsed !== null) setText(formatAmount(parsed));
        }}
        className={cn(
          inputClass,
          "h-[34px] px-3 tabular-nums",
          flare && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
        )}
      />
      {lineless ? null : (
        <p
          id={`${id}-line`}
          role={line ? "alert" : undefined}
          className={cn(
            "min-h-[17px] text-[12px] leading-[17px]",
            line ? "text-flare-400" : "text-ash-400",
          )}
        >
          {line ?? words ?? help ?? ""}
        </p>
      )}
      {extra}
    </div>
  );
}

/**
 * A quiet text action ("Formla ekle", "Envanterden seç", "Harcananı sıfırla"). `bone` where gold
 * would be a second loud thing beside the card's answer ("Sınırlara git" on an approval).
 */
export function QuietLink({
  children,
  onClick,
  disabled = false,
  tone = "gold",
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  tone?: "gold" | "bone";
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "cursor-pointer rounded-sm text-[12.5px] font-medium underline-offset-2 outline-none",
        tone === "gold" ? "text-gold-300 hover:text-gold-200" : "text-bone-100 hover:text-bone-50",
        "transition-[color,opacity] duration-150 hover:underline focus-visible:ring-2 focus-visible:ring-gold-300",
        "disabled:cursor-not-allowed disabled:text-ash-500 disabled:no-underline",
      )}
    >
      {children}
    </button>
  );
}

/** "HH:MM", 24 hours, typed quickly; `onChange` gets a valid time or null while it is not one. */
export function TimeField({
  label,
  value,
  onChange,
  invalid,
  className,
  hideLabel,
}: {
  label: string;
  value: string;
  onChange: (value: string | null) => void;
  invalid?: boolean;
  className?: string;
  hideLabel?: boolean;
}) {
  const id = useId();
  const [text, setText] = useState(value);
  useEffect(() => {
    setText((current) => (readTime(current) === value ? current : value));
  }, [value]);
  const bad = invalid || (text.trim() !== "" && readTime(text) === null);
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label
        htmlFor={id}
        className={cn("text-[12.5px] font-medium text-ash-300", hideLabel && "sr-only")}
      >
        {label}
      </label>
      <input
        id={id}
        inputMode="numeric"
        autoComplete="off"
        spellCheck={false}
        value={text}
        placeholder="00:00"
        maxLength={5}
        aria-invalid={bad ? true : undefined}
        onChange={(event) => {
          setText(event.target.value);
          onChange(readTime(event.target.value));
        }}
        onBlur={() => {
          const time = readTime(text);
          if (time) setText(time);
        }}
        className={cn(
          inputClass,
          "h-[34px] w-[84px] px-3 text-center tabular-nums",
          bad && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
        )}
      />
    </div>
  );
}

/**
 * A whole number (a quantity, minutes, a percent); `onChange` gets null while it is not one.
 * `after`: words that finish the sentence right beside the field ("dakika sonra hayır sayılır").
 */
export function IntField({
  label,
  value,
  onChange,
  unit,
  unitFirst = false,
  after,
  error,
  className,
  inputClassName,
}: {
  label: string;
  value: number;
  onChange: (value: number | null) => void;
  unit?: string;
  /** The unit before the number (Turkish "%90"). */
  unitFirst?: boolean;
  after?: ReactNode;
  error?: string | null;
  className?: string;
  inputClassName?: string;
}) {
  const id = useId();
  const [text, setText] = useState(String(value));
  useEffect(() => {
    setText((current) => (Number(current) === value ? current : String(value)));
  }, [value]);
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label htmlFor={id} className="text-[13px] font-medium text-bone-100">
        {label}
      </label>
      <div className={cn("relative", after !== undefined && "flex items-center gap-2.5")}>
        <input
          id={id}
          inputMode="numeric"
          autoComplete="off"
          spellCheck={false}
          value={text}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? `${id}-error` : undefined}
          onChange={(event) => {
            const next = event.target.value.replace(/[^\d]/g, "");
            setText(next);
            onChange(next === "" ? null : Number(next));
          }}
          className={cn(
            inputClass,
            "h-[34px] px-3 tabular-nums",
            unit && (unitFirst ? "pl-8" : "pr-10"),
            error && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
            inputClassName,
          )}
        />
        {unit ? (
          <span
            className={cn(
              "pointer-events-none absolute top-1/2 -translate-y-1/2 text-[12.5px] text-ash-400",
              unitFirst ? "left-3" : "right-3",
            )}
          >
            {unit}
          </span>
        ) : null}
        {after !== undefined ? <span className="text-[13px] text-ash-300">{after}</span> : null}
      </div>
      {error ? (
        <p id={`${id}-error`} role="alert" className="text-[12px] text-flare-400">
          {error}
        </p>
      ) : null}
    </div>
  );
}

/** A group's heading inside the panel's sections (Tektur, the launcher's section voice). */
export function GroupTitle({
  children,
  id,
  aside,
}: {
  children: ReactNode;
  id?: string;
  aside?: ReactNode;
}) {
  return (
    <div className="mb-1 flex min-h-[28px] items-center justify-between gap-3">
      <h3 id={id} className="font-display text-[15px] font-semibold text-bone-50">
        {children}
      </h3>
      {aside}
    </div>
  );
}
