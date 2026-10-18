import { CaretDown, Check, MagnifyingGlass, X } from "@phosphor-icons/react";
import { MotionConfig, motion } from "motion/react";
import { Popover, Select as RadixSelect, Slider as RadixSlider, ToggleGroup } from "radix-ui";
import {
  forwardRef,
  type InputHTMLAttributes,
  memo,
  type ReactNode,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { cn } from "../lib/cn";
import { matchesQuery } from "../search";
import { Chip } from "./bits";

/** Label above, helper or error below (never placeholder-as-label). */
export function Field({
  label,
  help,
  error,
  children,
  htmlFor,
  className,
}: {
  label: string;
  help?: ReactNode;
  error?: string | null;
  children: ReactNode;
  htmlFor?: string;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label htmlFor={htmlFor} className="text-[13px] font-medium text-bone-100">
        {label}
      </label>
      {children}
      {error ? (
        <p role="alert" className="text-[12.5px] text-flare-400">
          {error}
        </p>
      ) : help ? (
        <p className="text-[12.5px] text-ash-400">{help}</p>
      ) : null}
    </div>
  );
}

export const Input = forwardRef<
  HTMLInputElement,
  InputHTMLAttributes<HTMLInputElement> & { invalid?: boolean }
>(function Input({ className, invalid, ...props }, ref) {
  return (
    <input
      ref={ref}
      aria-invalid={invalid || undefined}
      className={cn(
        "h-[34px] w-full rounded-md border border-white/10 bg-obsidian-800/80 px-3 text-[13.5px] text-bone-50 transition-[border-color,box-shadow,opacity] duration-150 placeholder:text-ash-500",
        "hover:border-white/18 focus:border-gold-400/70 focus:outline-none focus-visible:outline-none focus:ring-2 focus:ring-gold-400/25",
        "disabled:cursor-not-allowed disabled:opacity-45 disabled:hover:border-white/10",
        invalid && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
        className,
      )}
      {...props}
    />
  );
});

/**
 * Search field. While it has text, the first Esc clears it (a dialog or sheet around it closes on
 * the next one): Radix listens for Esc on the document in the capture phase, so the field listens
 * on the window, which comes first. `clearLabel` adds a clear (×) button while there is text.
 */
export const SearchInput = forwardRef<
  HTMLInputElement,
  {
    value: string;
    onChange: (value: string) => void;
    placeholder: string;
    label: string;
    className?: string;
    autoFocus?: boolean;
    /** Accessible name of the clear button ("Aramayı temizle"); without it there is none. */
    clearLabel?: string;
  }
>(function SearchInput(
  { value, onChange, placeholder, label, className, autoFocus, clearLabel },
  ref,
) {
  const input = useRef<HTMLInputElement | null>(null);
  const latest = useRef({ value, onChange });
  latest.current = { value, onChange };
  const [focused, setFocused] = useState(false);
  useEffect(() => {
    if (!focused) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !latest.current.value) return;
      event.preventDefault();
      event.stopPropagation();
      latest.current.onChange("");
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [focused]);
  const setRefs = (node: HTMLInputElement | null) => {
    input.current = node;
    if (typeof ref === "function") ref(node);
    else if (ref) ref.current = node;
  };
  const clearable = Boolean(clearLabel && value);
  return (
    <div className={cn("relative", className)}>
      <MagnifyingGlass
        size={16}
        className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-ash-400"
      />
      <Input
        ref={setRefs}
        aria-label={label}
        value={value}
        // Only where the caller asks (a search dialog opening).
        autoFocus={autoFocus}
        onChange={(e) => onChange(e.target.value)}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        placeholder={placeholder}
        className={cn("pl-9", clearable && "pr-9")}
      />
      {clearable ? (
        <button
          type="button"
          aria-label={clearLabel}
          onClick={() => {
            onChange("");
            input.current?.focus();
          }}
          className="hit-area absolute top-1/2 right-1.5 flex size-6 -translate-y-1/2 cursor-pointer items-center justify-center rounded text-ash-400 transition-[background-color,color] duration-150 hover:bg-white/8 hover:text-bone-100"
        >
          <X size={14} weight="bold" />
        </button>
      ) : null}
    </div>
  );
});

export interface SelectOption<T extends string> {
  value: T;
  label: string;
  hint?: string;
  /** A short gold tag after the label ("Önerilen"), also shown in the closed select. */
  badge?: string;
}

/** One row of the open list. Memoized: a parent's re-render does not redraw every row. */
const SelectItem = memo(function SelectItem({
  value,
  label,
  hint,
  badge,
}: {
  value: string;
  label: string;
  hint?: string;
  badge?: string;
}) {
  return (
    <RadixSelect.Item
      value={value}
      className="relative flex cursor-pointer items-center gap-2 rounded-md py-[7px] pr-8 pl-2.5 text-[13px] text-bone-100 outline-none select-none data-[highlighted]:bg-white/8"
    >
      <span className="min-w-0 flex-1">
        <RadixSelect.ItemText>
          {label}
          {badge ? <Badge text={badge} /> : null}
        </RadixSelect.ItemText>
        {hint ? <span className="block text-[12px] text-ash-400">{hint}</span> : null}
      </span>
      <RadixSelect.ItemIndicator className="absolute right-2">
        <Check size={15} className="text-gold-400" />
      </RadixSelect.ItemIndicator>
    </RadixSelect.Item>
  );
});

const triggerClass =
  "flex h-[34px] w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-white/10 bg-obsidian-800/80 px-3 text-left text-[13.5px] text-bone-50 transition-[border-color,box-shadow,opacity] duration-150 hover:border-white/18 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none data-[disabled]:opacity-50 disabled:cursor-default disabled:opacity-50";

function Badge({ text }: { text: string }) {
  return (
    <Chip tone="gold" className="ml-2 h-[18px] px-1.5 align-[1px] text-[10.5px]">
      {text}
    </Chip>
  );
}

/**
 * A long list (Minecraft versions, loader versions): a search field and plain rows in a popover,
 * drawn only while it is open. Radix's select mounts every row as a full item with its own
 * context, collection entry and hidden native option when it opens: 103 versions cost a 94 ms
 * task per opening, snapshots many times that. Arrow keys move, Enter picks, Esc closes, typing
 * filters; the focus stays in the field (`aria-activedescendant`).
 */
function SearchableSelect<T extends string>({
  value,
  onValueChange,
  options,
  label,
  className,
  disabled,
  searchLabel,
  emptyText,
}: {
  value: T;
  onValueChange: (value: T) => void;
  options: SelectOption<T>[];
  label: string;
  className?: string;
  disabled?: boolean;
  searchLabel: string;
  emptyText: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const selected = options.find((o) => o.value === value);
  // Turkish folding like every search ("iron" finds "Iron", "isik" finds "Işık").
  const rows = useMemo(
    () => (query.trim() ? options.filter((o) => matchesQuery(o.label, query)) : options),
    [options, query],
  );
  const index = Math.min(active, Math.max(0, rows.length - 1));

  // Keeps the active row in view by scrolling the list only, once the popover has its size.
  useEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() => {
      const list = listRef.current;
      const row = list?.querySelector<HTMLElement>(`[data-index="${index}"]`);
      if (!list || !row) return;
      if (row.offsetTop < list.scrollTop) list.scrollTop = row.offsetTop;
      else if (row.offsetTop + row.offsetHeight > list.scrollTop + list.clientHeight) {
        list.scrollTop = row.offsetTop + row.offsetHeight - list.clientHeight;
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [index, open]);

  const pick = (next: T) => {
    onValueChange(next);
    setOpen(false);
  };
  const move = (to: number) => setActive(Math.min(rows.length - 1, Math.max(0, to)));

  return (
    <Popover.Root
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        // Opening starts on the current value.
        if (next) {
          setQuery("");
          setActive(
            Math.max(
              0,
              options.findIndex((o) => o.value === value),
            ),
          );
        }
      }}
    >
      <Popover.Trigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-haspopup="listbox"
          disabled={disabled}
          className={cn(triggerClass, className)}
        >
          <span className="min-w-0 truncate whitespace-nowrap">
            {selected?.label}
            {selected?.badge ? <Badge text={selected.badge} /> : null}
          </span>
          <CaretDown size={14} className="shrink-0 text-ash-400" />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={6}
          collisionPadding={12}
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            inputRef.current?.focus();
          }}
          className="glass-strong pop-motion z-50 flex max-h-[min(380px,var(--radix-popover-content-available-height))] w-[var(--radix-popover-trigger-width)] min-w-[220px] flex-col rounded-[var(--radius-panel)] p-1 shadow-[0_24px_60px_rgb(34_6_6/0.55)]"
        >
          <div className="relative shrink-0 p-1">
            <MagnifyingGlass
              size={16}
              className="pointer-events-none absolute top-1/2 left-4 -translate-y-1/2 text-ash-400"
            />
            <Input
              ref={inputRef}
              role="combobox"
              aria-label={searchLabel}
              aria-expanded={open}
              aria-controls={listId}
              aria-autocomplete="list"
              aria-activedescendant={rows[index] ? `${listId}-${index}` : undefined}
              value={query}
              placeholder={searchLabel}
              className="pl-9"
              spellCheck={false}
              onChange={(event) => {
                // Typing starts from the best match.
                setQuery(event.target.value);
                setActive(0);
              }}
              onKeyDown={(event) => {
                const keys: Record<string, number> = {
                  ArrowDown: index + 1,
                  ArrowUp: index - 1,
                  PageDown: index + 8,
                  PageUp: index - 8,
                };
                if (event.key in keys) {
                  event.preventDefault();
                  move(keys[event.key] ?? index);
                } else if (event.key === "Enter") {
                  event.preventDefault();
                  const row = rows[index];
                  if (row) pick(row.value);
                }
              }}
            />
          </div>
          <div
            ref={listRef}
            id={listId}
            role="listbox"
            aria-label={label}
            tabIndex={-1}
            className="relative min-h-0 overflow-y-auto px-1 pb-1"
          >
            {rows.map((o, i) => (
              <button
                key={o.value}
                id={`${listId}-${i}`}
                type="button"
                role="option"
                aria-selected={o.value === value}
                tabIndex={-1}
                data-index={i}
                onMouseMove={() => i !== index && setActive(i)}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => pick(o.value)}
                className={cn(
                  "relative flex w-full cursor-pointer items-center rounded-md py-[7px] pr-8 pl-2.5 text-left text-[13px] text-bone-100 outline-none select-none",
                  i === index && "bg-white/8",
                )}
              >
                <span className="min-w-0 flex-1 tabular-nums">
                  {o.label}
                  {o.badge ? <Badge text={o.badge} /> : null}
                  {o.hint ? <span className="block text-[12px] text-ash-400">{o.hint}</span> : null}
                </span>
                {o.value === value ? (
                  <Check size={15} className="absolute right-2 text-gold-400" aria-hidden />
                ) : null}
              </button>
            ))}
          </div>
          {rows.length === 0 ? (
            <p className="px-3 pt-1 pb-2.5 text-[13px] text-ash-400">{emptyText}</p>
          ) : null}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

