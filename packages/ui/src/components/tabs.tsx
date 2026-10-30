import { WarningCircle } from "@phosphor-icons/react";
import { AnimatePresence, motion } from "motion/react";
import { Tabs as RadixTabs } from "radix-ui";
import { type ReactNode, useEffect, useLayoutEffect, useRef, type WheelEvent } from "react";
import { type Glide, glideUnderline } from "../glide";
import { focusTabQuietly } from "../input-modality";
import { cn } from "../lib/cn";
import { enter, leave } from "../motion";

export interface TabItem<T extends string> {
  value: T;
  label: string;
  count?: number;
  /**
   * Something waits in the tab (unsaved changes): a small gold dot by the label, read out as
   * this text. It sits in the tab's padding, so it never moves the other tabs.
   */
  marker?: string;
  /**
   * What the tab counts could not be read (its own notice is inside): a small red mark in the
   * count's place, read out as this text, so the row neither drops a number silently nor shifts.
   */
  alert?: string;
}

/**
 * Underline tabs with a gold indicator that slides between items. Labels never wrap: when the
 * row is wider than its room it scrolls sideways (the mouse wheel scrolls it too) and the
 * chosen tab is kept in view.
 *
 * A pick glides the underline along the row (glide.ts): measured inside the list, so a dialog or
 * sheet that resizes, re-centres or scrolls with the new tab never carries it along. It glides in
 * Performance mode too (FIX1); under reduced motion it is simply under the new tab.
 */
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
  const listRef = useRef<HTMLDivElement>(null);
  // The tab whose underline shows, and the glide under way.
  const shown = useRef<string>(value);
  const glide = useRef<Glide | null>(null);
  useLayoutEffect(() => {
    const from = shown.current;
    if (from === value) return;
    shown.current = value;
    // Before the first paint of the new tab: its underline starts where the old one is.
    const list = listRef.current;
    glide.current = list ? glideUnderline(list, from, value, glide.current) : null;
  }, [value]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the chosen tab changes
  useEffect(() => {
    const list = listRef.current;
    if (!list || list.scrollWidth <= list.clientWidth) return;
    const active = list.querySelector<HTMLElement>("[data-state=active]");
    active?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [value]);

  const onWheel = (event: WheelEvent<HTMLDivElement>) => {
    const list = event.currentTarget;
    if (list.scrollWidth <= list.clientWidth || Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
      return;
    }
    list.scrollLeft += event.deltaY;
  };

  return (
    <RadixTabs.Root
      value={value}
      onValueChange={(v) => onValueChange(v as T)}
      className={cn("flex min-h-0 flex-1 flex-col", className)}
    >
      <RadixTabs.List
        ref={listRef}
        aria-label={label}
        onWheel={onWheel}
        // The divider is an inset shadow so the indicator can sit on it inside the scroll box.
        className="flex shrink-0 gap-0.5 overflow-x-auto overflow-y-hidden shadow-[inset_0_-1px_0_rgb(255_255_255/0.08)] [scrollbar-width:none]"
      >
        {items.map((item) => (
          <RadixTabs.Trigger
            key={item.value}
            value={item.value}
            data-tab={item.value}
            onMouseDown={focusTabQuietly}
            className="group relative flex h-[34px] shrink-0 cursor-pointer items-center gap-1.5 px-2.5 text-[13px] font-medium whitespace-nowrap text-ash-300 transition-colors duration-150 hover:text-bone-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-inset data-[state=active]:text-bone-50"
          >
            {item.label}
            {item.alert ? (
              <span className="flex">
                <WarningCircle size={14} weight="fill" className="text-flare-400" aria-hidden />
                <span className="sr-only">{item.alert}</span>
              </span>
            ) : typeof item.count === "number" ? (
              // Chamfered like every small chip (design.md: the rounded corner is for containers),
              // in the bone of Studio's sidebar counts, so a count reads the same everywhere.
              <span className="chamfer flex h-[18px] min-w-[18px] items-center justify-center bg-white/10 px-1 text-[11px] font-semibold text-bone-100 tabular-nums [--cut:4px]">
                {item.count}
              </span>
            ) : null}
            <AnimatePresence initial={false}>
              {item.marker ? (
                <motion.span
                  key="marker"
                  // Beside a count it sits a little higher and further out, clear of the
                  // count's corner (MODHUB2: "Modlar 12" with unapplied choices).
                  className={cn(
                    "pointer-events-none absolute flex",
                    typeof item.count === "number" ? "top-[5px] right-px" : "top-[7px] right-[3px]",
                  )}
                  initial={{ opacity: 0, transform: "scale(0.6)" }}
                  animate={{ opacity: 1, transform: "scale(1)", transition: enter(0.2) }}
                  exit={{ opacity: 0, transform: "scale(0.6)", transition: leave(0.2) }}
                >
                  <span aria-hidden className="size-1.5 rounded-full bg-gold-400" />
                  <span className="sr-only">{item.marker}</span>
                </motion.span>
              ) : null}
            </AnimatePresence>
            {/* Every tab has its underline; only the chosen one shows (glide.ts measures the
                hidden one of the tab it leaves). */}
            <span
              aria-hidden
              data-underline=""
              data-on={value === item.value ? "" : undefined}
              className="pointer-events-none invisible absolute inset-x-2 bottom-0 h-[2px] origin-left bg-gold-400 data-[on]:visible"
            />
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
      className={cn("panel-in min-h-0 flex-1 overflow-y-auto pt-4 outline-none", className)}
    >
      {children}
    </RadixTabs.Content>
  );
}
