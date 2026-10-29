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

/** Scrolls `node`'s scroll area just enough that its bottom clears the bar's room. */
export function revealAboveBar(node: HTMLElement | null, reduce: boolean) {
  const scroller = scrollParent(node);
  if (!node || !scroller) return;
  const covered =
    node.getBoundingClientRect().bottom - (scroller.getBoundingClientRect().bottom - BAR_ROOM);
  if (covered > 0) scroller.scrollBy({ top: covered, behavior: reduce ? "auto" : "smooth" });
}

/**
 * The end of `node`'s scroll area is about to lose `lost` pixels. Scrolled down that far, the
 * browser would pull the content down in one jump when it goes; instead the area glides up by
 * as much, along the entrance curve in `ms` (at once for 0), so nothing moves when it shrinks.
 */
export function glideBeforeShrink(node: HTMLElement | null, lost: number, ms: number) {
  const scroller = scrollParent(node);
  if (!scroller || lost <= 0) return;
  const target = Math.max(0, scroller.scrollHeight - scroller.clientHeight - lost);
  const from = scroller.scrollTop;
  if (from <= target) return;
  if (ms <= 0) {
    scroller.scrollTop = target;
    return;
  }
  const start = performance.now();
  const step = (now: number) => {
    const t = Math.min(1, (now - start) / ms);
    scroller.scrollTop = t >= 1 ? target : from + (target - from) * (1 - 2 ** (-10 * t));
    if (t < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
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
              {/* Rows the bar covers fade out just above it, so a card cut by the bar never shows
                  as a sliver over its edge (TASTE-SWEEP: the cloud list at 1024, where the reveal
                  stops at the section's header). */}
              <span
                aria-hidden
                className="pointer-events-none absolute inset-x-0 -top-8 h-8 bg-[linear-gradient(to_bottom,transparent,rgb(10_9_12/0.85))]"
              />
              {/* The toasts' near-solid surface, not glass: inside the tab's scroller the blur
                  never reaches the rows under the bar, and their text showed through its tint
                  over "1 mod to remove." (POLISH6; the toasts had the same problem). */}
              <div className="toast-surface relative flex min-h-[58px] items-center gap-4 overflow-hidden rounded-[var(--radius-panel)] px-4 py-2.5">
                {children}
              </div>
            </motion.section>
          ) : null}
        </AnimatePresence>
      </div>
    </>
  );
}
