// Building blocks of the AFK settings sheet sections.
import { Info, WarningCircle } from "@phosphor-icons/react";
import { cn, Switch } from "@pl/ui";
import { type ReactNode, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Issue, Limit } from "./model";

/** Translated text of an inline problem. */
export function useIssueText() {
  const { t } = useTranslation();
  return (issue: Issue | undefined): string | null => {
    if (!issue) return null;
    return t(`afkSettings.errors.${issue.code}`, {
      min: issue.limit?.min.toLocaleString("tr-TR"),
      max: issue.limit?.max.toLocaleString("tr-TR"),
    });
  };
}

/** Section title; `description` only where the fields alone do not say what the feature does. */
export function SectionIntro({ title, description }: { title: string; description?: string }) {
  return (
    <header className="mb-2">
      <h3 className="font-display text-[18px] leading-tight font-semibold text-bone-50">{title}</h3>
      {description ? (
        <p className="mt-1 max-w-[60ch] text-[13px] text-ash-400">{description}</p>
      ) : null}
    </header>
  );
}

/** "Klasörü aç" inside a line of help: the account folder holds its chat log and alerts. */
export function FolderLink({ onOpen }: { onOpen: () => void }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={onOpen}
      className="cursor-pointer rounded-sm font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300 disabled:cursor-default disabled:no-underline"
    >
      {t("afk.openFolder")}
    </button>
  );
}

/** Title and help on the left, switch on the right (the settings page row). */
export function ToggleRow({
  title,
  help,
  checked,
  onCheckedChange,
  disabled,
  emphasis,
}: {
  title: string;
  help?: ReactNode;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  /** The section's main switch. */
  emphasis?: boolean;
}) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-6 py-3",
        emphasis && "mb-1 border-b border-white/7 pb-4",
        disabled && "opacity-55",
      )}
    >
      <div className="min-w-0">
        <p className={cn("font-medium text-bone-50", emphasis ? "text-[14.5px]" : "text-[13.5px]")}>
          {title}
        </p>
        {help ? <p className="mt-0.5 max-w-[58ch] text-[12.5px] text-ash-400">{help}</p> : null}
      </div>
      <Switch
        checked={checked}
        onCheckedChange={onCheckedChange}
        label={title}
        hideLabel
        disabled={disabled}
      />
    </div>
  );
}

/** Fields that only matter while the section's switch is on. */
export function Dependent({ enabled, children }: { enabled: boolean; children: ReactNode }) {
  return (
    <fieldset
      disabled={!enabled}
      className={cn("m-0 min-w-0 border-0 p-0 transition-opacity", !enabled && "opacity-55")}
    >
      {children}
    </fieldset>
  );
}

export function Hint({
  children,
  tone = "muted",
  className,
}: {
  children: ReactNode;
  tone?: "muted" | "warn";
  className?: string;
}) {
  const Icon = tone === "warn" ? WarningCircle : Info;
  return (
    <p
      className={cn(
        "flex items-start gap-2 text-[12.5px] leading-snug",
        tone === "warn" ? "text-gold-300" : "text-ash-400",
        className,
      )}
    >
      <Icon size={15} className="mt-px shrink-0" weight={tone === "warn" ? "fill" : "regular"} />
      <span>{children}</span>
    </p>
  );
}

export function ErrorText({ children, id }: { children: ReactNode; id?: string }) {
  return (
    <p id={id} role="alert" className="text-[12.5px] text-flare-400">
      {children}
    </p>
  );
}

// The look of the shared Input (34 px, 13.5 px text), for the number and line-list fields.
const inputBase =
  "w-full rounded-md border border-white/10 bg-obsidian-800/80 text-[13.5px] text-bone-50 transition-colors placeholder:text-ash-500 hover:border-white/18 focus:border-gold-400/70 focus:outline-none focus:ring-2 focus:ring-gold-400/25 disabled:cursor-not-allowed";

const parse = (text: string) => {
  const normalized = text.trim().replace(",", ".");
  return normalized === "" ? Number.NaN : Number(normalized);
};

const show = (value: number) => (Number.isFinite(value) ? String(value).replace(".", ",") : "");

/**
 * Numeric input that keeps what is typed (a half-typed "1," stays) and reports NaN while the text
 * is not a number. Turkish decimal comma and dot are both accepted.
 */
export function NumberField({
  label,
  value,
  onChange,
  limit,
  unit,
  error,
  help,
  className,
  hideLabel,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  limit: Limit;
  unit?: string;
  error?: string | null;
  help?: string;
  className?: string;
  hideLabel?: boolean;
}) {
  const id = useId();
  const [text, setText] = useState(() => show(value));
  // Follow outside changes (reset, swap) without fighting the user's typing.
  useEffect(() => {
    setText((current) => (Object.is(parse(current), value) ? current : show(value)));
  }, [value]);
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label
        htmlFor={id}
        className={cn("text-[13px] font-medium text-bone-100", hideLabel && "sr-only")}
      >
        {label}
      </label>
      <div className="relative">
        <input
          id={id}
          inputMode={limit.integer ? "numeric" : "decimal"}
          autoComplete="off"
          spellCheck={false}
          value={text}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? `${id}-error` : undefined}
          onChange={(e) => {
            setText(e.target.value);
            onChange(parse(e.target.value));
          }}
          className={cn(
            inputBase,
            "h-[34px] px-3 tabular-nums",
            unit && "pr-12",
            error && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
          )}
        />
        {unit ? (
          <span className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[12.5px] text-ash-400">
            {unit}
          </span>
        ) : null}
      </div>
      {error ? (
        <ErrorText id={`${id}-error`}>{error}</ErrorText>
      ) : help ? (
        <p className="text-[12.5px] text-ash-400">{help}</p>
      ) : null}
    </div>
  );
}

/** A list edited as text, one entry per line (kick messages, alert words). */
export function LinesField({
  label,
  help,
  value,
  onChange,
  rows = 4,
  placeholder,
}: {
  label: string;
  help?: ReactNode;
  value: string[];
  onChange: (value: string[]) => void;
  rows?: number;
  placeholder?: string;
}) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-[13px] font-medium text-bone-100">
        {label}
      </label>
      <textarea
        id={id}
        rows={rows}
        value={value.join("\n")}
        placeholder={placeholder}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value.split("\n"))}
        className={cn(inputBase, "resize-y px-3 py-2 leading-relaxed")}
      />
      {help ? <div className="text-[12.5px] text-ash-400">{help}</div> : null}
    </div>
  );
}

/** Two numbers side by side under one heading (min/max seconds). */
export function RangeRow({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="py-3">
      <p className="mb-2 text-[13.5px] font-medium text-bone-50">{title}</p>
      <div className="grid max-w-[420px] grid-cols-2 gap-3">{children}</div>
    </div>
  );
}

export const inputClass = inputBase;
