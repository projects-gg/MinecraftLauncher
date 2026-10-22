// How a sticker moves (owner, 22 Oct 2026; chat2.md §8): the image is still, and moves once in
// code, when it arrives and when the pointer comes onto it or presses it. One pattern per sticker
// (the manifest's `motion`, the same words on the phone and in the overlay), transform and
// opacity only, on the compositor (Web Animations), never a loop. A pattern under way finishes;
// the next pointer that arrives meanwhile does not restart it. Distances are fractions of the
// sticker's own size, so 112 px in chat and 64 px in the picker move alike.
import type { StickerMotion } from "@pl/stickers";

interface Pattern {
  /** Keyframes (each one's `easing` runs to the next). */
  frames: Keyframe[];
  duration: number;
  /** The point it turns and squashes around. */
  origin: string;
}

const out = "cubic-bezier(0.22, 1, 0.36, 1)";
const inOut = "cubic-bezier(0.45, 0, 0.55, 1)";
const down = "cubic-bezier(0.55, 0, 1, 0.45)";

/** Keyframes from [offset, transform, easing?] (opacity stays 1 unless given). */
const frames = (list: Array<[number, string, string?]>): Keyframe[] =>
  list.map(([offset, transform, easing]) => ({ offset, transform, easing: easing ?? inOut }));

