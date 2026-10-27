// The composer's emoji button and its picker (CHAT2, docs: chat2.md §3.1): a 32 px icon button
// (Phosphor Smiley, "Emoji ve çıkartmalar") that opens the 336 × 384 picker above it, glass
// obsidian, rising 4 px as it fades in (140 ms; a fade only under reduced motion). The picker, its
// data and its font load when the pointer or the focus first reaches the button, so it opens at
// once; while closed nothing of it is mounted. Esc and a press outside close it; the focus then
// goes where the owner says (the composer's field), or back to the button after keys.
import { Smiley } from "@phosphor-icons/react";
import type { Sticker as StickerData } from "@pl/stickers";
import {
  EntranceGate,
  easeOutExpo,
  exitOf,
  focusPanel,
  GatedMotion,
  IconButton,
  returnFocusAfterKeys,
  Skeleton,
  swallowDismissClick,
  useMotionPrefs,
} from "@pl/ui";
import { AnimatePresence } from "motion/react";
import { Popover } from "radix-ui";
import { lazy, type ReactNode, Suspense, useCallback, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { PANEL_HEIGHT, PANEL_WIDTH, type PickerTab, SKELETON_KEYS } from "./picker-layout";

const loadPanel = () => import("./picker-panel");
const PickerPanel = lazy(loadPanel);

let preloaded = false;

/**
 * Loads the picker before it is needed (the pointer is on its way to the button): the panel's
 * code, the data in the interface's language and the font's common slice.
 */
export function preloadEmojiPicker(lang: string): void {
  if (preloaded) return;
  preloaded = true;
  void loadPanel()
    .then(() => import("./data"))
    .then(({ loadEmojiData }) => loadEmojiData(lang.startsWith("en") ? "en" : "tr"))
    .then(() => {
      // The slice of the faces and hands most chats draw; the others load as they show.
      if (typeof document !== "undefined") void document.fonts?.load('22px "PL Emoji"', "😀");
    })
    .catch(() => {
      preloaded = false;
    });
}

export function EmojiPickerButton({
  onEmoji,
  onSticker,
  disabled = false,
  initialTab,
  onOpenChange,
  returnFocus,
  closeOnEmoji = false,
  label: ownLabel,
  icon,
  side = "top",
  align = "end",
}: {
  /** An emoji was picked (in the chosen skin tone); the picker stays open. */
  onEmoji: (emoji: string) => void;
  /** A sticker was picked; the picker closes. Without it the picker has no sticker tab. */
  onSticker?: (sticker: StickerData) => void;
  disabled?: boolean;
  initialTab?: PickerTab;
  onOpenChange?: (open: boolean) => void;
  /** Where the focus goes when the picker closes (the composer's field). */
  returnFocus?: () => HTMLElement | null;
  /** A reaction's picker: one pick and it closes. */
  closeOnEmoji?: boolean;
  /** The button's name and mark in place of the composer's ("Başka bir emoji", +). */
  label?: string;
  icon?: ReactNode;
  side?: "top" | "bottom";
  align?: "start" | "center" | "end";
}) {
  const { t, i18n } = useTranslation();
  const { reduce } = useMotionPrefs();
  const [open, setOpenState] = useState(false);
  const panel = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement | null>(null);

  const setOpen = useCallback(
    (next: boolean) => {
      setOpenState(next);
      onOpenChange?.(next);
    },
    [onOpenChange],
  );
  const intent = () => preloadEmojiPicker(i18n.language);
  const label = ownLabel ?? (onSticker ? t("chat2.picker.open") : t("chat2.picker.openEmoji"));

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <IconButton
          label={label}
          icon={icon ?? <Smiley size={19} weight={open ? "fill" : "regular"} aria-hidden />}
          tooltipSide="top"
          active={open}
          disabled={disabled}
          onPointerEnter={intent}
          onFocus={intent}
        />
      </Popover.Trigger>
      <AnimatePresence>
        {open ? (
          <Popover.Portal forceMount>
            <EntranceGate key="emoji-picker">
              <Popover.Content
                onPointerDownOutside={swallowDismissClick}
                forceMount
                asChild
                side={side}
                align={align}
                sideOffset={10}
                collisionPadding={12}
                aria-label={label}
                onOpenAutoFocus={(event) => {
                  // The search field when it is there, else the panel (it moves there on mount).
                  event.preventDefault();
                  const target = search.current ?? panel.current;
                  if (target) focusPanel(target);
                }}
                onCloseAutoFocus={(event) => {
                  const target = returnFocus?.();
                  if (target) {
                    event.preventDefault();
                    target.focus({ preventScroll: true });
                    return;
                  }
                  returnFocusAfterKeys(event);
                }}
              >
                <GatedMotion.div
                  ref={panel}
                  tabIndex={-1}
                  className="z-50 overflow-hidden rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] outline-none backdrop-blur-xl [html[data-perf=on]_&]:bg-obsidian-900 [html[data-perf=on]_&]:backdrop-blur-none"
                  style={{
                    width: PANEL_WIDTH,
                    height: PANEL_HEIGHT,
                    transformOrigin: "var(--radix-popover-content-transform-origin)",
                  }}
                  initial={{
                    opacity: 0,
                    transform: reduce ? "none" : "translateY(4px) scale(0.985)",
                  }}
                  animate={{
                    opacity: 1,
                    transform: "translateY(0px) scale(1)",
                    transition: { duration: 0.14, ease: easeOutExpo },
                  }}
                  exit={{
                    opacity: 0,
                    transform: reduce ? "none" : "translateY(2px) scale(0.99)",
                    transition: { duration: exitOf(0.14), ease: easeOutExpo },
                  }}
                >
                  <Suspense fallback={<PanelSkeleton />}>
                    <PickerPanel
                      stickers={Boolean(onSticker)}
                      initialTab={initialTab}
                      onEmoji={(emoji) => {
                        onEmoji(emoji);
                        if (closeOnEmoji) setOpen(false);
                      }}
                      onSticker={(sticker) => {
                        onSticker?.(sticker);
                        setOpen(false);
                      }}
                      searchRef={(node) => {
                        search.current = node;
                        // Mounted after the layer opened (the code was still loading): the
                        // focus moves on from the panel.
                        if (node && document.activeElement === panel.current) focusPanel(node);
                      }}
                    />
                  </Suspense>
                </GatedMotion.div>
              </Popover.Content>
            </EntranceGate>
          </Popover.Portal>
        ) : null}
      </AnimatePresence>
    </Popover.Root>
  );
}

/** The panel's shape while its code loads (only before the first open finished loading it). */
function PanelSkeleton() {
  return (
    <div aria-hidden className="flex h-full flex-col gap-3 p-2.5">
      <Skeleton className="h-[34px] w-40 rounded-md" />
      <Skeleton className="h-[34px] w-full rounded-md" />
      <div className="grid flex-1 grid-cols-8 content-start gap-y-3 px-1.5 pt-2">
        {SKELETON_KEYS.slice(0, 40).map((key) => (
          <Skeleton key={key} className="size-[22px] rounded-full" />
        ))}
      </div>
    </div>
  );
}
