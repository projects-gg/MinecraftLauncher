// The settings panel of an AFK account, desktop or 7/24 Bulut (SHEET-UX): a right sheet as wide as
// the account's AFK+ panel; on the left a rail of a few sections (which one is open, which hold
// unsaved edits or a problem), on the right one section at a time, and under it the change bar,
// docked, while something waits to be saved. A press outside, Esc or the close button always
// closes it: unsaved edits wait in `drafts.ts` and come back with the panel.
import { type Icon, WarningCircle, X } from "@phosphor-icons/react";
import {
  cn,
  EntranceGate,
  focusPanel,
  focusTabQuietly,
  GatedMotion,
  IconButton,
  Skeleton,
  scrimMotion,
  sheetMotion,
  sheetOutside,
  useInteractionMotion,
  useMotionPrefs,
} from "@pl/ui";
import { AnimatePresence, LayoutGroup, MotionConfig, motion } from "motion/react";
import { Dialog, Tabs } from "radix-ui";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { AfkHead } from "../afk-head";
import { DOCK_HEIGHT } from "../cloud/plus/change-dock";

export interface PanelSection<Id extends string> {
  id: Id;
  label: string;
  icon: Icon;
  /** Holds edits not saved yet: a gold dot. */
  unsaved: boolean;
  /** Holds a field to fix: the flare mark. */
  problem: boolean;
}

/** The sheet itself: scrim, slide, header; its body is the editor, its skeleton or its failure. */
export function SettingsPanel({
  open,
  onClose,
  nickname,
  title,
  server,
  children,
}: {
  open: boolean;
  onClose: () => void;
  nickname: string;
  title: string;
  server: string;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const animate = useInteractionMotion();
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <AnimatePresence>
        {open ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-30 bg-obsidian-950/55"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                // A press outside closes it as Esc does; unsaved edits wait as a draft. A toast's
                // button is not outside.
                {...sheetOutside}
                // The panel takes the focus, not the close button (its tooltip would pop open).
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  focusPanel(event.currentTarget as HTMLElement);
                }}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex w-[min(860px,94vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
                  <header className="flex items-center gap-3.5 border-b border-white/8 px-5 py-4">
                    <AfkHead nickname={nickname} size={36} />
                    <div className="min-w-0 flex-1">
                      <Dialog.Title className="truncate font-display text-[18px] font-semibold text-bone-50">
                        {title}
                      </Dialog.Title>
                      <p className="truncate text-[12.5px] text-ash-400" data-selectable>
                        {server}
                      </p>
                    </div>
                    <IconButton
                      label={t("common.close")}
                      icon={<X size={18} />}
                      size="sm"
                      onClick={onClose}
                    />
                  </header>
                  <MotionConfig reducedMotion="user">{children}</MotionConfig>
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}

/** The rail beside one section: a gold dot where edits wait, the flare mark where a field is wrong. */
export function PanelBody<Id extends string>({
  sections,
  section,
  onSection,
  label,
  scrollerRef,
  dockRef,
  bar,
  children,
}: {
  sections: Array<PanelSection<Id>>;
  section: Id;
  onSection: (id: Id) => void;
  /** The rail's accessible name. */
  label: string;
  scrollerRef?: (element: HTMLDivElement | null) => void;
  /** The zero-height slot at the scroller's bottom edge the change bar docks into. */
  dockRef: (element: HTMLDivElement | null) => void;
  /** The change bar: outside the section, so a section change never makes it rise again. */
  bar: ReactNode;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  return (
    <Tabs.Root
      value={section}
      onValueChange={(value) => onSection(value as Id)}
      orientation="vertical"
      className="flex min-h-0 flex-1"
    >
      <Tabs.List
        aria-label={label}
        className="flex w-[192px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-white/7 p-3"
      >
        {sections.map((item) => {
          const ItemIcon = item.icon;
          const on = item.id === section;
          return (
            <Tabs.Trigger
              key={item.id}
              value={item.id}
              onMouseDown={focusTabQuietly}
              className={cn(
                "relative isolate flex h-9 w-full cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-left text-[13px] font-medium transition-[color,background-color,transform,scale] duration-150 ease-out outline-none focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.98]",
                on ? "text-bone-50" : "text-ash-300 hover:bg-white/5 hover:text-bone-50",
              )}
            >
              {/* The highlight glides to the chosen section (in place in Performance mode). */}
              {on ? (
                <motion.span
                  aria-hidden
                  layoutId={layout ? "afk-settings-section" : undefined}
                  className="absolute inset-0 -z-10 rounded-md bg-white/8"
                  transition={{ type: "spring", stiffness: 480, damping: 40 }}
                />
              ) : null}
              <ItemIcon
                size={17}
                weight={on ? "duotone" : "regular"}
                className={cn("shrink-0", on && "text-gold-300")}
              />
              <span className="min-w-0 flex-1 truncate">{item.label}</span>
              {item.problem ? (
                <WarningCircle
                  size={16}
                  weight="fill"
                  className="shrink-0 text-flare-400"
                  aria-label={t("afkSettings.sectionHasErrors")}
                />
              ) : item.unsaved ? (
                <span className="flex shrink-0 items-center">
                  <span aria-hidden className="size-1.5 rounded-full bg-gold-400" />
                  <span className="sr-only">{t("afkPanel.unsavedSection")}</span>
                </span>
              ) : null}
            </Tabs.Trigger>
          );
        })}
      </Tabs.List>
      <motion.div
        ref={scrollerRef}
        layoutScroll
        className="relative min-h-0 flex-1 overflow-y-auto overscroll-contain [scrollbar-gutter:stable]"
      >
        <Tabs.Content
          key={section}
          value={section}
          className="flex min-h-full flex-col outline-none"
        >
          {/* A new section rises into place (`panel-in`); its end keeps the change bar's room, so
              the bar's coming and going never changes how far the column scrolls. */}
          <LayoutGroup>
            <div className="panel-in flex flex-1 flex-col px-6 pt-5">
              {children}
              <div aria-hidden className="shrink-0" style={{ height: DOCK_HEIGHT + 16 }} />
            </div>
          </LayoutGroup>
        </Tabs.Content>
        <div ref={dockRef} className="sticky bottom-0 z-10 h-0" />
        {bar}
      </motion.div>
    </Tabs.Root>
  );
}

/** While the account's settings load: the rail and a section's first rows. */
export function PanelSkeleton({ rows }: { rows: number }) {
  return (
    <div className="flex min-h-0 flex-1" aria-hidden>
      <div className="w-[192px] shrink-0 space-y-1.5 border-r border-white/7 p-3">
        {Array.from({ length: rows }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: placeholders have no identity
          <Skeleton key={i} className="h-9" />
        ))}
      </div>
      <div className="flex-1 space-y-4 px-6 pt-6">
        <Skeleton className="h-6 w-40" />
        <Skeleton className="h-[34px] w-full" />
        <Skeleton className="h-4 w-[70%]" />
        <Skeleton className="h-[34px] w-full" />
        <Skeleton className="h-4 w-[60%]" />
      </div>
    </div>
  );
}
