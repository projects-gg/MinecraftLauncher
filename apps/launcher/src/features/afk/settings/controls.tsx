// Building blocks of the AFK settings panels, desktop and 7/24 Bulut (SHEET-UX): the Settings page's
// rows (words on the left, the control on the right edge, a hairline between rows), one block per
// feature led by its switch, and folds that show a feature's fields only while its switch is on
// (they fade in place and the rows below glide, never a height animation; docs/design.md §5).
import { Info, WarningCircle } from "@phosphor-icons/react";
import { cn, easeOutExpo, enter, Input, leave, Switch, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatCount } from "@/lib/format";
import { joinAddress, splitAddress } from "../address";
import type { Issue, Limit } from "./model";

/** Translated text of an inline problem. */
export function useIssueText() {
  const { t } = useTranslation();
  return (issue: Issue | undefined): string | null => {
    if (!issue) return null;
    // Under the one address field a bare range would not say it is about the port.
    if (issue.field === "serverPort" && issue.code === "range") return t("afkPanel.portRange");
    return t(`afkSettings.errors.${issue.code}`, {
      min: issue.limit ? formatCount(issue.limit.min) : undefined,
      max: issue.limit ? formatCount(issue.limit.max) : undefined,
    });
  };
}

const glideTransition = { layout: { duration: 0.3, ease: easeOutExpo } };

/**
 * What a row, a block or a fold passes to motion: it glides to its new place when a fold above it
 * opens or closes; in Performance mode and under reduced motion it is simply there.
 */
export function useGlide() {
  const { layout } = useMotionPrefs();
  return layout
    ? ({ layout: "position", transition: glideTransition } as const)
    : ({ layout: false } as const);
}

/** The section's title at the top of its column, with a count or a link on the right. */
export function SectionTitle({ title, aside }: { title: string; aside?: ReactNode }) {
  return (
    <header className="mb-1 flex min-h-8 items-center justify-between gap-3">
      <h2 className="font-display text-[18px] leading-tight font-semibold text-bone-50">{title}</h2>
      {aside}
    </header>
  );
}

/**
 * One feature: its lead row and the rows under it. Blocks part with a hairline and a little more
 * room than rows do, so a block reads as a group and not as one more row.
 */
export function Block({ children, className }: { children: ReactNode; className?: string }) {
  const glide = useGlide();
  return (
    <motion.section
      {...glide}
      // The first block follows the section's title: no hairline above it.
      className={cn(
        "relative mt-2 border-t border-white/9 pt-2 first-of-type:mt-0 first-of-type:border-t-0 first-of-type:pt-0",
        className,
      )}
    >
      {children}
    </motion.section>
  );
}

/**
 * A switch's own fields, only while it is on: they stand a step in (so they read as that switch's,
 * the block's lead or a row inside it), fade in place (popped out of the flow as they leave) and
 * every row below glides.
 */
export function Fold({ open, children }: { open: boolean; children: ReactNode }) {
  const glide = useGlide();
  return (
    <AnimatePresence initial={false} mode="popLayout">
      {open ? (
        <motion.div
          key="fold"
          {...glide}
          className="pl-5"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.22) }}
          exit={{ opacity: 0, transition: leave(0.16) }}
        >
          {children}
        </motion.div>
      ) : null}
    </AnimatePresence>
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

const rowLine = "border-t border-white/6";

/**
 * One setting: its words on the left, its control on the right edge (every control of a section
 * ends on the same line). `htmlFor` makes the title the control's label. A problem goes under the
 * whole row.
 */
export function SettingRow({
  title,
  help,
  htmlFor,
  error,
  children,
  className,
  lead = false,
}: {
  title: string;
  help?: ReactNode;
  htmlFor?: string;
  error?: string | null;
  children: ReactNode;
  className?: string;
  /** The block's first row: no hairline above it. */
  lead?: boolean;
}) {
  const glide = useGlide();
  return (
    <motion.div {...glide} className={cn(!lead && rowLine, "py-3", className)}>
      <div className="flex items-center justify-between gap-6">
        <div className="min-w-0">
          {htmlFor ? (
            <label htmlFor={htmlFor} className="block text-[13.5px] font-medium text-bone-100">
              {title}
            </label>
          ) : (
            <p className="text-[13.5px] font-medium text-bone-100">{title}</p>
          )}
          {help ? <p className="mt-0.5 max-w-[52ch] text-[12.5px] text-ash-400">{help}</p> : null}
        </div>
        <div className="flex shrink-0 items-center gap-2">{children}</div>
      </div>
      {error ? (
        <div className="mt-1.5">
          <ErrorText>{error}</ErrorText>
        </div>
      ) : null}
    </motion.div>
  );
}

