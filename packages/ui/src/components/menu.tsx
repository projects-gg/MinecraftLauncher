import { CaretRight, Check } from "@phosphor-icons/react";
import { DropdownMenu, ContextMenu as RadixContextMenu } from "radix-ui";
import {
  type ReactElement,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { returnFocusAfterKeys } from "../input-modality";
import { cn } from "../lib/cn";
import { fromPortal } from "../native-menu";
import { Kbd } from "./bits";
import { SliderValue } from "./forms";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
  /**
   * A choice rather than an action (sort order, a filter): a bone check marks it when `true` (the
   * screen's one gold is its main action, design.md §2). Read out as a checkbox item, or a radio
   * item with `radio` (one choice of a group). Space checks it and keeps the menu open (APG);
   * Enter and a click check it and close the menu.
   */
  checked?: boolean;
  radio?: boolean;
  /**
   * A group's name over the rows after it, up to the next line (`null`) or the next name: never a
   * row (no focus, no select; `onSelect` is not used). Its rows are read as a group with this name
   * ("Kare Hızı" over 15 / 30 / 60 fps).
   */
  heading?: boolean;
  /** A submenu (opens to the side on hover, → or Enter); `onSelect` is not used then. */
  items?: Array<MenuItem | null>;
  /** The keys that do the same outside the menu ("Alt ↑"), shown at the row's end. */
  shortcut?: string;
  /**
   * A value the row sets in place (a person's volume): the label and the value on one line, a
   * track under them. The menu stays open while it changes; `onSelect` is not used then.
   */
  slider?: MenuSlider;
  /**
   * One quiet sentence under the label: why a dimmed row is off ("Bu bilgisayarda en çok
   * Normal."). With `disabled` the row stays in the arrows' reach (APG: a disabled item is
   * focusable), reads its reason with its name, and a press on it does nothing.
   */
  hint?: string;
}

/** A row's value (`MenuItem.slider`). */
export interface MenuSlider {
  value: number;
  min: number;
  max: number;
  /** ← and → move it this far; PageUp and PageDown ten times as far. */
  step: number;
  /** A mark on the track (a volume's 100 %): a drag that comes this close lands on it. */
  notch?: number;
  /** The value as shown and read out ("%140"). */
  format: (value: number) => string;
  /**
   * Each change; `final` once a drag lets go, or once key steps rest (half a second without one,
   * leaving the row or the menu closing): keep the value then, not at each step.
   */
  onChange: (value: number, final: boolean) => void;
}

/**
 * A row of small actions above a menu's rows (a chat message's quick reactions): each one a menu
 * item of its own, reached with the arrows like the rows below, its `content` drawn in a square.
 */
export interface MenuStripItem {
  key: string;
  /** Its accessible name ("👍 ile tepki ver"). */
  label: string;
  content: ReactNode;
  onSelect: () => void;
  /** It is the player's choice already (a reaction they gave): the quiet plate (a chosen chip). */
  on?: boolean;
}

const contentClass =
  "glass-strong pop-motion z-50 min-w-[190px] rounded-[var(--radius-panel)] p-1 shadow-[0_24px_60px_rgb(34_6_6/0.55)]";

/**
 * A menu never runs past the window (QOL-209): taller than the room Radix found on its side (a
 * short window, a trigger low on the screen), its rows scroll inside it with the app's thin bar.
 * Menus that fit are unchanged.
 */
const dropdownFits = "max-h-[var(--radix-dropdown-menu-content-available-height)] overflow-y-auto";
const contextFits = "max-h-[var(--radix-context-menu-content-available-height)] overflow-y-auto";

