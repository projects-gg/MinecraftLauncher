import { motion } from "motion/react";
import { Tabs as RadixTabs } from "radix-ui";
import { type ReactNode, useId } from "react";
import { cn } from "../lib/cn";

export interface TabItem<T extends string> {
  value: T;
  label: string;
  count?: number;
}

/** Underline tabs with a gold indicator that slides between items. */
export function Tabs<T extends string>({
  value,
  onValueChange,
  items,
  children,
  label,
  className,
}: {
  value: T;
  onValueChange: (value: T) => void;
  items: TabItem<T>[];
  children: ReactNode;
  label: string;
  className?: string;
}) {
  const layoutId = useId();
  return (
    <RadixTabs.Root
      value={value}
      onValueChange={(v) => onValueChange(v as T)}
      className={cn("flex min-h-0 flex-1 flex-col", className)}
    >
      <RadixTabs.List aria-label={label} className="flex shrink-0 gap-1 border-b border-white/8">
        {items.map((item) => (
          <RadixTabs.Trigger
            key={item.value}
            value={item.value}
            className="group relative flex h-10 cursor-pointer items-center gap-1.5 px-3 text-[13.5px] font-medium text-ash-400 transition-colors hover:text-bone-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300 data-[state=active]:text-bone-50"
          >
            {item.label}
            {typeof item.count === "number" ? (
              <span className="rounded bg-white/8 px-1.5 text-[11px] text-ash-300 tabular-nums">
                {item.count}
              </span>
            ) : null}
            {value === item.value ? (
              <motion.span
                layoutId={layoutId}
                className="absolute inset-x-2 -bottom-px h-[2px] bg-gold-400"
                transition={{ type: "spring", stiffness: 480, damping: 38 }}
              />
            ) : null}
          </RadixTabs.Trigger>
        ))}
      </RadixTabs.List>
      {children}
    </RadixTabs.Root>
  );
}

export function TabPanel({
  value,
  children,
  className,
}: {
  value: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <RadixTabs.Content
      value={value}
      className={cn("min-h-0 flex-1 overflow-y-auto pt-5 outline-none", className)}
    >
      {children}
    </RadixTabs.Content>
  );
}
