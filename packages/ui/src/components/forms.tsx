import { CaretDown, Check, MagnifyingGlass, X } from "@phosphor-icons/react";
import { Popover, Select as RadixSelect, Slider as RadixSlider, ToggleGroup } from "radix-ui";
import {
  type CSSProperties,
  createContext,
  forwardRef,
  type InputHTMLAttributes,
  memo,
  type ReactNode,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useTransition,
} from "react";
import { type Glide, glidePlate, PLATE_CUT } from "../glide";
import { cn } from "../lib/cn";
import { matchesQuery } from "../search";
import { Chip } from "./bits";

/** What a `Field` tells the control inside it: the id of its help or error line, and whether it
 *  shows an error. */
export interface FieldInfo {
  descriptionId?: string;
  invalid: boolean;
}

const FieldContext = createContext<FieldInfo>({ invalid: false });

/** The surrounding `Field`'s help or error line (for `aria-describedby`) and error state. */
export function useField(): FieldInfo {
  return useContext(FieldContext);
}

/**
 * Label above, helper or error below (never placeholder-as-label). The control inside is
 * described by the line below it (`useField`; Input and Studio's date field use it), so a screen
 * reader says "Boşsa hemen başlar." with the field, not only to someone who can see it.
 */
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
  const lineId = useId();
  const described = Boolean(error || help);
  const info = useMemo(
    () => ({ descriptionId: described ? lineId : undefined, invalid: Boolean(error) }),
    [described, lineId, error],
  );
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label htmlFor={htmlFor} className="text-[13px] font-medium text-bone-100">
        {label}
      </label>
      <FieldContext.Provider value={info}>{children}</FieldContext.Provider>
      {error ? (
        <p id={lineId} role="alert" className="text-[12.5px] text-flare-400">
          {error}
        </p>
      ) : help ? (
        <p id={lineId} className="text-[12.5px] text-ash-400">
          {help}
        </p>
      ) : null}
    </div>
  );
}

export const Input = forwardRef<
  HTMLInputElement,
  InputHTMLAttributes<HTMLInputElement> & { invalid?: boolean }
>(function Input({ className, invalid, ...props }, ref) {
  const field = useField();
  return (
    <input
      ref={ref}
      aria-invalid={invalid || undefined}
      aria-describedby={field.descriptionId}
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
  /**
   * A short quiet tag after the label ("Önerilen"), also shown in the closed select. Neutral, never
   * gold (AFK-CLEAR): the gold is the page's main action and the chosen plates.
   */
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
    // 11 px: nothing is smaller (design guide §3). Neutral, the words quieter than the label's.
    <Chip tone="neutral" className="ml-2 h-[18px] px-1.5 align-[1px] text-[11px] text-ash-300">
      {text}
    </Chip>
  );
}

/**
 * A long list (Minecraft versions, loader versions): a search field and plain rows in a popover,
 * drawn only while it is open. Radix's select mounts every row as a full item with its own
 * context, collection entry and hidden native option when it opens: 103 versions cost a 94 ms
 * task per opening, snapshots many times that. Arrow keys move, Enter picks, Esc closes, typing
 * filters; the focus stays in the field (`aria-activedescendant`). Modal, as the short select is:
 * a press outside only closes it (it pressed "Kaydet" under it before), and a sheet around it
 * stays open (SHEET-UX).
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
      modal
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
  placeholder,
}: {
  value: T;
  onValueChange: (value: T) => void;
  options: SelectOption<T>[];
  label: string;
  className?: string;
  disabled?: boolean;
  searchable?: { searchLabel: string; emptyText: string };
  /** Shown while `value` is "" (nothing chosen yet), in the placeholder grey. */
  placeholder?: string;
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
      <RadixSelect.Trigger
        aria-label={label}
        className={cn(triggerClass, "data-[placeholder]:text-ash-500", className)}
      >
        {/* A long choice is cut with an ellipsis: the trigger keeps its one 34 px line. */}
        <span className="min-w-0 truncate whitespace-nowrap">
          <RadixSelect.Value placeholder={placeholder} />
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

/**
 * About when the plate has arrived after a pick (within a few pixels on its spring), for work
 * that should wait for it: the language picker's window cross-fade (a view transition) freezes
 * the screen while it starts and fades out a picture of the old one, so a plate still on its way
 * would stop and fade out beside the arriving one.
 */
export const SEGMENTED_GLIDE_MS = 170;

/** One choice of a `Segmented`. */
interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  icon?: ReactNode;
  hint?: string;
  lang?: string;
  hintLang?: string;
}

