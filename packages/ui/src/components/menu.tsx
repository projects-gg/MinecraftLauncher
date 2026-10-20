import { CaretRight, Check } from "@phosphor-icons/react";
import { DropdownMenu, ContextMenu as RadixContextMenu } from "radix-ui";
import type { ReactElement, MouseEvent as ReactMouseEvent, ReactNode } from "react";
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
   * A choice rather than an action (sort order, a filter): a gold check marks it when `true`.
   * Read out as a checkbox item, or a radio item with `radio` (one choice of a group).
   */
  checked?: boolean;
  radio?: boolean;
  /** A submenu (opens to the side on hover, → or Enter); `onSelect` is not used then. */
  items?: Array<MenuItem | null>;
  /** The keys that do the same outside the menu ("Alt ↑"), shown at the row's end. */
  shortcut?: string;
}

const contentClass =
  "glass-strong pop-motion z-50 min-w-[190px] rounded-[var(--radius-panel)] p-1 shadow-[0_24px_60px_rgb(34_6_6/0.55)]";

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
            className={cn("shrink-0 text-gold-400", !item.checked && "invisible")}
          />
        )}
      </Item>
    );
  });
}

/** Action menu for "more" buttons and instance cards. Use `null` for a separator. */
export function Menu({
  trigger,
  items,
  align = "end",
  side = "bottom",
}: {
  trigger: ReactElement;
  items: Array<MenuItem | null>;
  align?: "start" | "center" | "end";
  side?: "top" | "right" | "bottom" | "left";
}) {
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>{trigger}</DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align={align}
          side={side}
          sideOffset={6}
          className={contentClass}
          onCloseAutoFocus={(event) => focusBackAfterKeys(event)}
        >
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
  disabled,
  wrapTrigger,
  onCloseAutoFocus,
}: {
  /** The element that opens the menu on right click (one element; it receives the handlers). */
  children: ReactElement;
  items: Array<MenuItem | null>;
  disabled?: boolean;
  /**
   * Puts something around the trigger that also needs the element, such as a `Tooltip`: the
   * trigger passes props and refs through, so both reach the element.
   */
  wrapTrigger?: (trigger: ReactElement) => ReactElement;
  /** When the menu has closed; `preventDefault()` keeps the focus from going back. */
  onCloseAutoFocus?: (event: Event) => void;
}) {
  const trigger = (
    <RadixContextMenu.Trigger asChild disabled={disabled} onContextMenu={skipPortalClicks}>
      {children}
    </RadixContextMenu.Trigger>
  );
  return (
    <RadixContextMenu.Root>
      {wrapTrigger ? wrapTrigger(trigger) : trigger}
      <RadixContextMenu.Portal>
        <RadixContextMenu.Content
          collisionPadding={8}
          className={contentClass}
          onCloseAutoFocus={(event) => focusBackAfterKeys(event, onCloseAutoFocus)}
        >
          {menuRows(items, CONTEXT)}
        </RadixContextMenu.Content>
      </RadixContextMenu.Portal>
    </RadixContextMenu.Root>
  );
}
