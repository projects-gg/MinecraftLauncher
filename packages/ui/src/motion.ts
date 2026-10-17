// Shared motion vocabulary (rules: docs/design.md §5). Entrances ease out; exits are ~30% faster;
// only transform and opacity move, written as `transform` strings where an element enters or
// leaves (hardware accelerated, smooth while the page is busy). "Reduce motion" keeps fades only;
// Performance mode keeps fades and drops layout glides.
import { type Transition, useReducedMotion } from "motion/react";
import { usePerf } from "./perf";

/** Entrance easing: fast start, long soft landing. */
export const easeOutExpo = [0.16, 1, 0.3, 1] as const;
export const easeInOutQuart = [0.76, 0, 0.24, 1] as const;
/** iOS-like drawer curve (sheets). */
export const easeDrawer = [0.32, 0.72, 0, 1] as const;

export const springSnappy: Transition = { type: "spring", stiffness: 420, damping: 32 };
export const springGentle: Transition = { type: "spring", stiffness: 160, damping: 24 };

export const durations = {
  hover: 0.12,
  small: 0.2,
  panel: 0.32,
  sheet: 0.36,
  boot: 0.7,
} as const;

/** Exits run ~30% faster than entrances so dismissals feel responsive. */
export function exitOf(enter: number) {
  return enter * 0.7;
}

export const enter = (duration: number): Transition => ({ duration, ease: easeOutExpo });
export const leave = (duration: number): Transition => ({
  duration: exitOf(duration),
  ease: easeOutExpo,
});

export interface MotionPrefs {
  /** The OS asks for reduced motion: fades only, no movement. */
  reduce: boolean;
  /** Layout glides may run (not reduced, not Performance mode). */
  layout: boolean;
}

export function useMotionPrefs(): MotionPrefs {
  const reduce = useReducedMotion() ?? false;
  const { performanceMode } = usePerf();
  return { reduce, layout: !reduce && !performanceMode };
}

export const fadeUp = {
  initial: { opacity: 0, y: 10 },
  animate: { opacity: 1, y: 0, transition: { duration: durations.panel, ease: easeOutExpo } },
  exit: { opacity: 0, y: 6, transition: { duration: exitOf(durations.panel), ease: easeOutExpo } },
};

/**
 * A right-hand sheet (account center, friends, AFK settings and console, appearance): slides
 * in on the drawer curve and leaves faster; a short fade under reduced motion.
 */
export function sheetMotion(reduce: boolean) {
  if (reduce) {
    return {
      initial: { opacity: 0 },
      animate: { opacity: 1, transition: { duration: 0.16, ease: easeOutExpo } },
      exit: { opacity: 0, transition: { duration: 0.12, ease: easeOutExpo } },
    };
  }
  return {
    initial: { transform: "translateX(100%)" },
    animate: {
      transform: "translateX(0%)",
      transition: { duration: durations.sheet, ease: easeDrawer },
    },
    exit: {
      transform: "translateX(100%)",
      transition: { duration: exitOf(durations.sheet), ease: easeDrawer },
    },
  };
}

/** The dimmed layer under a sheet or dialog. */
export function scrimMotion(reduce: boolean) {
  return {
    initial: { opacity: 0 },
    animate: { opacity: 1, transition: { duration: reduce ? 0.16 : 0.24, ease: easeOutExpo } },
    exit: { opacity: 0, transition: { duration: reduce ? 0.12 : 0.18, ease: easeOutExpo } },
  };
}

/** A centred dialog panel: rises and settles from 98.5%; a fade under reduced motion. */
export function dialogMotion(reduce: boolean) {
  if (reduce) {
    return {
      initial: { opacity: 0 },
      animate: { opacity: 1, transition: { duration: 0.16, ease: easeOutExpo } },
      exit: { opacity: 0, transition: { duration: 0.12, ease: easeOutExpo } },
    };
  }
  return {
    initial: { opacity: 0, transform: "translateY(12px) scale(0.985)" },
    animate: {
      opacity: 1,
      transform: "translateY(0px) scale(1)",
      transition: { duration: 0.3, ease: easeOutExpo },
    },
    exit: {
      opacity: 0,
      transform: "translateY(6px) scale(0.99)",
      transition: { duration: exitOf(0.3), ease: easeOutExpo },
    },
  };
}