/** The option's cut corners, shared by its plate and its focus ring (`chamfer`). */
const optionCut = { "--cut": `${PLATE_CUT}px` } as CSSProperties;

/**
 * The plate's fill and the labels' ink over it: gold in the launcher. An app restyles them with
 * `--segmented-plate` and `--segmented-ink` (Studio's chosen option is a quiet plate: its one gold
 * is the top bar's Yayınla, docs/design.md §9).
 */
const PLATE_FILL = "bg-[color:var(--segmented-plate,var(--color-gold-500))]";
const PLATE_INK = "text-[color:var(--segmented-ink,var(--color-gold-ink))]";

/**
 * An option's plate, shown under the chosen one at rest. While the plate glides
 * (`data-gliding` on the control) the window over the row takes its place (glide.ts).
 */
function OptionPlate({ on }: { on: boolean }) {
  return (
    <span
      aria-hidden
      data-on={on ? "" : undefined}
      className={cn(
        "chamfer pointer-events-none absolute inset-0 -z-10 hidden data-[on]:block group-data-[gliding]/control:invisible",
        PLATE_FILL,
      )}
    />
  );
}

/**
 * Mutually exclusive options shown inline (loader kind, view mode, release channel). An option's
 * `hint` follows its label in a quieter weight ("Otomatik (Türkçe)"); `lang` and `hintLang` name
 * the language of text in another language than the page's, so screen readers say it right.
 * `onIntent` runs when the pointer or the focus reaches the control (to load what a pick needs
 * before the click). `disabled` keeps the choice on screen and dims it (150 ms, like the other
 * controls); say why next to it.
 *
 * A pick glides the gold plate along the row to the chosen option (glide.ts): measured inside the
 * control, so a dialog that changes size or moves with the choice never carries the plate along,
 * and a pick during a glide starts from where the plate is. Labels are dark exactly where the plate
 * is over them. It glides in Performance mode too (it answers the player's own click and runs on
 * the compositor); under reduced motion the plate is simply there and the labels fade.
 */
