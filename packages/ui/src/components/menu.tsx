import { DropdownMenu } from "radix-ui";
import type { ReactElement, ReactNode } from "react";
import { cn } from "../lib/cn";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
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
          className="glass-strong z-50 min-w-[200px] rounded-[var(--radius-panel)] p-1.5 shadow-[0_24px_60px_rgb(34_6_6/0.55)]"
        >
          {items.map((item, i) =>
            item === null ? (
              // biome-ignore lint/suspicious/noArrayIndexKey: separators have no identity
              <DropdownMenu.Separator key={`sep-${i}`} className="my-1.5 h-px bg-white/8" />
            ) : (
              <DropdownMenu.Item
                key={item.label}
                disabled={item.disabled}
                onSelect={item.onSelect}
                className={cn(
                  "flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2 text-[13px] outline-none select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-45",
                  item.danger
                    ? "text-flare-400 data-[highlighted]:bg-flare-400/12"
                    : "text-bone-100 data-[highlighted]:bg-white/8",
                )}
              >
                {item.icon ? <span className="flex text-ash-300">{item.icon}</span> : null}
                {item.label}
              </DropdownMenu.Item>
            ),
          )}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