/** The strip of small actions and the line under it. */
function stripRow(strip: readonly MenuStripItem[] | undefined, parts: MenuParts): ReactNode {
  if (!strip?.length) return null;
  const { Item, Separator } = parts;
  return [
    <div key="strip" className="flex items-center gap-0.5 px-0.5 pt-0.5">
      {strip.map((item) => (
        <Item
          key={item.key}
          aria-label={item.label}
          onSelect={item.onSelect}
          className={cn(
            "chamfer flex size-[34px] cursor-pointer items-center justify-center outline-none select-none [--cut:6px]",
            "transition-[background-color,transform,scale] duration-150 data-[highlighted]:bg-white/10 active:scale-[0.92]",
            item.on && "bg-white/14 data-[highlighted]:bg-white/18",
          )}
        >
          {item.content}
        </Item>
      ))}
    </div>,
    <Separator key="strip-line" className="my-1.5 h-px bg-white/8" />,
  ];
}

/**
 * `onCloseAutoFocus` of both menus: the trigger gets the focus back only after keyboard use
 * (after a plain click it lit up with its ring and popped its tooltip; `input-modality.ts`).
 */
function focusBackAfterKeys(event: Event, own?: (event: Event) => void) {
  own?.(event);
  returnFocusAfterKeys(event);
}

const rowClass =
  "flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-[7px] text-[13px] outline-none select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-45";

/** Whose rows they are, above them (`title`), in the card title's face: never a row. */
const titleClass =
  "max-w-[240px] truncate px-2.5 pt-1.5 pb-1 font-display text-[13px] leading-[16px] font-semibold text-bone-50 select-none";

/** A group's name over its rows (`MenuItem.heading`): quiet, never a row. */
const headingClass =
  "px-2.5 pt-1.5 pb-0.5 text-[12px] leading-[16px] font-semibold text-ash-400 select-none";

interface MenuParts {
  Item: typeof DropdownMenu.Item | typeof RadixContextMenu.Item;
  Separator: typeof DropdownMenu.Separator | typeof RadixContextMenu.Separator;
  Sub: typeof DropdownMenu.Sub | typeof RadixContextMenu.Sub;
  SubTrigger: typeof DropdownMenu.SubTrigger | typeof RadixContextMenu.SubTrigger;
  SubContent: typeof DropdownMenu.SubContent | typeof RadixContextMenu.SubContent;
  Portal: typeof DropdownMenu.Portal | typeof RadixContextMenu.Portal;
  Group: typeof DropdownMenu.Group | typeof RadixContextMenu.Group;
  Label: typeof DropdownMenu.Label | typeof RadixContextMenu.Label;
}

const DROPDOWN: MenuParts = {
  Item: DropdownMenu.Item,
  Separator: DropdownMenu.Separator,
  Sub: DropdownMenu.Sub,
  SubTrigger: DropdownMenu.SubTrigger,
  SubContent: DropdownMenu.SubContent,
  Portal: DropdownMenu.Portal,
  Group: DropdownMenu.Group,
  Label: DropdownMenu.Label,
};

const CONTEXT: MenuParts = {
  Item: RadixContextMenu.Item,
  Separator: RadixContextMenu.Separator,
  Sub: RadixContextMenu.Sub,
  SubTrigger: RadixContextMenu.SubTrigger,
  SubContent: RadixContextMenu.SubContent,
  Portal: RadixContextMenu.Portal,
  Group: RadixContextMenu.Group,
  Label: RadixContextMenu.Label,
};

function Icon({ item, className }: { item: MenuItem; className?: string }) {
  return item.icon ? (
    <span className={cn("flex", item.danger ? "text-flare-400" : "text-ash-300", className)}>
      {item.icon}
    </span>
  ) : null;
}

/** How close (in the track's own units, of its whole range) a drag lands on the notch. */
const NOTCH_PULL = 0.02;

/** Keys a slider row takes for itself (the menu keeps ↑ and ↓ for its rows). */
function keyStep(key: string, step: number, min: number, max: number, value: number) {
  switch (key) {
    case "ArrowRight":
      return value + step;
    case "ArrowLeft":
      return value - step;
    case "PageUp":
      return value + step * 10;
    case "PageDown":
      return value - step * 10;
    case "Home":
      return min;
    case "End":
      return max;
    default:
      return null;
  }
}

