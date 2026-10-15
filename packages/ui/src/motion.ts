import type { Transition } from "motion/react";

/** Entrance easing: fast start, long soft landing. */
export const easeOutExpo = [0.16, 1, 0.3, 1] as const;
export const easeInOutQuart = [0.76, 0, 0.24, 1] as const;

export const springSnappy: Transition = { type: "spring", stiffness: 420, damping: 32 };
export const springGentle: Transition = { type: "spring", stiffness: 160, damping: 24 };

export const durations = {
  hover: 0.12,
  small: 0.2,
  panel: 0.32,
  boot: 0.7,
} as const;

/** Exits run ~30% faster than entrances so dismissals feel responsive. */
export function exitOf(enter: number) {
  return enter * 0.7;
}

export const fadeUp = {
  initial: { opacity: 0, y: 10 },
  animate: { opacity: 1, y: 0, transition: { duration: durations.panel, ease: easeOutExpo } },
  exit: { opacity: 0, y: 6, transition: { duration: exitOf(durations.panel), ease: easeOutExpo } },
};
