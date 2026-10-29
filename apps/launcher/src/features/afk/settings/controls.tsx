// Building blocks of the AFK settings panels, desktop and 7/24 Bulut (SHEET-UX): the Settings page's
// rows (words on the left, the control on the right edge, a hairline between rows), one block per
// feature led by its switch, and folds that show a feature's fields only while its switch is on
// (they fade in place and the rows below glide, never a height animation; docs/design.md §5).
import { Info, Plus, WarningCircle } from "@phosphor-icons/react";
import { cn, easeOutExpo, enter, Input, leave, Switch, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  createContext,
  type InputHTMLAttributes,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
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
    // A stop for a refused password, on its line (the 7/24 Bulut panel's words).
    if (issue.code === "wrongPassword") return t("afkPanel.wrongPassword");
    if (issue.code === "wrongPasswordKept") return t("afkPanel.wrongPasswordSealed");
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

const OneGoldScope = createContext(false);

/**
 * A part of the window under the one-gold rule (design.md §2) for what has no bone default of its
 * own yet: its quiet text links are bone, so the screen's gold is its one action (the save,
 * "Devam"). The 7/24 Bulut question rows and setup steps (SETUP-UX-L). Switches are bone everywhere
 * already (@pl/ui's default, BETA-FB).
 */
export function OneGold({ children }: { children: ReactNode }) {
  return <OneGoldScope.Provider value={true}>{children}</OneGoldScope.Provider>;
}

/** Whether this part of the window is under the one-gold rule (`OneGold`). */
export function useOneGold(): boolean {
  return useContext(OneGoldScope);
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
    <p id={id} role="alert" className="whitespace-pre-line text-[12.5px] text-flare-400">
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
  const { t } = useTranslation();
  const error = errors.find(Boolean) ?? null;
  // "5 sn ile 10 sn arası": a random wait between the two, in words (a bare dash did not say it).
  const after = t("afkPanel.rangeAfter");
  return (
    <SettingRow title={title} help={help} error={error}>
      {children[0]}
      <span aria-hidden className="text-[13px] text-ash-300">
        {t("afkPanel.rangeJoin")}
      </span>
      {children[1]}
      {after ? (
        <span aria-hidden className="text-[13px] text-ash-300">
          {after}
        </span>
      ) : null}
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

/**
 * A reply's placeholders shown by name: the stored token (`{u}` on 7/24 Bulut, `$u` for the
 * desktop client) reads "{Yazanın adı}" in the field, typing included, and goes back as it is
 * stored (SHEET-UX; raw codes mid-edit were deleted or broken).
 */
export interface TokenSet {
  toShown: (stored: string) => string;
  toStored: (shown: string) => string;
  /** A whole shown token ("{Yazanın adı}"), as one capturing group. */
  shown: RegExp;
}

const escapeRe = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** 7/24 Bulut: `{u}` the sender's name, `{1}`…`{9}` the pattern's parts. */
export function cloudTokens(sender: string, part: (n: number) => string): TokenSet {
  const names = new Map<string, string>([["u", sender]]);
  for (let n = 1; n <= 9; n++) names.set(String(n), part(n));
  const back = new Map([...names].map(([id, name]) => [`{${name}}`, `{${id}}`]));
  const shown = new RegExp(`(${[...back.keys()].map(escapeRe).join("|")})`);
  return {
    toShown: (stored) => stored.replace(/\{(u|[1-9])\}/g, (_, id: string) => `{${names.get(id)}}`),
    toStored: (text) => text.replace(new RegExp(shown.source, "g"), (m) => back.get(m) ?? m),
    shown,
  };
}

/** The desktop client: `$u` the sender's name. */
export function desktopTokens(sender: string): TokenSet {
  const token = `{${sender}}`;
  return {
    toShown: (stored) => stored.replaceAll("$u", token),
    toStored: (text) => text.replaceAll(token, "$u"),
    shown: new RegExp(`(${escapeRe(token)})`),
  };
}

/** Where every shown token starts and ends in `text`. */
function tokenSpans(text: string, set: TokenSet): Array<[number, number]> {
  return [...text.matchAll(new RegExp(set.shown.source, "g"))].map((m) => [
    m.index ?? 0,
    (m.index ?? 0) + m[0].length,
  ]);
}

/**
 * A plain reply's field with its placeholders as chips, also while typing: the field keeps the
 * readable text ("{Yazanın adı}") and a layer over it draws each token as a chip in the very same
 * letters, so the caret, selection, copy, paste and undo stay the field's own. The caret never
 * rests inside a token, and Backspace or Delete takes a token whole. `value` is the stored text.
 */
export function TokenField({
  id,
  value,
  onChange,
  tokens,
  placeholder,
  invalid,
  inputRef,
  onFocus,
  label,
  maxLength = 256,
  autoFocus,
  className,
  ...rest
}: {
  id: string;
  value: string;
  onChange: (stored: string) => void;
  tokens: TokenSet;
  placeholder?: string;
  invalid?: boolean;
  inputRef?: Ref<HTMLInputElement>;
  onFocus?: () => void;
  /** Its accessible name when no label stands over it. */
  label?: string;
  /** Of the stored text. */
  maxLength?: number;
  autoFocus?: boolean;
  className?: string;
} & Pick<InputHTMLAttributes<HTMLInputElement>, "autoComplete">) {
  const [text, setText] = useState(() => tokens.toShown(value));
  const own = useRef<HTMLInputElement | null>(null);
  const layer = useRef<HTMLSpanElement | null>(null);
  // Follow outside changes (an inserted chip, "Vazgeç") without fighting the typing.
  useEffect(() => {
    setText((current) => (tokens.toStored(current) === value ? current : tokens.toShown(value)));
  }, [value, tokens]);
  const chips = tokenSpans(text, tokens).length > 0;
  // The layer moves with the field's own scroll (a long reply).
  const follow = () => {
    const input = own.current;
    if (input && layer.current)
      layer.current.style.transform = `translateX(${-input.scrollLeft}px)`;
  };
  const commit = (shown: string, caretFromEnd?: number) => {
    const stored = tokens.toStored(shown);
    if (stored.length > maxLength) return;
    const normal = tokens.toShown(stored);
    setText(normal);
    onChange(stored);
    // A typed "{u}" turned into its name: the caret keeps its place from the end.
    if (normal !== shown && caretFromEnd !== undefined) {
      requestAnimationFrame(() => {
        const at = Math.max(0, normal.length - caretFromEnd);
        own.current?.setSelectionRange(at, at);
        follow();
      });
    }
  };
  // The caret steps out of a token (to its nearer end); a selection takes tokens whole.
  const snap = () => {
    const input = own.current;
    if (!input) return;
    const start = input.selectionStart ?? 0;
    const end = input.selectionEnd ?? start;
    let from = start;
    let to = end;
    for (const [a, b] of tokenSpans(input.value, tokens)) {
      if (start === end) {
        if (start > a && start < b) {
          from = start - a < b - start ? a : b;
          to = from;
        }
      } else {
        if (from > a && from < b) from = a;
        if (to > a && to < b) to = b;
      }
    }
    if (from !== start || to !== end) input.setSelectionRange(from, to);
    follow();
  };
  const pieces = text.split(tokens.shown);
  return (
    <div className={cn("relative min-w-0 flex-1", className)}>
      <Input
        {...rest}
        ref={(element) => {
          own.current = element;
          if (typeof inputRef === "function") inputRef(element);
          else if (inputRef) inputRef.current = element;
        }}
        id={id}
        aria-label={label}
        value={text}
        placeholder={placeholder}
        spellCheck={false}
        invalid={invalid}
        autoFocus={autoFocus}
        onFocus={onFocus}
        onSelect={snap}
        onScroll={follow}
        onKeyUp={follow}
        onKeyDown={(event) => deleteWholeToken(event, tokens.shown, (shown) => commit(shown))}
        onChange={(event) => {
          const shown = event.target.value;
          commit(shown, shown.length - (event.target.selectionStart ?? shown.length));
        }}
        // Same letters under and over: no kerning or ligatures that could part them.
        className={cn(
          "h-9 text-[13.5px] [font-kerning:none] [font-variant-ligatures:none]",
          chips && "text-transparent caret-bone-50",
        )}
      />
      {chips ? (
        <span
          aria-hidden
          className="pointer-events-none absolute inset-0 overflow-hidden rounded-md"
        >
          <span
            ref={layer}
            className="absolute inset-y-0 left-px flex items-center px-3 text-[13.5px] whitespace-pre text-bone-50 [font-kerning:none] [font-variant-ligatures:none]"
          >
            {pieces.map((piece, i) =>
              i % 2 === 1 ? (
                <span
                  // biome-ignore lint/suspicious/noArrayIndexKey: the pieces of one text, in order
                  key={i}
                  className="rounded-[3px] bg-white/14 box-decoration-clone"
                >
                  <span className="text-transparent">{piece.slice(0, 1)}</span>
                  {piece.slice(1, -1)}
                  <span className="text-transparent">{piece.slice(-1)}</span>
                </span>
              ) : (
                // biome-ignore lint/suspicious/noArrayIndexKey: the pieces of one text, in order
                <span key={i}>{piece}</span>
              ),
            )}
          </span>
        </span>
      ) : null}
    </div>
  );
}

/**
 * Backspace or Delete beside or inside a placeholder (`{u}`, `$u`) takes the whole of it, so one
 * press never leaves a broken "{u" behind (SHEET-UX). `split` has one capturing group: the token.
 */
export function deleteWholeToken(
  event: KeyboardEvent<HTMLInputElement>,
  split: RegExp,
  onText: (text: string) => void,
) {
  if (event.key !== "Backspace" && event.key !== "Delete") return;
  if (event.ctrlKey || event.altKey || event.metaKey) return;
  const input = event.currentTarget;
  const caret = input.selectionStart ?? 0;
  // A selection is deleted as usual.
  if (caret !== (input.selectionEnd ?? caret)) return;
  const text = input.value;
  for (const match of text.matchAll(new RegExp(split.source, `${split.flags.replace("g", "")}g`))) {
    const from = match.index ?? 0;
    const to = from + match[0].length;
    const hit =
      event.key === "Backspace" ? caret > from && caret <= to : caret >= from && caret < to;
    if (!hit) continue;
    event.preventDefault();
    onText(text.slice(0, from) + text.slice(to));
    requestAnimationFrame(() => input.setSelectionRange(from, from));
    return;
  }
}

/** A chip that puts a placeholder where the reply's caret is ("+ Yazanın adı"). */
export function InsertChip({
  label,
  onClick,
  children,
}: {
  /** Its accessible name, when the words alone do not say they add something. */
  label?: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className="group relative isolate inline-flex h-6 shrink-0 cursor-pointer items-center gap-1 px-2 text-[12px] font-semibold whitespace-nowrap text-bone-100 transition-[transform,scale] duration-150 ease-out [--cut:4px] focus-visible:outline-none active:scale-[0.97]"
    >
      <span
        aria-hidden
        className="chamfer-ring absolute -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible:opacity-100"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-0 -z-10 bg-white/7 transition-colors duration-150 group-hover:bg-white/12"
      />
      <Plus size={11} weight="bold" aria-hidden className="text-ash-300" />
      {children}
    </button>
  );
}

export const inputClass = inputBase;