/** How long a key step waits for the next one before its value is final (kept, once). */
const KEY_SETTLE_MS = 500;

/**
 * A row that sets a value: its label and value, and a track the pointer drags (it lands on the
 * notch when close) or ← → change. Selecting it keeps the menu open. The value follows the pointer
 * at once (no transition: it is the pointer's, not a motion), the fill drawn with `scaleX`. A drag
 * is final when it lets go; key steps once they rest (half a second, leaving the row, the menu
 * closing), not at each press (each final value is a settings write).
 */
function SliderRow({
  item,
  Item,
}: {
  item: MenuItem & { slider: MenuSlider };
  Item: MenuParts["Item"];
}) {
  const { slider } = item;
  const { min, max, step, notch, format, onChange } = slider;
  const [value, setValue] = useState(slider.value);
  const dragging = useRef(false);
  const track = useRef<HTMLSpanElement>(null);
  // The owner's latest callback, and the value key steps left waiting to be final.
  const changed = useRef(onChange);
  changed.current = onChange;
  const keyed = useRef<{ value: number; timer: ReturnType<typeof setTimeout> } | null>(null);
  const settle = () => {
    const steps = keyed.current;
    if (!steps) return;
    keyed.current = null;
    clearTimeout(steps.timer);
    changed.current(steps.value, true);
  };
  // The menu closing ends the steps too (it takes the row away).
  useEffect(
    () => () => {
      const steps = keyed.current;
      if (!steps) return;
      keyed.current = null;
      clearTimeout(steps.timer);
      changed.current(steps.value, true);
    },
    [],
  );
  // A value the owner changes (another device, a reset) shows unless a drag holds it.
  useEffect(() => {
    if (!dragging.current) setValue(slider.value);
  }, [slider.value]);
  const clamp = (v: number) => Math.max(min, Math.min(max, v));
  const set = (next: number, final: boolean) => {
    const v = clamp(next);
    setValue(v);
    onChange(v, final);
  };
  const fromPointer = (clientX: number, final: boolean) => {
    const box = track.current?.getBoundingClientRect();
    if (!box || box.width === 0) return;
    let v = Math.round(min + ((clientX - box.left) / box.width) * (max - min));
    if (notch !== undefined && Math.abs(v - notch) <= (max - min) * NOTCH_PULL) v = notch;
    set(v, final);
  };
  const at = (v: number) => (v - min) / (max - min);
  return (
    <Item
      key={item.label}
      role="slider"
      aria-label={item.label}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      aria-valuetext={format(value)}
      onSelect={(event) => event.preventDefault()}
      onKeyDown={(event) => {
        const next = keyStep(event.key, step, min, max, value);
        if (next === null) return;
        // Its own: the menu's Home/End jumps and a submenu's ← close stay away from it.
        event.preventDefault();
        event.stopPropagation();
        // Each step shows and plays at once; it is final once the steps rest.
        const v = clamp(next);
        set(v, false);
        if (keyed.current) clearTimeout(keyed.current.timer);
        keyed.current = { value: v, timer: setTimeout(settle, KEY_SETTLE_MS) };
      }}
      onBlur={settle}
      className={cn(
        rowClass,
        "cursor-default flex-col items-stretch gap-1 py-2 text-bone-100 data-[highlighted]:bg-white/[0.06]",
      )}
    >
      <span className="flex items-center gap-2.5">
        <Icon item={item} />
        <span className="min-w-0 flex-1">{item.label}</span>
        <span className="font-display text-[13px] font-semibold text-bone-50 tabular-nums">
          <SliderValue text={format(value)} />
        </span>
      </span>
      <span
        ref={track}
        className="relative mx-[7px] h-4 cursor-pointer touch-none"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          // A drag takes over from key steps: its letting go is final.
          if (keyed.current) clearTimeout(keyed.current.timer);
          keyed.current = null;
          dragging.current = true;
          event.currentTarget.setPointerCapture(event.pointerId);
          fromPointer(event.clientX, false);
        }}
        onPointerMove={(event) => {
          if (dragging.current) fromPointer(event.clientX, false);
        }}
        onPointerUp={(event) => {
          if (!dragging.current) return;
          dragging.current = false;
          fromPointer(event.clientX, true);
        }}
        onPointerCancel={() => {
          if (!dragging.current) return;
          dragging.current = false;
          onChange(value, true);
        }}
      >
        <span className="absolute inset-x-0 top-1/2 h-1 -translate-y-1/2 overflow-hidden rounded-full bg-white/10">
          <span
            className="absolute inset-0 origin-left bg-bone-100"
            style={{ transform: `scaleX(${at(value)})` }}
          />
        </span>
        {notch !== undefined ? (
          <span
            aria-hidden
            className="absolute top-1/2 h-2.5 w-0.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white/40"
            style={{ left: `${at(notch) * 100}%` }}
          />
        ) : null}
        <span
          aria-hidden
          className="absolute top-1/2 size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-bone-100 bg-obsidian-900 shadow-[0_2px_6px_rgb(34_6_6/0.6)]"
          style={{ left: `${at(value) * 100}%` }}
        />
      </span>
    </Item>
  );
}

