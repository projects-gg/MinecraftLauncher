// Reduced motion for the animations motion runs as Web Animations (docs/design.md §5).
//
// `MotionConfig reducedMotion="user"` makes x, y, scale, rotate and layout animations jump, but
// not a `transform` string (`animate={{ transform: "translateY(0px)" }}`), the form this app uses
// for entrances and fills because it runs on the compositor. POLISH3's audit found the hero copy,
// the news column, the OYNA fill and the tabs' "unsaved" dot still sliding or growing under the OS
// setting; dozens of components write such strings, and the next one would too. So under reduced
// motion every Web Animation holds its movement at its final value for the whole animation: the
// element lands at once, and a fade in the same animation, or in its own (motion makes one per
// value), plays as written. Nothing changes without the OS setting.
const MOVEMENT = ["transform", "translate", "scale", "rotate", "offsetDistance"] as const;

type Frames = Keyframe[] | PropertyIndexedKeyframes;

const isMovement = (key: string) => (MOVEMENT as readonly string[]).includes(key);

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
 * Installs the guard once, before anything renders (each app's main.tsx). It reads the OS setting
 * at every animation, so a change while the app runs applies to the next one.
 */
export function guardReducedMotion(): void {
  if (installed || typeof window === "undefined" || typeof Element === "undefined") return;
  if (typeof Element.prototype.animate !== "function" || typeof window.matchMedia !== "function") {
    return;
  }
  installed = true;
  const query = window.matchMedia("(prefers-reduced-motion: reduce)");
  const animate = Element.prototype.animate;
  Element.prototype.animate = function guardedAnimate(
    this: Element,
    keyframes: Frames | null,
    options?: number | KeyframeAnimationOptions,
  ): Animation {
    if (query.matches) {
      const still = stillKeyframes(keyframes);
      if (still) return animate.call(this, still, options);
    }
    return animate.call(this, keyframes, options);
  };
}