export function Segmented<T extends string>({
  value,
  onValueChange,
  options,
  label,
  className,
  onIntent,
  disabled = false,
  describedBy,
}: {
  value: T;
  onValueChange: (value: T) => void;
  options: Array<SegmentedOption<T>>;
  label: string;
  className?: string;
  onIntent?: () => void;
  disabled?: boolean;
  /** Id of a line that describes the control (what the choice does, or why it waits). */
  describedBy?: string;
}) {
  // A pick shows at once and the caller's update follows as a transition: what the pick brings
  // (a dialog's next tab, a long list) renders after the plate has set off, so a slow render never
  // holds the plate back, and the page stays responsive while it renders. A caller that keeps its
  // value sees the plate glide back.
  const [settling, startTransition] = useTransition();
  const [picked, setPicked] = useState<T | null>(null);
  const current = settling && picked !== null ? picked : value;
  const control = useRef<HTMLDivElement>(null);
  // The value whose option shows the plate, and the glide under way.
  const shown = useRef(current);
  const glide = useRef<Glide | null>(null);
  useLayoutEffect(() => {
    const from = shown.current;
    if (from === current) return;
    shown.current = current;
    // Before the first paint of the pick: the new option's plate starts where the old one is.
    const element = control.current;
    glide.current = element ? glidePlate(element, from, current, glide.current) : null;
  }, [current]);
  return (
    <ToggleGroup.Root
      ref={control}
      type="single"
      value={current}
      onValueChange={(v) => {
        if (!v) return;
        setPicked(v as T);
        startTransition(() => onValueChange(v as T));
      }}
      onPointerEnter={onIntent}
      onFocus={onIntent}
      aria-label={label}
      aria-describedby={describedBy}
      disabled={disabled}
      className={cn(
        "group/control relative isolate inline-flex rounded-md border border-white/10 bg-obsidian-850/80 p-[3px] transition-opacity duration-150",
        disabled && "pointer-events-none opacity-45",
        className,
      )}
    >
      {/* One stacking context for the control: the plate at rest stays under every label. */}
      {options.map((o) => (
        <ToggleGroup.Item
          key={o.value}
          value={o.value}
          data-option={o.value}
          style={optionCut}
          className={cn(
            "group/option relative flex h-[26px] cursor-pointer items-center px-3 text-[13px] font-medium text-ash-300 transition-colors duration-150",
            "hover:text-bone-50 focus-visible:outline-none",
          )}
        >
          <OptionPlate on={current === o.value} />
          {/* The keyboard ring never touches gold. Around the chosen option's plate it sits outside,
              as on buttons (`chamfer-ring`, its 2 px gap over the control's padding and border); an
              option without the plate keeps it inside, 1 px in from the edge, so a dark line parts
              it from a neighbour's plate. */}
          {current === o.value ? (
            <span
              aria-hidden
              className="chamfer-ring pointer-events-none absolute -z-10 bg-gold-300 opacity-0 group-focus-visible/option:opacity-100"
            />
          ) : (
            <span
              aria-hidden
              className="chamfer pointer-events-none absolute inset-0 -z-10 opacity-0 shadow-[inset_0_0_0_1px_var(--color-obsidian-900),inset_0_0_0_3px_var(--color-gold-300)] group-focus-visible/option:opacity-100"
            />
          )}
          {/* The label twice in one cell: in its own colour and in the plate's ink. */}
          <span className="grid">
            <OptionLabel option={o} />
            <OptionLabel option={o} ink />
          </span>
        </ToggleGroup.Item>
      ))}
      {/* The plate while it glides (glide.ts): a window over the row, gold, with every label in
          ink where the labels are. Two clipping layers hold its two edges and cut corners. */}
      <span
        aria-hidden
        data-glide=""
        style={optionCut}
        className="pointer-events-none invisible absolute inset-0 z-10 group-data-[gliding]/control:visible"
      >
        <span data-glide-right="" className="chamfer-end absolute">
          <span data-glide-left="" className={cn("chamfer-start absolute h-full", PLATE_FILL)}>
            <span data-glide-ink="" className={cn("absolute text-[13px] font-medium", PLATE_INK)}>
              {options.map((o) => (
                <span
                  key={o.value}
                  data-glide-label=""
                  className="absolute flex items-center gap-1.5 whitespace-nowrap"
                >
                  <LabelContent option={o} />
                </span>
              ))}
            </span>
          </span>
        </span>
      </span>
    </ToggleGroup.Root>
  );
}

/**
 * One copy of an option's label. The chosen option shows the ink copy at rest; while the plate
 * glides, every option shows its own colour and the window carries the ink (glide.ts). Under
 * reduced motion the two copies fade (150 ms). The ink copy is hidden from screen readers.
 */
function OptionLabel<T extends string>({
  option,
  ink = false,
}: {
  option: SegmentedOption<T>;
  ink?: boolean;
}) {
  return (
    <span
      data-label={ink ? "ink" : "base"}
      aria-hidden={ink || undefined}
      className={cn(
        "flex items-center gap-1.5 whitespace-nowrap [grid-area:1/1] motion-reduce:transition-opacity motion-reduce:duration-150",
        ink
          ? cn(
              PLATE_INK,
              "opacity-0 group-data-[state=on]/option:opacity-100 group-data-[gliding]/control:opacity-0!",
            )
          : "group-data-[state=on]/option:opacity-0 group-data-[gliding]/control:opacity-100!",
      )}
    >
      <LabelContent option={option} />
    </span>
  );
}

function LabelContent<T extends string>({ option }: { option: SegmentedOption<T> }) {
  return (
    <>
      {option.icon}
      <span lang={option.lang}>{option.label}</span>
      {option.hint ? (
        <span lang={option.hintLang} className="-ml-0.5 font-normal opacity-75">
          {option.hint}
        </span>
      ) : null}
    </>
  );
}

/** A slider's value with its "%" in the UI font: Tektur's sign reads like a slash (the launcher's
 * live percentages set it the same way). */
function SliderValue({ text }: { text: string }) {
  const at = text.indexOf("%");
  if (at < 0) return text;
  return (
    <>
      {text.slice(0, at)}
      <span className="font-sans font-medium">%</span>
      {text.slice(at + 1)}
    </>
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
        {/* The value in bone, not gold: the gold is the track's fill and the page's main action
            (AFK-CLEAR; Studio's one-gold rule, design.md §9). */}
        <span className="font-display text-[15px] font-semibold text-bone-50 tabular-nums">
          {format ? <SliderValue text={format(value)} /> : value}
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