/**
 * Closed, the select renders only the chosen row: Radix draws every item of a closed select into
 * a detached fragment, three times on mount (a list of 100 Minecraft versions cost a 80 ms task
 * when a dialog opened, and every keystroke elsewhere in the dialog redrew them). `searchable`
 * swaps in the long-list picker above (pass `searchLabel` and `emptyText` with it).
 */
export function Select<T extends string>({
  value,
  onValueChange,
  options,
  label,
  className,
  disabled,
  searchable,
}: {
  value: T;
  onValueChange: (value: T) => void;
  options: SelectOption<T>[];
  label: string;
  className?: string;
  disabled?: boolean;
  searchable?: { searchLabel: string; emptyText: string };
}) {
  const [open, setOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const closeTimer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(closeTimer.current), []);
  if (searchable) {
    return (
      <SearchableSelect
        value={value}
        onValueChange={onValueChange}
        options={options}
        label={label}
        className={className}
        disabled={disabled}
        searchLabel={searchable.searchLabel}
        emptyText={searchable.emptyText}
      />
    );
  }
  // The whole list stays while the closing animation plays (110 ms), then only the chosen row.
  const shown = open || closing ? options : options.filter((o) => o.value === value);
  return (
    <RadixSelect.Root
      value={value}
      onValueChange={(v) => onValueChange(v as T)}
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        window.clearTimeout(closeTimer.current);
        setClosing(!next);
        if (!next) closeTimer.current = window.setTimeout(() => setClosing(false), 200);
      }}
      disabled={disabled}
    >
      <RadixSelect.Trigger aria-label={label} className={cn(triggerClass, className)}>
        {/* A long choice is cut with an ellipsis: the trigger keeps its one 34 px line. */}
        <span className="min-w-0 truncate whitespace-nowrap">
          <RadixSelect.Value />
        </span>
        <RadixSelect.Icon className="shrink-0">
          <CaretDown size={14} className="text-ash-400" />
        </RadixSelect.Icon>
      </RadixSelect.Trigger>
      <RadixSelect.Portal>
        <RadixSelect.Content
          position="popper"
          sideOffset={6}
          className="glass-strong pop-motion z-50 max-h-[min(360px,var(--radix-select-content-available-height))] min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-[var(--radius-panel)] p-1 shadow-[0_24px_60px_rgb(34_6_6/0.55)]"
        >
          <RadixSelect.Viewport>
            {shown.map((o) => (
              <SelectItem
                key={o.value}
                value={o.value}
                label={o.label}
                hint={o.hint}
                badge={o.badge}
              />
            ))}
          </RadixSelect.Viewport>
        </RadixSelect.Content>
      </RadixSelect.Portal>
    </RadixSelect.Root>
  );
}

