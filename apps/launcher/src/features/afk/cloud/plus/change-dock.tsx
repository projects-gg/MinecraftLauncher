// "Ayarlar"'s change bar, docked: flush with the bottom of the panel's content column and as wide as
// it (never over its scroll bar), solid obsidian with a hairline on top, so no row shows through it
// or peeks out below it. The library's floating `ChangeBar` leaves a 12 px gap the list scrolls
// through: in this narrow column it cut a help line in two and showed the next group's title under
// it. The dock rises from the bottom edge and leaves faster; reduced motion fades it. The section
// always ends with `DOCK_HEIGHT` of room, so the dock's coming and going never changes the scroll
// height, and when it appears over the control the player just changed, the list scrolls that
// control clear of it.
import { enter, leave, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect } from "react";
import { createPortal } from "react-dom";
import { revealAboveBar } from "@/features/library/change-bar";

/** The dock's height, and the room the section keeps for it at its end. */
export const DOCK_HEIGHT = 60;

export function PlusChangeDock({
  dock,
  open,
  label,
  children,
}: {
  /** The panel's zero-height sticky slot at the bottom of the scroller (`plus-panel.tsx`). */
  dock: HTMLElement | null;
  open: boolean;
  /** Accessible name of the bar's region. */
  label: string;
  children: ReactNode;
}) {
  const { reduce } = useMotionPrefs();

  useEffect(() => {
    if (!open) return;
    const active = document.activeElement;
    const scroller = dock?.parentElement;
    if (!(active instanceof HTMLElement) || !scroller?.contains(active)) return;
    // The changed control's whole row (its name and the line under it), not only the control.
    revealAboveBar(active.closest("li") ?? active.parentElement ?? active, reduce);
  }, [open, reduce, dock]);

  if (!dock) return null;
  const away = reduce ? "none" : "translateY(100%)";
  return createPortal(
    <AnimatePresence>
      {open ? (
        <motion.section
          key="dock"
          aria-label={label}
          initial={{ opacity: 0, transform: away }}
          animate={{
            opacity: 1,
            transform: reduce ? "none" : "translateY(0%)",
            transition: enter(0.28),
          }}
          exit={{ opacity: 0, transform: away, transition: leave(0.2) }}
          className="absolute inset-x-0 bottom-0 flex items-center gap-4 border-t border-white/8 bg-obsidian-900 px-6 shadow-[0_-14px_28px_rgb(34_6_6/0.38)]"
          style={{ height: DOCK_HEIGHT }}
        >
          {children}
        </motion.section>
      ) : null}
    </AnimatePresence>,
    dock,
  );
}
