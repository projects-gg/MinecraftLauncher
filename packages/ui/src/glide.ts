// The gliding plate of `Segmented` (docs/design.md §5, MOTION1).
//
// The plate used to be a shared layout animation (motion's `layoutId`): it flew from the old
// option's place on the screen to the new option's place. A dialog that changes size with the
// choice moves the whole control between those two measurements (the new instance dialog grows and
// re-centres when "Mod paketi bul" lists packs), so the plate rose into the control from 60 px
// below it. Now the plate rests inside the chosen option, and a pick glides it along the row,
// measured inside the control: whatever a dialog, a scroller or the window does around the
// control, the plate stays on its row.
//
// While it glides, the plate is a window over the row: two clipping layers, one holding the right
// edge with the bottom-right cut, one the left edge with the top-left cut, filled with gold and
// holding a copy of every label in the plate's ink. Only their translations move (on the
// compositor, smooth while the page renders what the pick brings): the plate takes each width
// without stretching its cut corners, and a label is dark exactly where the plate is over it and
// light beside it, never pale. The motion is the sliding markers' spring (480/38), solved exactly
// and sampled into keyframes, so a pick during a glide starts from where the plate is, with the
// speed it has.

export interface Spring {
  stiffness: number;
  damping: number;
  mass?: number;
}

/** One coordinate of the plate: its offset from where it comes to rest (px) and its speed (px/s). */
export interface Drift {
  x: number;
  v: number;
}

/** The plate's left edge, right edge and top, each against the option it glides to. */
export interface PlateDrift {
  left: Drift;
  right: Drift;
  top: Drift;
}

