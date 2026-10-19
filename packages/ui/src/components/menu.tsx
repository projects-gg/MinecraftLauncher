import { Check } from "@phosphor-icons/react";
import { DropdownMenu, ContextMenu as RadixContextMenu } from "radix-ui";
import type { ReactElement, ReactNode } from "react";
import { cn } from "../lib/cn";

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
}

const contentClass =
  "glass-strong pop-motion z-50 min-w-[190px] rounded-[var(--radius-panel)] p-1 shadow-[0_24px_60px_rgb(34_6_6/0.55)]";

/** The rows of a menu; both menus share them (one look for "more" buttons and right clicks). */
function menuRows(
  items: Array<MenuItem | null>,
  Item: typeof DropdownMenu.Item | typeof RadixContextMenu.Item,
  Separator: typeof DropdownMenu.Separator | typeof RadixContextMenu.Separator,
) {
  return items.map((item, i) =>
    item === null ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: separators have no identity
      <Separator key={`sep-${i}`} className="my-1.5 h-px bg-white/8" />
    ) : (
      <Item
        key={item.label}
        disabled={item.disabled}
        onSelect={item.onSelect}
        role={
          item.checked === undefined
            ? "menuitem"
            : item.radio
              ? "menuitemradio"
              : "menuitemcheckbox"
        }
        aria-checked={item.checked}
        className={cn(
          "flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-[7px] text-[13px] outline-none select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-45",
          item.danger
            ? "text-flare-400 data-[highlighted]:bg-flare-400/12"
            : "text-bone-100 data-[highlighted]:bg-white/8",
        )}
      >
        {item.icon ? (
          <span className={cn("flex", item.danger ? "text-flare-400" : "text-ash-300")}>
            {item.icon}
          </span>
        ) : null}
        <span className="min-w-0 flex-1">{item.label}</span>
        {item.checked === undefined ? null : (
          <Check
            size={15}
            weight="bold"
            aria-hidden
            className={cn("shrink-0 text-gold-400", !item.checked && "invisible")}
          />
        )}
      </Item>
    ),
  );
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
        <DropdownMenu.Content align={align} side={side} sideOffset={6} className={contentClass}>
          {menuRows(items, DropdownMenu.Item, DropdownMenu.Separator)}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
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
    <RadixContextMenu.Trigger asChild disabled={disabled}>
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
          onCloseAutoFocus={onCloseAutoFocus}
        >
          {menuRows(items, RadixContextMenu.Item, RadixContextMenu.Separator)}
        </RadixContextMenu.Content>
      </RadixContextMenu.Portal>
    </RadixContextMenu.Root>
  );
}