/** The plate's spring (sliding markers, docs/design.md §5). */
const PLATE_SPRING = { type: "spring", stiffness: 480, damping: 38 } as const;

/**
 * About when the plate has arrived after a pick (within a few pixels on its spring), for work
 * that should wait for it: the language picker's window cross-fade (a view transition) freezes
 * the screen while it starts and fades out a picture of the old one, so a plate still on its way
 * would stop and fade out beside the arriving one.
 */
export const SEGMENTED_GLIDE_MS = 170;

/**
 * Mutually exclusive options shown inline (loader kind, view mode, release channel). An option's
 * `hint` follows its label in a quieter weight ("Otomatik (Türkçe)"); `lang` and `hintLang` name
 * the language of text in another language than the page's, so screen readers say it right.
 * `onIntent` runs when the pointer or the focus reaches the control (to load what a pick needs
 * before the click).
 */
export function Segmented<T extends string>({
  value,
  onValueChange,
  options,
  label,
  className,
  onIntent,
}: {
  value: T;
  onValueChange: (value: T) => void;
  options: Array<{
    value: T;
    label: string;
    icon?: ReactNode;
    hint?: string;
    lang?: string;
    hintLang?: string;
  }>;
  label: string;
  className?: string;
  onIntent?: () => void;
}) {
  // The gold plate slides to the chosen option (shared layout id, unique per control).
  const layoutId = useId();
  return (
    <ToggleGroup.Root
      type="single"
      value={value}
      onValueChange={(v) => v && onValueChange(v as T)}
      onPointerEnter={onIntent}
      onFocus={onIntent}
      aria-label={label}
      className={cn(
        "isolate inline-flex rounded-md border border-white/10 bg-obsidian-850/80 p-[3px]",
        className,
      )}
    >
      <MotionConfig reducedMotion="user">
        {/* One stacking context for the control: the moving plate stays under every label.
            The chosen label darkens a beat later, as the plate arrives under it. */}
        {options.map((o) => (
          <ToggleGroup.Item
            key={o.value}
            value={o.value}
            className={cn(
              "group relative flex h-[26px] cursor-pointer items-center px-3 text-[13px] font-medium text-ash-300 transition-colors duration-150 [--cut:5px]",
              "hover:text-bone-50 data-[state=on]:text-gold-ink data-[state=on]:delay-[70ms] focus-visible:outline-none",
            )}
          >
            {value === o.value ? (
              <motion.span
                aria-hidden
                layoutId={layoutId}
                className="chamfer absolute inset-0 -z-10 bg-gold-500"
                transition={PLATE_SPRING}
              />
            ) : null}
            <span
              aria-hidden
              className="chamfer pointer-events-none absolute inset-0 -z-10 opacity-0 shadow-[inset_0_0_0_2px_var(--color-gold-300)] group-focus-visible:opacity-100"
            />
            <span className="flex items-center gap-1.5 whitespace-nowrap">
              {o.icon}
              <span lang={o.lang}>{o.label}</span>
              {o.hint ? (
                <span lang={o.hintLang} className="-ml-0.5 font-normal opacity-75">
                  {o.hint}
                </span>
              ) : null}
            </span>
          </ToggleGroup.Item>
        ))}
      </MotionConfig>
    </ToggleGroup.Root>
  );
}