/** An option's place inside its control, in the control's own pixels. */
export interface OptionBox {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/** The sliding markers' spring (docs/design.md §5). */
export const PLATE_SPRING: Spring = { stiffness: 480, damping: 38 };

/** The plate's cut corners, px (the `--cut` of `chamfer`). */
export const PLATE_CUT = 5;

/** A glide ends once every edge is this close to rest (px) and slower than this (px/s). */
const REST_PX = 0.2;
const REST_SPEED = 6;

/** Keyframes are sampled this often (s): the compositor draws straight lines between them. */
const SAMPLE_S = 1 / 240;

/** The longest a glide may take (s), whatever the spring. */
const LONGEST_S = 1.2;

const STILL: Drift = { x: 0, v: 0 };
const AT_REST: PlateDrift = { left: STILL, right: STILL, top: STILL };

/**
 * Where a spring released at `from` is `t` seconds later: the exact solution of the damped
 * oscillator, so a glide can be sampled ahead and picked up again at any moment.
 */
export function springAt(spring: Spring, from: Drift, t: number): Drift {
  const mass = spring.mass ?? 1;
  const w0 = Math.sqrt(spring.stiffness / mass);
  const zeta = spring.damping / (2 * Math.sqrt(spring.stiffness * mass));
  const { x: x0, v: v0 } = from;
  if (zeta < 1) {
    const a = zeta * w0;
    const wd = w0 * Math.sqrt(1 - zeta * zeta);
    const fade = Math.exp(-a * t);
    const cos = Math.cos(wd * t);
    const sin = Math.sin(wd * t);
    return {
      x: fade * (x0 * cos + ((v0 + a * x0) / wd) * sin),
      v: fade * (v0 * cos - ((w0 * w0 * x0 + a * v0) / wd) * sin),
    };
  }
  if (zeta === 1) {
    const fade = Math.exp(-w0 * t);
    const b = v0 + w0 * x0;
    return { x: fade * (x0 + b * t), v: fade * (v0 - w0 * b * t) };
  }
  const root = w0 * Math.sqrt(zeta * zeta - 1);
  const r1 = -zeta * w0 + root;
  const r2 = -zeta * w0 - root;
  const c2 = (v0 - r1 * x0) / (r2 - r1);
  const c1 = x0 - c2;
  const e1 = Math.exp(r1 * t);
  const e2 = Math.exp(r2 * t);
  return { x: c1 * e1 + c2 * e2, v: r1 * c1 * e1 + r2 * c2 * e2 };
}

/** Where every edge of a glide that started at `drift` is `t` seconds later. */
export function plateAt(spring: Spring, drift: PlateDrift, t: number): PlateDrift {
  return {
    left: springAt(spring, drift.left, t),
    right: springAt(spring, drift.right, t),
    top: springAt(spring, drift.top, t),
  };
}

const rests = (d: Drift) => Math.abs(d.x) < REST_PX && Math.abs(d.v) < REST_SPEED;
const atRest = (d: PlateDrift) => rests(d.left) && rests(d.right) && rests(d.top);

/** Seconds until the plate rests (0 when it already does). */
export function settleTime(spring: Spring, drift: PlateDrift): number {
  for (let t = 0; t < LONGEST_S; t += SAMPLE_S) {
    if (atRest(plateAt(spring, drift, t))) return t;
  }
  return LONGEST_S;
}

/**
 * The plate's drift against another option: the same edges on the screen and the same speeds,
 * measured from the new option's place. `from` is the option the drift was measured against.
 */
export function rebase(drift: PlateDrift, from: OptionBox, to: OptionBox): PlateDrift {
  return {
    left: { x: from.left + drift.left.x - to.left, v: drift.left.v },
    right: { x: from.right + drift.right.x - to.right, v: drift.right.v },
    top: { x: from.top + drift.top.x - to.top, v: drift.top.v },
  };
}

export interface GlideFrames {
  /** Milliseconds. */
  duration: number;
  /** The layer that holds the right edge (and the bottom-right cut): follows the right edge. */
  right: Keyframe[];
  /** The layer inside it that holds the left edge (and the top-left cut). */
  left: Keyframe[];
  /** The labels in ink inside both: they stay where the labels are. */
  ink: Keyframe[];
}

const round = (value: number, places: number) => {
  const unit = 10 ** places;
  return Math.round(value * unit) / unit;
};
const translate = (x: number, y: number) => `translate(${round(x, 3)}px, ${round(y, 3)}px)`;

/**
 * Keyframes that glide the plate from `drift` to rest under its option: the right layer moves
 * with the right edge, the left layer (inside it) by what the left edge moves more, and the ink
 * labels (inside both) back by the left edge's move, so they never move at all. `null` when
 * nothing would move.
 */
export function glideFrames(spring: Spring, drift: PlateDrift): GlideFrames | null {
  const seconds = settleTime(spring, drift);
  if (seconds <= 0) return null;
  const steps = Math.max(1, Math.ceil(seconds / SAMPLE_S));
  const frames: GlideFrames = {
    duration: Math.round(seconds * 1000),
    right: [],
    left: [],
    ink: [],
  };
  for (let i = 0; i <= steps; i++) {
    const last = i === steps;
    const offset = last ? 1 : i / steps;
    const now = last ? AT_REST : plateAt(spring, drift, offset * seconds);
    const left = round(now.left.x, 3);
    const right = round(now.right.x, 3);
    const top = round(now.top.x, 3);
    frames.right.push({ offset, transform: translate(right, top) });
    frames.left.push({ offset, transform: translate(left - right, 0) });
    frames.ink.push({ offset, transform: translate(-left, -top) });
  }
  return frames;
}

/** A glide under way: the option it goes to and the animations of the window over the row. */
export interface Glide {
  option: HTMLElement;
  control: HTMLElement;
  token: string;
  drift: PlateDrift;
  duration: number;
  animations: Animation[];
}

/** Where a glide's plate is now, against its option (at rest once the glide is over). */
function glideNow(glide: Glide): PlateDrift {
  const time = glide.animations[0]?.currentTime;
  const ms = typeof time === "number" ? time : glide.duration;
  if (ms >= glide.duration) return AT_REST;
  return plateAt(PLATE_SPRING, glide.drift, Math.max(0, ms) / 1000);
}

/** Stops a glide where it is; the chosen option shows its own plate again. */
export function stopGlide(glide: Glide): void {
  for (const animation of glide.animations) animation.cancel();
  if (glide.control.dataset.gliding === glide.token) delete glide.control.dataset.gliding;
}

/** The control's options (`data-option`), in order. */
function optionsOf(control: HTMLElement): HTMLElement[] {
  return [...control.children].filter(
    (child): child is HTMLElement => child instanceof HTMLElement && "option" in child.dataset,
  );
}

/**
 * Boxes inside the control, in its own pixels from its padding edge (where its absolutely placed
 * layers start): the screen positions, freed from whatever scales or moves the control (a dialog's
 * entrance, its glide to a new size, a scroller).
 */
function boxesIn(control: HTMLElement, elements: HTMLElement[]): OptionBox[] | null {
  const frame = control.getBoundingClientRect();
  const scaleX = control.offsetWidth > 0 ? frame.width / control.offsetWidth : 0;
  const scaleY = control.offsetHeight > 0 ? frame.height / control.offsetHeight : 0;
  if (!(scaleX > 0) || !(scaleY > 0)) return null;
  const x0 = frame.left + control.clientLeft * scaleX;
  const y0 = frame.top + control.clientTop * scaleY;
  return elements.map((element) => {
    const box = element.getBoundingClientRect();
    return {
      left: (box.left - x0) / scaleX,
      right: (box.right - x0) / scaleX,
      top: (box.top - y0) / scaleY,
      bottom: (box.bottom - y0) / scaleY,
    };
  });
}

const place = (
  element: HTMLElement,
  left: number,
  top: number,
  width?: number,
  height?: number,
) => {
  element.style.left = `${left}px`;
  element.style.top = `${top}px`;
  if (width !== undefined) element.style.width = `${width}px`;
  if (height !== undefined) element.style.height = `${height}px`;
};

let serial = 0;

/**
 * Glides the plate from the option of `from` (or from where a glide under way has it) to the
 * option of `to`, and returns the glide. Under reduced motion the plate is simply there and the
 * labels fade (styles).
 */
export function glidePlate(
  control: HTMLElement,
  from: string,
  to: string,
  previous: Glide | null,
): Glide | null {
  const options = optionsOf(control);
  const was = options.findIndex((option) => option.dataset.option === from);
  const will = options.findIndex((option) => option.dataset.option === to);
  const source = options[was];
  const target = options[will];
  const now = previous && previous.option === source ? glideNow(previous) : AT_REST;
  if (previous) stopGlide(previous);
  if (!source || !target || source === target || reducedMotion()) return null;
  const glide = control.querySelector<HTMLElement>(":scope > [data-glide]");
  const right = glide?.querySelector<HTMLElement>("[data-glide-right]");
  const left = glide?.querySelector<HTMLElement>("[data-glide-left]");
  const ink = glide?.querySelector<HTMLElement>("[data-glide-ink]");
  const copies = ink ? [...ink.querySelectorAll<HTMLElement>("[data-glide-label]")] : [];
  const labels = options.map((option) => option.querySelector<HTMLElement>('[data-label="base"]'));
  if (!right || !left || !ink || typeof right.animate !== "function") return null;
  if (copies.length !== options.length || labels.some((label) => !label)) return null;
  const boxes = boxesIn(control, [...options, ...(labels as HTMLElement[])]);
  const sourceBox = boxes?.[was];
  const targetBox = boxes?.[will];
  if (!boxes || !sourceBox || !targetBox) return null;
  const drift = rebase(now, sourceBox, targetBox);
  const frames = glideFrames(PLATE_SPRING, drift);
  if (!frames) return null;
  // The window at rest over the chosen option: the right layer reaches from far left to its right
  // edge, the left layer from its left edge to far right, the ink labels where the labels are.
  const reach = control.clientWidth + control.clientHeight;
  const width = targetBox.right - targetBox.left;
  place(right, targetBox.right - reach, targetBox.top, reach, targetBox.bottom - targetBox.top);
  place(left, reach - width, 0, reach);
  place(ink, 0, 0);
  copies.forEach((copy, index) => {
    const label = boxes[options.length + index];
    if (label) place(copy, label.left - targetBox.left, label.top - targetBox.top);
  });
  serial += 1;
  const token = String(serial);
  control.dataset.gliding = token;
  const timing = { duration: frames.duration };
  const animations = [
    right.animate(frames.right, timing),
    left.animate(frames.left, timing),
    ink.animate(frames.ink, timing),
  ];
  animations[0]?.finished.then(
    () => {
      if (control.dataset.gliding === token) delete control.dataset.gliding;
    },
    () => {},
  );
  return { option: target, control, token, drift, duration: frames.duration, animations };
}

function reducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}