/**
 * The rows of a menu; both menus share them (one look for "more" buttons and right clicks). A
 * group's name (`heading`) and the rows after it, up to the next line or name, make one group.
 */
function menuRows(items: Array<MenuItem | null>, parts: MenuParts): ReactNode[] {
  const { Group, Label } = parts;
  const out: ReactNode[] = [];
  for (let i = 0; i < items.length; i += 1) {
    const item = items[i] ?? null;
    if (!item?.heading) {
      out.push(menuRow(item, i, parts));
      continue;
    }
    let end = i + 1;
    while (end < items.length && items[end] && !items[end]?.heading) end += 1;
    const rows = items.slice(i + 1, end);
    out.push(
      <Group key={`group-${item.label}`} aria-label={item.label}>
        <Label aria-hidden className={headingClass}>
          {item.label}
        </Label>
        {rows.map((row, j) => menuRow(row, i + 1 + j, parts))}
      </Group>,
    );
    i = end - 1;
  }
  return out;
}

/** One row: a line, a slider, a submenu, or an action or choice. */
function menuRow(item: MenuItem | null, i: number, parts: MenuParts): ReactNode {
  const { Item, Separator, Sub, SubTrigger, SubContent, Portal } = parts;
  if (item === null) {
    // Separators have no identity: their place is their key.
    return <Separator key={`sep-${i}`} className="my-1.5 h-px bg-white/8" />;
  }
  if (item.slider) {
    return <SliderRow key={item.label} item={{ ...item, slider: item.slider }} Item={Item} />;
  }
  if (item.items) {
    return (
      <Sub key={item.label}>
        <SubTrigger
          disabled={item.disabled}
          className={cn(
            rowClass,
            "text-bone-100 data-[highlighted]:bg-white/8 data-[state=open]:bg-white/8",
          )}
        >
          <Icon item={item} />
          <span className="min-w-0 flex-1">{item.label}</span>
          <CaretRight size={13} weight="bold" aria-hidden className="shrink-0 text-ash-400" />
        </SubTrigger>
        <Portal>
          <SubContent
            sideOffset={6}
            alignOffset={-4}
            collisionPadding={8}
            // Two named groups of choices fit before it scrolls (a share's "Gelişmiş").
            className={cn(contentClass, "max-h-[420px] overflow-y-auto")}
          >
            {menuRows(item.items, parts)}
          </SubContent>
        </Portal>
      </Sub>
    );
  }
  // A dimmed row that says why (`hint`) stays reachable and does nothing when pressed.
  const hinted = item.hint !== undefined;
  const reasoned = item.disabled === true && hinted;
  // APG: Space checks a checkbox or radio row and leaves the menu open; Enter checks and closes.
  const checks = item.checked !== undefined && !reasoned;
  return (
    <Item
      key={item.label}
      disabled={item.disabled && !reasoned}
      aria-disabled={reasoned || undefined}
      onSelect={reasoned ? (event) => event.preventDefault() : item.onSelect}
      onKeyDown={
        checks
          ? (event) => {
              if (event.key !== " ") return;
              event.preventDefault();
              item.onSelect();
            }
          : undefined
      }
      // A closing menu leaves the focus alone: when the item opened a sheet or a dialog, the
      // layer coming up under the pointer made Radix focus the fading menu again, and the
      // layer's focus trap took the focus back as keyboard focus (rings on every tab after a
      // plain click; POLISH6).
      onPointerLeave={(event) => {
        const menu = event.currentTarget.closest("[role=menu]");
        if (menu?.getAttribute("data-state") === "closed") event.preventDefault();
      }}
      role={
        item.checked === undefined ? "menuitem" : item.radio ? "menuitemradio" : "menuitemcheckbox"
      }
      aria-checked={item.checked}
      className={cn(
        rowClass,
        hinted && "items-start",
        item.danger
          ? "text-flare-400 data-[highlighted]:bg-flare-400/12"
          : "text-bone-100 data-[highlighted]:bg-white/8",
        reasoned && "cursor-default data-[highlighted]:bg-white/[0.04]",
      )}
    >
      <Icon item={item} className={cn(hinted && "mt-[2px]", reasoned && "opacity-45")} />
      {hinted ? (
        <span className="min-w-0 flex-1">
          <span className={cn("block", reasoned && "opacity-45")}>{item.label}</span>
          <span className="mt-0.5 block text-[12px] leading-[1.35] text-ash-400">{item.hint}</span>
        </span>
      ) : (
        <span className="min-w-0 flex-1">{item.label}</span>
      )}
      {item.shortcut ? <Kbd>{item.shortcut}</Kbd> : null}
      {item.checked === undefined ? null : (
        <Check
          size={15}
          weight="bold"
          aria-hidden
          className={cn(
            "shrink-0 text-bone-100",
            hinted && "mt-[2px]",
            !item.checked && "invisible",
          )}
        />
      )}
    </Item>
  );
}