export const STICKER_PATTERNS: Record<StickerMotion, Pattern> = {
  bounce: {
    duration: 820,
    origin: "50% 100%",
    frames: frames([
      [0, "translateY(0) scale(1, 1)", out],
      [0.12, "translateY(0) scale(1.06, 0.92)", out],
      [0.32, "translateY(-14%) scale(0.96, 1.05)", down],
      [0.5, "translateY(0) scale(1.05, 0.94)", out],
      [0.66, "translateY(-6%) scale(0.99, 1.02)", down],
      [0.82, "translateY(0) scale(1.02, 0.98)", out],
      [1, "translateY(0) scale(1, 1)"],
    ]),
  },
  wobble: {
    duration: 760,
    origin: "50% 90%",
    frames: frames([
      [0, "rotate(0deg)"],
      [0.16, "rotate(-9deg)"],
      [0.34, "rotate(8deg)"],
      [0.52, "rotate(-6deg)"],
      [0.7, "rotate(4deg)"],
      [0.86, "rotate(-1.5deg)"],
      [1, "rotate(0deg)"],
    ]),
  },
  beat: {
    duration: 760,
    origin: "50% 55%",
    frames: frames([
      [0, "scale(1)", out],
      [0.14, "scale(1.14)", inOut],
      [0.3, "scale(0.98)", out],
      [0.44, "scale(1.09)", inOut],
      [0.7, "scale(1)"],
      [1, "scale(1)"],
    ]),
  },
  shake: {
    duration: 560,
    origin: "50% 60%",
    frames: frames([
      [0, "translateX(0) rotate(0deg)"],
      [0.12, "translateX(-7%) rotate(-2deg)"],
      [0.28, "translateX(6%) rotate(2deg)"],
      [0.44, "translateX(-5%) rotate(-1.5deg)"],
      [0.6, "translateX(3.5%) rotate(1deg)"],
      [0.78, "translateX(-1.5%) rotate(0deg)"],
      [1, "translateX(0) rotate(0deg)"],
    ]),
  },
  shiver: {
    duration: 560,
    origin: "50% 60%",
    frames: frames([
      [0, "translateX(0)"],
      [0.1, "translateX(-2.5%)"],
      [0.2, "translateX(2.5%)"],
      [0.3, "translateX(-2.5%)"],
      [0.4, "translateX(2%)"],
      [0.5, "translateX(-2%)"],
      [0.62, "translateX(1.5%)"],
      [0.74, "translateX(-1%)"],
      [0.86, "translateX(0.5%)"],
      [1, "translateX(0)"],
    ]),
  },
  wave: {
    duration: 900,
    origin: "50% 100%",
    frames: frames([
      [0, "rotate(0deg)"],
      [0.18, "rotate(12deg)"],
      [0.38, "rotate(-9deg)"],
      [0.58, "rotate(7deg)"],
      [0.78, "rotate(-3deg)"],
      [1, "rotate(0deg)"],
    ]),
  },
  nod: {
    duration: 720,
    origin: "50% 100%",
    frames: frames([
      [0, "translateY(0) scale(1, 1)"],
      [0.22, "translateY(5%) scale(1.02, 0.96)"],
      [0.44, "translateY(0) scale(1, 1)"],
      [0.64, "translateY(4%) scale(1.015, 0.97)"],
      [0.86, "translateY(0) scale(1, 1)"],
      [1, "translateY(0) scale(1, 1)"],
    ]),
  },
  sway: {
    duration: 1100,
    origin: "50% 100%",
    frames: frames([
      [0, "rotate(0deg)"],
      [0.42, "rotate(-7deg)"],
      [0.76, "rotate(3deg)"],
      [1, "rotate(0deg)"],
    ]),
  },
  float: {
    duration: 900,
    origin: "50% 70%",
    frames: frames([
      [0, "translateY(0) scale(1)", out],
      [0.42, "translateY(-9%) scale(1.04)", inOut],
      [0.76, "translateY(1%) scale(1)", inOut],
      [1, "translateY(0) scale(1)"],
    ]),
  },
  pop: {
    duration: 560,
    origin: "50% 60%",
    frames: frames([
      [0, "scale(1)", out],
      [0.24, "scale(1.16)", inOut],
      [0.52, "scale(0.97)", inOut],
      [0.76, "scale(1.02)", inOut],
      [1, "scale(1)"],
    ]),
  },
  tilt: {
    duration: 900,
    origin: "50% 80%",
    frames: frames([
      [0, "rotate(0deg)", out],
      [0.3, "rotate(-10deg)"],
      [0.56, "rotate(-10deg)"],
      [0.82, "rotate(2deg)"],
      [1, "rotate(0deg)"],
    ]),
  },
  swing: {
    duration: 760,
    origin: "35% 90%",
    frames: frames([
      [0, "rotate(0deg)"],
      [0.3, "rotate(-12deg)", down],
      [0.48, "rotate(10deg)", out],
      [0.74, "rotate(-3deg)"],
      [1, "rotate(0deg)"],
    ]),
  },
  sink: {
    duration: 900,
    origin: "50% 100%",
    frames: frames([
      [0, "translateY(0)"],
      [0.36, "translateY(8%)"],
      [0.66, "translateY(-3%)"],
      [0.86, "translateY(1%)"],
      [1, "translateY(0)"],
    ]),
  },
  glitch: {
    duration: 520,
    origin: "50% 50%",
    frames: [
      { offset: 0, transform: "translateX(0)", opacity: 1, easing: "steps(1, end)" },
      { offset: 0.12, transform: "translateX(-5%)", opacity: 0.82, easing: "steps(1, end)" },
      { offset: 0.24, transform: "translateX(4%)", opacity: 1, easing: "steps(1, end)" },
      { offset: 0.36, transform: "translateX(-3%)", opacity: 0.86, easing: "steps(1, end)" },
      { offset: 0.5, transform: "translateX(2%)", opacity: 1, easing: "steps(1, end)" },
      { offset: 0.64, transform: "translateX(0)", opacity: 1 },
      { offset: 1, transform: "translateX(0)", opacity: 1 },
    ],
  },
};

const running = new WeakMap<Element, Animation>();

/**
 * Plays the pattern once on `element`, unless one is already under way there (it finishes).
 * Returns the animation, or null when it did not start.
 */
export function playStickerMotion(element: HTMLElement, motion: StickerMotion): Animation | null {
  const current = running.get(element);
  if (current && current.playState === "running") return null;
  const pattern = STICKER_PATTERNS[motion];
  if (typeof element.animate !== "function") return null;
  element.style.transformOrigin = pattern.origin;
  const animation = element.animate(pattern.frames, { duration: pattern.duration });
  running.set(element, animation);
  const done = () => {
    if (running.get(element) === animation) running.delete(element);
  };
  animation.addEventListener("finish", done);
  animation.addEventListener("cancel", done);
  return animation;
}

/** The arrival under reduced motion or in Performance mode: a fade, nothing moves. */
export function fadeInSticker(element: HTMLElement): Animation | null {
  if (typeof element.animate !== "function") return null;
  return element.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 200, easing: out });
}
