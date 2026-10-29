// Reduced motion (docs/design.md §5): one answer for the whole app, read everywhere motion is
// decided. It is the system's setting or the app's own (`setAppReducedMotion`: the launcher's
// "Animasyonları azalt" and its Performance mode, BETA-FB). motion's config and the hooks
// (`useReduceMotion`, perf.tsx, motion.ts), the CSS (the root's `data-motion="reduce"`, styles.css),
// the Web Animations guard below, the app's own scrolls and the plates' glides (glide.ts) all ask
// here. Before BETA-FB each of them read the system's media query alone: the launcher had no way to
// ask for less motion, and Performance mode left the switches' thumbs and the plates moving.
//
// `MotionConfig reducedMotion` makes x, y, scale, rotate and layout animations jump, but not a
// `transform` string (`animate={{ transform: "translateY(0px)" }}`), the form this app uses for
// entrances and fills because it runs on the compositor. POLISH3's audit found the hero copy, the
// news column, the OYNA fill and the tabs' "unsaved" dot still sliding or growing under the OS
// setting; dozens of components write such strings, and the next one would too. So under reduced
// motion every Web Animation holds its movement at its final value for the whole animation: the
// element lands at once, and a fade in the same animation, or in its own (motion makes one per
// value), plays as written. Nothing changes while motion is not reduced.
import { useSyncExternalStore } from "react";

const QUERY = "(prefers-reduced-motion: reduce)";
const MOVEMENT = ["transform", "translate", "scale", "rotate", "offsetDistance"] as const;

type Frames = Keyframe[] | PropertyIndexedKeyframes;

const isMovement = (key: string) => (MOVEMENT as readonly string[]).includes(key);

/** The app's own wish for less motion (`setAppReducedMotion`). */
let appReduced = false;
/** The system's media query, looked up once (null where there is none: tests, workers). */
let system: MediaQueryList | null | undefined;
const listeners = new Set<() => void>();

function systemQuery(): MediaQueryList | null {
  if (system !== undefined) return system;
  system =
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia(QUERY)
      : null;
  // A change in the system's settings while the app runs applies at once.
  system?.addEventListener?.("change", changed);
  return system;
}

/** The root's mark for the CSS: `data-motion="reduce"` while motion is reduced, else "full". */
function markRoot() {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.motion = motionReduced() ? "reduce" : "full";
}

function changed() {
  markRoot();
  for (const listener of listeners) listener();
}

/** The system asks for reduced motion (Windows' animation effects off, macOS' "Reduce motion"). */
export function systemReducedMotion(): boolean {
  return systemQuery()?.matches ?? false;
}

/** Motion is reduced now: the system asks for it, or the app does. */
export function motionReduced(): boolean {
  return appReduced || systemReducedMotion();
}

/**
 * The app's own wish for less motion, beside the system's (the launcher: "Animasyonları azalt" or
 * Performance mode). Everything that reads `motionReduced` follows at once, the CSS included.
 */
export function setAppReducedMotion(on: boolean): void {
  if (on === appReduced) return;
  appReduced = on;
  changed();
}

/** Calls `listener` whenever the answer may have changed (for `useSyncExternalStore`). */
export function subscribeReducedMotion(listener: () => void): () => void {
  systemQuery();
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

const neverOnServer = () => false;

/**
 * True while motion is reduced (`motionReduced`); the component renders again when that changes.
 * Use it instead of motion's `useReducedMotion`, which reads the system's setting alone and only
 * once.
 */
export function useReduceMotion(): boolean {
  return useSyncExternalStore(subscribeReducedMotion, motionReduced, neverOnServer);
}

/** True while the system itself asks for reduced motion (the app's own switch cannot undo it). */
export function useSystemReducedMotion(): boolean {
  return useSyncExternalStore(subscribeReducedMotion, systemReducedMotion, neverOnServer);
}

/**
 * The keyframes with every movement held at its last value (`[last, last]`: a single value would
 * still animate from the element's current one), or `null` when nothing in them moves.
 */
export function stillKeyframes(keyframes: Frames | null | undefined): Frames | null {
  if (!keyframes || typeof keyframes !== "object") return null;
  if (Array.isArray(keyframes)) {
    const last: Record<string, unknown> = {};
    for (const frame of keyframes) {
      for (const [key, value] of Object.entries(frame)) {
        if (isMovement(key) && value !== undefined && value !== null) last[key] = value;
      }
    }
    const keys = Object.keys(last);
    if (keys.length === 0) return null;
    const frames = keyframes.length > 1 ? keyframes : [keyframes[0] ?? {}, keyframes[0] ?? {}];
    return frames.map((frame) => ({ ...frame, ...last })) as Keyframe[];
  }
  let moved = false;
  const next: PropertyIndexedKeyframes = { ...keyframes };
  for (const key of MOVEMENT) {
    const value = keyframes[key];
    if (value === undefined || value === null) continue;
    const list = Array.isArray(value) ? value : [value];
    const end = list[list.length - 1];
    if (end === undefined || end === null) continue;
    next[key] = [end, end] as PropertyIndexedKeyframes[string];
    moved = true;
  }
  return moved ? next : null;
}

let installed = false;

/**
 * Installs the guard once, before anything renders (each app's main.tsx), and marks the root for
 * the CSS (`data-motion`), which follows every change from then on. The guard asks at every
 * animation, so a change while the app runs applies to the next one.
 */
export function guardReducedMotion(): void {
  if (installed || typeof window === "undefined") return;
  installed = true;
  markRoot();
  if (typeof Element === "undefined" || typeof Element.prototype.animate !== "function") return;
  const animate = Element.prototype.animate;
  Element.prototype.animate = function guardedAnimate(
    this: Element,
    keyframes: Frames | null,
    options?: number | KeyframeAnimationOptions,
  ): Animation {
    if (motionReduced()) {
      const still = stillKeyframes(keyframes);
      if (still) return animate.call(this, still, options);
    }
    return animate.call(this, keyframes, options);
  };
}