/** A row's height, a separator's and the panel's padding, as `rowClass` and `contentClass` draw them. */
const ROW_PX = 33;
/** A slider row (`SliderRow`): its line, the track and their padding. */
const SLIDER_PX = 58;
const SEPARATOR_PX = 13;
const PANEL_PX = 10;
/** The strip of small actions with the line under it. */
const STRIP_PX = 50;
/** The title line (`titleClass`). */
const TITLE_PX = 25;
/** A row's reason under its label (`MenuItem.hint`). */
const HINT_PX = 18;
/** A group's name (`MenuItem.heading`, `headingClass`). */
const HEADING_PX = 24;
/** How far a menu keeps from the window's edges when it opens beside its trigger. */
const MENU_MARGIN = 8;

/** How tall a menu of these rows opens (no header): measured in the launcher, ±2 px. */
export function menuHeight(
  items: ReadonlyArray<MenuItem | null>,
  strip?: readonly MenuStripItem[],
  title?: string,
): number {
  const rows = items.filter((item) => item !== null).length;
  const sliders = items.filter((item) => item?.slider).length;
  const hints = items.filter((item) => item?.hint !== undefined && !item.slider).length;
  const headings = items.filter((item) => item?.heading).length;
  const separators = items.length - rows;
  return (
    PANEL_PX +
    rows * ROW_PX -
    headings * (ROW_PX - HEADING_PX) +
    sliders * (SLIDER_PX - ROW_PX) +
    hints * HINT_PX +
    separators * SEPARATOR_PX +
    (strip?.length ? STRIP_PX : 0) +
    (title ? TITLE_PX : 0)
  );
}