/** A setting whose field needs the column's width (an address, a list of lines). */
export function WideRow({
  children,
  className,
  lead = false,
}: {
  children: ReactNode;
  className?: string;
  /** The block's first row: no hairline above it. */
  lead?: boolean;
}) {
  const glide = useGlide();
  return (
    <motion.div {...glide} className={cn(!lead && rowLine, "py-3", className)}>
      {children}
    </motion.div>
  );
}

/**
 * Title and help on the left, switch on the right. `lead`: the block's own switch (its feature's
 * name two steps larger than a row's, no hairline above).
 */
export function ToggleRow({
  title,
  help,
  checked,
  onCheckedChange,
  disabled,
  lead = false,
}: {
  title: string;
  help?: ReactNode;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  lead?: boolean;
}) {
  const id = useId();
  const glide = useGlide();
  return (
    <motion.div
      {...glide}
      className={cn(
        // `switch-row`: the switch lifts while the pointer is on the title too (`hovered:`).
        "switch-row flex items-center justify-between gap-6",
        lead ? "py-3.5" : cn(rowLine, "py-3"),
      )}
    >
      <div className="min-w-0">
        {/* The title is the switch's label: a click on it toggles too (the help may hold links).
            A switch that cannot be used dims with its title; its help stays readable, since it
            often says why. */}
        <label
          htmlFor={id}
          className={cn(
            "block",
            lead
              ? "text-[15.5px] leading-snug font-semibold text-bone-50"
              : "text-[13.5px] font-medium text-bone-100",
            disabled ? "cursor-default opacity-55" : "cursor-pointer",
          )}
        >
          {title}
        </label>
        {help ? (
          <p id={`${id}-help`} className="mt-0.5 max-w-[52ch] text-[12.5px] text-ash-400">
            {help}
          </p>
        ) : null}
      </div>
      <Switch
        id={id}
        describedBy={help ? `${id}-help` : undefined}
        checked={checked}
        onCheckedChange={onCheckedChange}
        label={title}
        hideLabel
        disabled={disabled}
      />
    </motion.div>
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
        tone === "warn" ? "text-bone-100" : "text-ash-400",
        className,
      )}
    >
      <Icon
        size={15}
        className={cn("mt-px shrink-0", tone === "warn" && "text-gold-300")}
        weight={tone === "warn" ? "fill" : "regular"}
      />
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
 * is not a number. Turkish decimal comma and dot are both accepted. In a row it is compact (its
 * label read out, not shown; the row says the problem).
 */
export function NumberField({
  label,
  value,
  onChange,
  limit,
  unit,
  invalid,
  className,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  limit: Limit;
  unit?: string;
  invalid?: boolean;
  className?: string;
}) {
  const [text, setText] = useState(() => show(value));
  // Follow outside changes (reset, swap) without fighting the user's typing.
  useEffect(() => {
    setText((current) => (Object.is(parse(current), value) ? current : show(value)));
  }, [value]);
  return (
    <div className={cn("relative w-[104px]", className)}>
      <input
        aria-label={label}
        inputMode={limit.integer ? "numeric" : "decimal"}
        autoComplete="off"
        spellCheck={false}
        value={text}
        aria-invalid={invalid ? true : undefined}
        onChange={(e) => {
          setText(e.target.value);
          onChange(parse(e.target.value));
        }}
        className={cn(
          inputBase,
          "h-[34px] px-3 tabular-nums",
          unit && "pr-11",
          invalid && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
        )}
      />
      {unit ? (
        <span className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[12.5px] text-ash-400">
          {unit}
        </span>
      ) : null}
    </div>
  );
}

