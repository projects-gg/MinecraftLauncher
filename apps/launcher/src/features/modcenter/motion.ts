// Motion of the mod center (docs/design.md §5): ease-out entrances, exits ~30% faster, only
// transform and opacity. Presence animations use `transform` strings (hardware accelerated).
// "Reduce motion" keeps fades only; layout glides (rows, banners, dialog panels) are also off
// in Performance mode, where things simply appear in place. The shared pieces live in @pl/ui.
import { easeOutExpo, enter, leave } from "@pl/ui";
import type { Transition } from "motion/react";

export { enter, leave, type MotionPrefs, useMotionPrefs } from "@pl/ui";

/** Glide of rows and panels when content above them changes size. */
export const glide: Transition = { duration: 0.3, ease: easeOutExpo };

const shift = (y: number, reduce: boolean) => (reduce ? "none" : `translateY(${y}px)`);

/** Appears in place, rising a few pixels. */
export function rise(reduce: boolean, distance = 6, duration = 0.24) {
  return {
    initial: { opacity: 0, transform: shift(distance, reduce) },
    animate: { opacity: 1, transform: shift(0, reduce), transition: enter(duration) },
    exit: { opacity: 0, transform: shift(-distance / 2, reduce), transition: leave(duration) },
  };
}

/**
 * Text replaced in place (with AnimatePresence `popLayout`): the old copy leaves fast and the
 * new one follows a beat later, so the two never sit on top of each other half-faded.
 */
export function swap(duration = 0.18) {
  return {
    initial: { opacity: 0 },
    animate: { opacity: 1, transition: { duration, ease: easeOutExpo, delay: 0.05 } },
    exit: { opacity: 0, transition: { duration: 0.08, ease: easeOutExpo } },
  };
}

/** Fades only (blocks that come and go). */
export function fade(duration = 0.18) {
  return {
    initial: { opacity: 0 },
    animate: { opacity: 1, transition: enter(duration) },
    exit: { opacity: 0, transition: leave(duration) },
  };
}

/** Small labels (badges): grow in from 90%, never from nothing. */
export function pop(reduce: boolean, duration = 0.2) {
  const from = reduce ? "none" : "scale(0.9)";
  return {
    initial: { opacity: 0, transform: from },
    animate: { opacity: 1, transform: reduce ? "none" : "scale(1)", transition: enter(duration) },
    exit: { opacity: 0, transform: from, transition: leave(duration) },
  };
}

export type Direction = 1 | -1;

/**
 * Steps of a dialog (`custom` = direction, also on the AnimatePresence so a leaving step
 * knows it): forward slides in from the right, back from the left.
 */
export function stepVariants(reduce: boolean) {
  const x = (px: number) => (reduce ? "none" : `translateX(${px}px)`);
  return {
    enter: (direction: Direction) => ({ opacity: 0, transform: x(14 * direction) }),
    center: { opacity: 1, transform: x(0), transition: enter(0.24) },
    exit: (direction: Direction) => ({
      opacity: 0,
      transform: x(-10 * direction),
      transition: { duration: 0.12, ease: easeOutExpo },
    }),
  };
}

/** Delay of the `index`th item of a list that enters together (short, capped). */
export function stagger(index: number, each = 0.04, max = 8) {
  return Math.min(index, max) * each;
}
