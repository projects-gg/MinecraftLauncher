import { CaretRight, Check } from "@phosphor-icons/react";
import { DropdownMenu, ContextMenu as RadixContextMenu } from "radix-ui";
import {
  type ReactElement,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  useRef,
  useState,
} from "react";
import { returnFocusAfterKeys } from "../input-modality";
import { cn } from "../lib/cn";
import { fromPortal } from "../native-menu";
import { Kbd } from "./bits";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
  /**
   * A choice rather than an action (sort order, a filter): a bone check marks it when `true` (the
   * screen's one gold is its main action, design.md §2). Read out as a checkbox item, or a radio
   * item with `radio` (one choice of a group).
   */
  checked?: boolean;
  radio?: boolean;
  /** A submenu (opens to the side on hover, → or Enter); `onSelect` is not used then. */
  items?: Array<MenuItem | null>;
  /** The keys that do the same outside the menu ("Alt ↑"), shown at the row's end. */
  shortcut?: string;
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

interface MenuParts {
  Item: typeof DropdownMenu.Item | typeof RadixContextMenu.Item;
  Separator: typeof DropdownMenu.Separator | typeof RadixContextMenu.Separator;
  Sub: typeof DropdownMenu.Sub | typeof RadixContextMenu.Sub;
  SubTrigger: typeof DropdownMenu.SubTrigger | typeof RadixContextMenu.SubTrigger;
  SubContent: typeof DropdownMenu.SubContent | typeof RadixContextMenu.SubContent;
  Portal: typeof DropdownMenu.Portal | typeof RadixContextMenu.Portal;
}

const DROPDOWN: MenuParts = {
  Item: DropdownMenu.Item,
  Separator: DropdownMenu.Separator,
  Sub: DropdownMenu.Sub,
  SubTrigger: DropdownMenu.SubTrigger,
  SubContent: DropdownMenu.SubContent,
  Portal: DropdownMenu.Portal,
};

const CONTEXT: MenuParts = {
  Item: RadixContextMenu.Item,
  Separator: RadixContextMenu.Separator,
  Sub: RadixContextMenu.Sub,
  SubTrigger: RadixContextMenu.SubTrigger,
  SubContent: RadixContextMenu.SubContent,
  Portal: RadixContextMenu.Portal,
};

function Icon({ item }: { item: MenuItem }) {
  return item.icon ? (
    <span className={cn("flex", item.danger ? "text-flare-400" : "text-ash-300")}>{item.icon}</span>
  ) : null;
}

/** The rows of a menu; both menus share them (one look for "more" buttons and right clicks). */
function menuRows(items: Array<MenuItem | null>, parts: MenuParts): ReactNode[] {
  const { Item, Separator, Sub, SubTrigger, SubContent, Portal } = parts;
  return items.map((item, i) => {
    if (item === null) {
      // biome-ignore lint/suspicious/noArrayIndexKey: separators have no identity
      return <Separator key={`sep-${i}`} className="my-1.5 h-px bg-white/8" />;
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
              className={cn(contentClass, "max-h-[360px] overflow-y-auto")}
            >
              {menuRows(item.items, parts)}
            </SubContent>
          </Portal>
        </Sub>
      );
    }
    return (
      <Item
        key={item.label}
        disabled={item.disabled}
        onSelect={item.onSelect}
        // A closing menu leaves the focus alone: when the item opened a sheet or a dialog, the
        // layer coming up under the pointer made Radix focus the fading menu again, and the
        // layer's focus trap took the focus back as keyboard focus (rings on every tab after a
        // plain click; POLISH6).
        onPointerLeave={(event) => {
          const menu = event.currentTarget.closest("[role=menu]");
          if (menu?.getAttribute("data-state") === "closed") event.preventDefault();
        }}
        role={
          item.checked === undefined
            ? "menuitem"
            : item.radio
              ? "menuitemradio"
              : "menuitemcheckbox"
        }
        aria-checked={item.checked}
        className={cn(
          rowClass,
          item.danger
            ? "text-flare-400 data-[highlighted]:bg-flare-400/12"
            : "text-bone-100 data-[highlighted]:bg-white/8",
        )}
      >
        <Icon item={item} />
        <span className="min-w-0 flex-1">{item.label}</span>
        {item.shortcut ? <Kbd>{item.shortcut}</Kbd> : null}
        {item.checked === undefined ? null : (
          <Check
            size={15}
            weight="bold"
            aria-hidden
            className={cn("shrink-0 text-bone-100", !item.checked && "invisible")}
          />
        )}
      </Item>
    );
  });
}

/** A row's height, a separator's and the panel's padding, as `rowClass` and `contentClass` draw them. */
const ROW_PX = 33;
const SEPARATOR_PX = 13;
const PANEL_PX = 10;
/** The strip of small actions with the line under it. */
const STRIP_PX = 50;
/** The title line (`titleClass`). */
const TITLE_PX = 25;
/** How far a menu keeps from the window's edges when it opens beside its trigger. */
const MENU_MARGIN = 8;

/** How tall a menu of these rows opens (no header): measured in the launcher, ±2 px. */
export function menuHeight(
  items: ReadonlyArray<MenuItem | null>,
  strip?: readonly MenuStripItem[],
  title?: string,
): number {
  const rows = items.filter((item) => item !== null).length;
  const separators = items.length - rows;
  return (
    PANEL_PX +
    rows * ROW_PX +
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
          className={cn(contentClass, className)}
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
          className={contentClass}
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