/** Two numbers of one setting (least and most seconds), side by side at the row's end. */
export function RangeRow({
  title,
  help,
  errors,
  children,
}: {
  title: string;
  help?: ReactNode;
  /** The two fields' problems, shown under the row. */
  errors: Array<string | null | undefined>;
  children: [ReactNode, ReactNode];
}) {
  const error = errors.find(Boolean) ?? null;
  return (
    <SettingRow title={title} help={help} error={error}>
      {children[0]}
      <span aria-hidden className="text-ash-400">
        –
      </span>
      {children[1]}
    </SettingRow>
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
      <label htmlFor={id} className="text-[13.5px] font-medium text-bone-100">
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

/**
 * The server's address as Minecraft's server list takes it, the port after a colon when the server
 * needs one ("mc.sunucu.net:25570"). What is typed stays as typed; host and port go to the draft.
 */
export function AddressField({
  id,
  label,
  host,
  port,
  onChange,
  help,
  error,
  invalid,
  placeholder,
}: {
  id: string;
  label: string;
  host: string;
  /** 0 or "": the server's own port. */
  port: number | string;
  onChange: (host: string, port: string) => void;
  help?: ReactNode;
  error?: string | null;
  invalid?: boolean;
  placeholder?: string;
}) {
  const [text, setText] = useState(() => joinAddress(host, port));
  // Follow outside changes (a reload) without fighting the typing.
  useEffect(() => {
    setText((current) => {
      const typed = splitAddress(current);
      const same = typed.host === host.trim() && (typed.port || "0") === String(port || "0");
      return same ? current : joinAddress(host, port);
    });
  }, [host, port]);
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-[13.5px] font-medium text-bone-100">
        {label}
      </label>
      <input
        id={id}
        value={text}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete="off"
        aria-invalid={invalid || error ? true : undefined}
        onChange={(e) => {
          setText(e.target.value);
          const next = splitAddress(e.target.value);
          onChange(next.host, next.port);
        }}
        className={cn(
          inputBase,
          "h-[34px] px-3",
          (invalid || error) &&
            "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
        )}
      />
      {error ? (
        <ErrorText>{error}</ErrorText>
      ) : help ? (
        <div className="text-[12.5px] text-ash-400">{help}</div>
      ) : null}
    </div>
  );
}

/** A quiet way under a field, underlined so it reads as an action (afk.projects.gg's look). */
export function TextAction({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="cursor-pointer self-start rounded-sm text-left text-[12.5px] font-medium text-ash-300 underline decoration-white/25 underline-offset-[3px] transition-colors duration-150 outline-none hover:text-bone-100 hover:decoration-current focus-visible:ring-2 focus-visible:ring-gold-300"
    >
      {children}
    </button>
  );
}

/**
 * An auto-reply rule's trigger, desktop and 7/24 Bulut, as on afk.projects.gg: a plain phrase by
 * default ("Mesajda geçen söz"). A pattern written by hand is the one advanced way ("Kalıbı kendim
 * yazacağım"), monospace with its one line of help; "Düz söze dön" goes back. The owner, 25 Oct:
 * players have no technical knowledge, so a "Düzenli ifade" switch in every rule gave way to this.
 */
export function TriggerField({
  id,
  value,
  regex,
  onChange,
  placeholder,
  error,
  maxLength,
  autoFocus,
}: {
  id: string;
  value: string;
  regex: boolean;
  /** The trigger's words and whether they are a pattern. */
  onChange: (value: string, regex: boolean) => void;
  placeholder: string;
  error: string | null;
  maxLength: number;
  autoFocus?: boolean;
}) {
  const { t } = useTranslation();
  // The pattern given up for a plain phrase comes back with "Kalıbı kendim yazacağım" (this visit).
  const kept = useRef<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const helpId = useId();
  const switchWay = () => {
    if (regex) {
      kept.current = value;
      onChange("", false);
    } else {
      onChange(kept.current ?? value, true);
      kept.current = null;
    }
    requestAnimationFrame(() => input.current?.focus());
  };
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <label htmlFor={id} className="text-[13px] font-medium text-bone-100">
        {regex ? t("afkPanel.rule.pattern") : t("afkPanel.rule.match")}
      </label>
      <Input
        ref={input}
        id={id}
        value={value}
        placeholder={regex ? t("afkPanel.rule.patternPlaceholder") : placeholder}
        spellCheck={false}
        autoComplete="off"
        maxLength={maxLength}
        invalid={Boolean(error)}
        autoFocus={autoFocus}
        aria-describedby={regex ? helpId : undefined}
        onChange={(event) => onChange(event.target.value, regex)}
        className={cn("h-9", regex && "font-mono text-[13px]")}
      />
      {error ? <ErrorText>{error}</ErrorText> : null}
      {regex ? (
        <p id={helpId} className="text-[12px] leading-snug text-ash-400">
          {t("afkPanel.rule.patternHelp")}
        </p>
      ) : null}
      <TextAction onClick={switchWay}>
        {regex ? t("afkPanel.rule.toPlain") : t("afkPanel.rule.writeMyself")}
      </TextAction>
    </div>
  );
}

export const inputClass = inputBase;