/** Slider with a visible label and live value. `disabled` shows a value chosen elsewhere
 * (e.g. "Otomatik") on the same track, so unlocking it keeps the thumb in place. */
export function Slider({
  value,
  onValueChange,
  min,
  max,
  step = 1,
  label,
  format,
  marks,
  className,
  disabled = false,
}: {
  value: number;
  onValueChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  label: string;
  format?: (value: number) => string;
  marks?: Array<{ value: number; label: string; tone?: "warn" }>;
  className?: string;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <div className="flex items-baseline justify-between">
        <span id={id} className="text-[13px] font-medium text-bone-100">
          {label}
        </span>
        <span className="font-display text-[15px] font-semibold text-gold-300 tabular-nums">
          {format ? format(value) : value}
        </span>
      </div>
      <RadixSlider.Root
        value={[value]}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onValueChange={([v]) => v !== undefined && onValueChange(v)}
        className="relative flex h-5 w-full cursor-pointer touch-none items-center select-none data-[disabled]:cursor-default"
      >
        <RadixSlider.Track className="relative h-1.5 grow overflow-hidden rounded-full bg-white/10">
          <RadixSlider.Range className="absolute h-full bg-[linear-gradient(90deg,var(--color-gold-600),var(--color-gold-400))] transition-opacity duration-200 data-[disabled]:opacity-50" />
        </RadixSlider.Track>
        <RadixSlider.Thumb
          aria-labelledby={id}
          aria-valuetext={format ? format(value) : undefined}
          className="block size-[18px] rounded-full border-2 border-gold-300 bg-obsidian-900 shadow-[0_2px_8px_rgb(34_6_6/0.6)] transition-[transform,scale,border-color] duration-200 hover:scale-110 focus-visible:ring-4 focus-visible:ring-gold-400/30 focus-visible:outline-none data-[disabled]:scale-75 data-[disabled]:border-ash-400 data-[disabled]:hover:scale-75"
        />
      </RadixSlider.Root>
      {marks ? (
        <div className="relative h-4 text-[11px] text-ash-400">
          {marks.map((m) => (
            <span
              key={m.value}
              className={cn(
                "absolute -translate-x-1/2 whitespace-nowrap tabular-nums",
                m.tone === "warn" && "text-flare-400",
              )}
              style={{ left: `${((m.value - min) / (max - min)) * 100}%` }}
            >
              {m.label}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}
