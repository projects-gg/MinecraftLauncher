// Reduced motion for the account surfaces: the same keyframes with every movement removed, so
// things fade instead of sliding, scaling or blurring. (`MotionConfig reducedMotion="user"`
// alone skips transform animations but can leave an entrance's starting scale on screen until
// its fade ends, which reads as a snap.)
import { useReducedMotion } from "motion/react";

const MOVEMENT = new Set(["x", "y", "scale", "scaleX", "scaleY", "rotate", "filter"]);

/** Removes movement from a motion target (keeps opacity, colours and the transition). */
export function withoutMovement<T>(target: T): T {
  if (!target || typeof target !== "object" || Array.isArray(target)) return target;
  return Object.fromEntries(
    Object.entries(target as Record<string, unknown>).filter(([key]) => !MOVEMENT.has(key)),
  ) as T;
}

/** `calm(target)`: the target as is, or without movement when the player asked for less motion. */
export function useCalm() {
  const reduced = useReducedMotion();
  return <T>(target: T): T => (reduced ? withoutMovement(target) : target);
}