/**
 * Where a menu opens below its trigger, or beside it when the whole menu has no room below: to
 * the trigger's left for a menu aligned to its end, else to its right, its bottom level with the
 * trigger's, so it grows upwards from the trigger (pushed down only where the window ends). A menu
 * turned upwards over a grid of cards sat under the next card's own "⋯" and read as that card's
 * (FEAT-L); shifted to fit, its top floated far above the trigger (SHEET-UX second look).
 */
export function placeBelowOrBeside(
  trigger: { bottom: number },
  viewportHeight: number,
  height: number,
  align: "start" | "center" | "end",
  sideOffset: number,
): { side: "bottom" | "left" | "right"; align: "start" | "center" | "end" } {
  const room = viewportHeight - trigger.bottom - sideOffset - MENU_MARGIN;
  if (room >= height) return { side: "bottom", align };
  return { side: align === "start" ? "right" : "left", align: "end" };
}

/** Action menu for "more" buttons and instance cards. Use `null` for a separator. */
export function Menu({
  trigger,
  items,
  strip,
  align = "end",
  side = "bottom",
  sideOffset = 6,
  collisionPadding,
  open,
  onOpenChange,
  onCloseAutoFocus,
  header,
  title,
  label,
  className,
  besideWhenNoRoom = false,
}: {
  trigger: ReactElement;
  items: Array<MenuItem | null>;
  /** Small actions in a row above the rows. */
  strip?: readonly MenuStripItem[];
  /**
   * Whose rows they are, in quiet words at the top (an account's name on its card's menu: opened
   * beside its button, a menu can sit over the next card; SHEET-UX). Also its accessible name
   * unless `label` is given.
   */
  title?: string;
  align?: "start" | "center" | "end";
  side?: "top" | "right" | "bottom" | "left";
  sideOffset?: number;
  /**
   * How far from the window's edges the menu keeps (Radix's default 0); per side when a menu must
   * stay within its owner's edges (the inventory's slot menu within the game's window).
   */
  collisionPadding?: number | Partial<Record<"top" | "right" | "bottom" | "left", number>>;
  /**
   * Opened and closed by its owner instead of by the trigger (the inventory opens a slot's menu
   * on a long press or Shift+F10, under the slot, with a hidden trigger there).
   */
  open?: boolean;
  /** When it opens and closes (a hover card holds itself open meanwhile). */
  onOpenChange?: (open: boolean) => void;
  /** When the menu has closed; `preventDefault()` keeps the focus from going back. */
  onCloseAutoFocus?: (event: Event) => void;
  /** What the rows act on, above them (the inventory's item tooltip); never a row itself. */
  header?: ReactNode;
  /** The menu's accessible name, when the rows alone do not say what they act on. */
  label?: string;
  /** A class for the menu's surface (the inventory's slot menu is solid over the game's window). */
  className?: string;
  /**
   * With `side="bottom"`: when the whole menu has no room below the trigger, it opens beside it
   * (`placeBelowOrBeside`) instead of turning upwards (cards in a grid, where it would sit under
   * the next card's own trigger).
   */
  besideWhenNoRoom?: boolean;
}) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [placed, setPlaced] = useState<ReturnType<typeof placeBelowOrBeside> | null>(null);
  const changeOpen = (next: boolean) => {
    // Decided once as it opens (before Radix places it), so it never jumps from above to beside.
    const element = triggerRef.current;
    if (next && besideWhenNoRoom && side === "bottom" && element) {
      const view = element.ownerDocument.defaultView;
      setPlaced(
        placeBelowOrBeside(
          element.getBoundingClientRect(),
          view?.innerHeight ?? Number.POSITIVE_INFINITY,
          menuHeight(items, strip, title),
          align,
          sideOffset,
        ),
      );
    }
    onOpenChange?.(next);
  };
  const at = besideWhenNoRoom ? placed : null;
  const name = label ?? title;
  return (
    <DropdownMenu.Root open={open} onOpenChange={changeOpen}>
      <DropdownMenu.Trigger asChild ref={triggerRef}>
        {trigger}
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align={at ? at.align : align}
          side={at ? at.side : side}
          sideOffset={sideOffset}
          collisionPadding={at && at.side !== "bottom" ? MENU_MARGIN : collisionPadding}
          className={cn(contentClass, dropdownFits, className)}
          // Radix names the menu after its trigger; a label of its own replaces that name.
          {...(name ? { "aria-label": name, "aria-labelledby": undefined } : {})}
          onCloseAutoFocus={(event) => focusBackAfterKeys(event, onCloseAutoFocus)}
        >
          {title ? (
            <DropdownMenu.Label aria-hidden className={titleClass}>
              {title}
            </DropdownMenu.Label>
          ) : null}
          {header}
          {stripRow(strip, DROPDOWN)}
          {menuRows(items, DROPDOWN)}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

/**
 * A right click that only passes through the trigger on React's tree (it started in an open menu or
 * a dialog rendered inside it) opens nothing: Radix opens the menu unless the event is prevented.
 */
function skipPortalClicks(event: ReactMouseEvent<HTMLElement>) {
  const selected = event.currentTarget.ownerDocument.getSelection()?.toString() ?? "";
  if (fromPortal(event.currentTarget, event.target, selected)) event.preventDefault();
}

/**
 * The right-click menu a desktop player expects on rows and tiles (screenshots, worlds, servers):
 * the same rows and look as `Menu`, opening at the pointer. Keep the same actions reachable
 * without it (a "more" button), since nothing tells the player it is there.
 */
export function ContextMenu({
  children,
  items,
  strip,
  title,
  disabled,
  wrapTrigger,
  onCloseAutoFocus,
  onOpenChange,
}: {
  /** The element that opens the menu on right click (one element; it receives the handlers). */
  children: ReactElement;
  items: Array<MenuItem | null>;
  /** Small actions in a row above the rows (a chat message's quick reactions). */
  strip?: readonly MenuStripItem[];
  /** Whose rows they are, at the top and as the menu's name, as on `Menu`. */
  title?: string;
  disabled?: boolean;
  /**
   * Puts something around the trigger that also needs the element, such as a `Tooltip`: the
   * trigger passes props and refs through, so both reach the element.
   */
  wrapTrigger?: (trigger: ReactElement) => ReactElement;
  /** When the menu has closed; `preventDefault()` keeps the focus from going back. */
  onCloseAutoFocus?: (event: Event) => void;
  /** When it opens and closes (the inventory shows the slot's tooltip meanwhile). */
  onOpenChange?: (open: boolean) => void;
}) {
  const trigger = (
    <RadixContextMenu.Trigger asChild disabled={disabled} onContextMenu={skipPortalClicks}>
      {children}
    </RadixContextMenu.Trigger>
  );
  return (
    <RadixContextMenu.Root onOpenChange={onOpenChange}>
      {wrapTrigger ? wrapTrigger(trigger) : trigger}
      <RadixContextMenu.Portal>
        <RadixContextMenu.Content
          collisionPadding={8}
          className={cn(contentClass, contextFits)}
          {...(title ? { "aria-label": title } : {})}
          onCloseAutoFocus={(event) => focusBackAfterKeys(event, onCloseAutoFocus)}
        >
          {title ? (
            <RadixContextMenu.Label aria-hidden className={titleClass}>
              {title}
            </RadixContextMenu.Label>
          ) : null}
          {stripRow(strip, CONTEXT)}
          {menuRows(items, CONTEXT)}
        </RadixContextMenu.Content>
      </RadixContextMenu.Portal>
    </RadixContextMenu.Root>
  );
}
