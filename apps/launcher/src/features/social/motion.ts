// Motion of the friends surfaces (docs/design.md §5): ease-out entrances, exits ~30% faster, only
// transform and opacity, written as `transform` strings (hardware accelerated). Reduced motion
// keeps fades only; Performance mode also drops layout glides. The shared pieces live in @pl/ui.
import { easeOutExpo, enter, leave, type MotionPrefs, useMotionPrefs } from "@pl/ui";
import type { Transition } from "motion/react";

export { enter, leave };

/** Fades only (`reduce`) and whether rows may glide (`layout`). */
export type SocialMotion = MotionPrefs;
export const useSocialMotion = useMotionPrefs;

/** Rows gliding when the list around them changes. */
export const glide: Transition = { type: "spring", stiffness: 420, damping: 36, mass: 0.9 };

const y = (px: number, reduce: boolean) => (reduce ? "none" : `translateY(${px}px)`);

/** Appears in place, rising a few pixels. */
export function rise(reduce: boolean, distance = 6, duration = 0.26, delay = 0) {
  return {
    initial: { opacity: 0, transform: y(distance, reduce) },
    animate: {
      opacity: 1,
      transform: y(0, reduce),
      transition: { ...enter(duration), delay },
    },
    exit: { opacity: 0, transform: y(-distance / 2, reduce), transition: leave(duration) },
  };
}

/** Text replaced in place (AnimatePresence `popLayout`): out fast, in a beat later. */
export function swap(reduce: boolean, duration = 0.2) {
  return {
    initial: { opacity: 0, transform: y(5, reduce) },
    animate: {
      opacity: 1,
      transform: y(0, reduce),
      transition: { ...enter(duration), delay: 0.04 },
    },
    exit: {
      opacity: 0,
      transform: y(-5, reduce),
      transition: { duration: 0.1, ease: easeOutExpo },
    },
  };
}

/** A cross-fade with no movement (headers that change while the page below slides). */
export function crossfade(duration = 0.22) {
  return {
    initial: { opacity: 0 },
    animate: { opacity: 1, transition: { ...enter(duration), delay: 0.05 } },
    exit: { opacity: 0, transition: { duration: 0.1, ease: easeOutExpo } },
  };
}

/** Small marks (badges, faces): grow in from 85%, never from nothing. */
export function pop(reduce: boolean, duration = 0.22) {
  const from = reduce ? "none" : "scale(0.85)";
  return {
    initial: { opacity: 0, transform: from },
    animate: { opacity: 1, transform: reduce ? "none" : "scale(1)", transition: enter(duration) },
    exit: { opacity: 0, transform: from, transition: leave(duration) },
  };
}

export type Direction = 1 | -1;

/**
 * Pages of the panel (`custom` = direction, also on AnimatePresence): deeper pages come in from
 * the right and the list returns from the left.
 */
export function pageVariants(reduce: boolean) {
  const x = (px: number) => (reduce ? "none" : `translateX(${px}px)`);
  return {
    enter: (direction: Direction) => ({ opacity: 0, transform: x(26 * direction) }),
    center: { opacity: 1, transform: x(0), transition: enter(0.3) },
    exit: (direction: Direction) => ({
      opacity: 0,
      transform: x(-18 * direction),
      transition: { duration: 0.16, ease: easeOutExpo },
    }),
  };
}

/** Delay of the `index`th item entering together (short and capped). */
export function stagger(index: number, each = 0.03, max = 10) {
  return Math.min(index, max) * each;
}
