import { enter, leave, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef } from "react";

/** The bar's height plus its gap to the bottom (58 + 12 px), and some air above it. */
const BAR_ROOM = 84;

/** The nearest ancestor that scrolls vertically (the tab panel). */
function scrollParent(node: HTMLElement | null): HTMLElement | null {
  for (let element = node?.parentElement ?? null; element; element = element.parentElement) {
    const { overflowY } = getComputedStyle(element);
    if (overflowY === "auto" || overflowY === "scroll") return element;
  }
  return null;
}

/**
 * The bar that floats at the bottom of a scroll area while there is something to save or apply
 * (instance settings, the mod center's choices). It rises in and leaves faster; what it shows
 * swaps inside it. A spacer always reserves its room and the bar hangs from a zero-height
 * anchor stuck to the bottom, so its coming and going never changes the scroll height and the
 * content never jumps. Put it last in a `flex min-h-full flex-col` column of the scroller.
 *
 * When it appears over the control the player just changed (a switch on the last visible row),
 * the list scrolls that control clear of it.
 */
export function ChangeBar({
  open,
  label,
  children,
}: {
  open: boolean;
  /** Accessible name of the bar's region. */
  label: string;
  children: ReactNode;
}) {
  const prefs = useMotionPrefs();
  const anchor = useRef<HTMLDivElement>(null);
  const shift = (y: number) => (prefs.reduce ? "none" : `translateY(${y}px)`);

  useEffect(() => {
    if (!open) return;
    const active = document.activeElement;
    const scroller = scrollParent(anchor.current);
    if (!(active instanceof HTMLElement) || !scroller?.contains(active)) return;
    // The whole row of the control (its name and notes too), not only the switch.
    const row = active.closest("li") ?? active;
    const covered =
      row.getBoundingClientRect().bottom - (scroller.getBoundingClientRect().bottom - BAR_ROOM);
    if (covered > 0) {
      scroller.scrollBy({ top: covered, behavior: prefs.reduce ? "auto" : "smooth" });
    }
  }, [open, prefs.reduce]);

  return (
    <>
      <div className="h-24 shrink-0" aria-hidden />
      <div ref={anchor} className="sticky bottom-0 z-10 mt-auto h-0">
        <AnimatePresence>
          {open ? (
            <motion.section
              key="bar"
              aria-label={label}
              initial={{ opacity: 0, transform: shift(18) }}
              animate={{ opacity: 1, transform: shift(0), transition: enter(0.3) }}
              exit={{ opacity: 0, transform: shift(12), transition: leave(0.3) }}
              className="absolute inset-x-0 bottom-3"
            >
              <div className="glass-strong relative flex min-h-[58px] items-center gap-4 overflow-hidden rounded-[var(--radius-panel)] px-4 py-2.5 shadow-[0_18px_50px_rgb(34_6_6/0.55)]">
                {children}
              </div>
            </motion.section>
          ) : null}
        </AnimatePresence>
      </div>
    </>
  );
}
