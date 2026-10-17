// A place files can be dropped on: a dashed panel with "Buraya bırak" in the middle and one line
// on what happens. The app decides the state (files over the window, the cursor over the zone,
// files it refuses, the drop) and this draws it. Only transform and opacity move, a new state
// retargets from wherever the last one is, and reduced motion or Performance mode keep the fades
// (docs/design.md §5). Gold pixel brackets close in on the zone under the cursor: brackets mark
// the target (docs/design.md §1).
import { Prohibit, TrayArrowDown } from "@phosphor-icons/react";
import {
  AnimatePresence,
  motion,
  type TargetAndTransition,
  type Transition,
  type Variants,
} from "motion/react";
import { type HTMLAttributes, type ReactNode, type Ref, useMemo } from "react";
import { GatedMotion } from "../entrance";
import { cn } from "../lib/cn";
import { easeOutExpo, useMotionPrefs } from "../motion";

/**
 * - `idle`: no files are being dragged (only zones that stay on the page are drawn then);
 * - `ready`: files are over the window and this zone would take them;
 * - `hover`: the cursor is over this zone;
 * - `recede`: another zone is under the cursor;
 * - `refuse`: these files cannot go here;
 * - `absorb`: the files were just dropped here.
 */
export type DropZoneState = "idle" | "ready" | "hover" | "recede" | "refuse" | "absorb";

/**
 * - `inline`: stays on the page (an empty tab, a dialog step) and reacts while files are dragged;
 * - `cover`: covers a panel's content while files are dragged over the window;
 * - `window`: the window-wide zone, drawn over a scrim.
 */
export type DropZoneVariant = "inline" | "cover" | "window";

export interface DropZoneProps extends Omit<HTMLAttributes<HTMLDivElement>, "title" | "children"> {
  state: DropZoneState;
  /** "Buraya bırak", or why these files cannot go here. */
  title: string;
  /** What happens to the files, in one short line; `null` while the files are still being read. */
  hint?: string | null;
  /** The tile's mark (a tray with an arrow by default); a "no" mark replaces it on `refuse`. */
  icon?: ReactNode;
  variant?: DropZoneVariant;
  /** Inline zones: buttons that do the same without dragging (pick a file, browse). */
  children?: ReactNode;
  /** Under the text, out of the flow so nothing moves when it arrives (the window's chips). */
  below?: ReactNode;
  /** A refusing zone steps back while the cursor is over another zone. */
  dimmed?: boolean;
  /** The zone's box. It never moves (the lift is inside it), so hit-testing cannot flicker. */
  ref?: Ref<HTMLDivElement>;
}

type Label = DropZoneState | "initial" | "exit";
type Look = TargetAndTransition;

const ease = (duration: number, delay = 0): Transition => ({ duration, ease: easeOutExpo, delay });

/** Seconds to reach each state: the cursor's state takes longest, leaving it is quicker. */
const timing: Record<DropZoneState, number> = {
  idle: 0.18,
  ready: 0.18,
  hover: 0.26,
  recede: 0.18,
  refuse: 0.2,
  absorb: 0.22,
};

/**
 * One layer's variants: its look in each state, eased out over that state's time (a look can
 * bring its own `transition`).
 */
function layer(looks: Partial<Record<Label, Look>>, seconds: Partial<Record<Label, number>> = {}) {
  const variants: Variants = {};
  for (const [label, look] of Object.entries(looks) as [Label, Look][]) {
    const time = seconds[label] ?? timing[label as DropZoneState] ?? 0.14;
    variants[label] = label === "initial" ? look : { transition: ease(time), ...look };
  }
  return variants;
}

/** Opacity of a layer per state, in the order idle, ready, hover, recede, refuse, absorb. */
function fade(values: [number, number, number, number, number, number], initial = values[0]) {
  const [idle, ready, hover, recede, refuse, absorb] = values;
  return layer({
    initial: { opacity: initial },
    idle: { opacity: idle },
    ready: { opacity: ready },
    hover: { opacity: hover },
    recede: { opacity: recede },
    refuse: { opacity: refuse },
    absorb: { opacity: absorb },
  });
}

// Files over the window: the frame brightens and warms; under the cursor it turns gold.
const neutralFrame = fade([0.6, 0.75, 0, 0.6, 1, 0]);
const goldFrame = fade([0, 0.4, 1, 0, 0, 1]);
const wash = fade([0, 0.4, 1, 0, 0, 1]);
const liftShadow = fade([0, 0, 1, 0, 0, 0]);
const tileGold = fade([0, 0.55, 1, 0, 0, 1]);
const refuseMark = fade([0, 0, 0, 0, 1, 0]);
/** Buttons step back while files hover the zone: the message is the thing to read then. */
const extras = fade([1, 1, 0.4, 1, 1, 0.4]);

const corners = [
  { key: "tl", x: -1, y: -1 },
  { key: "tr", x: 1, y: -1 },
  { key: "bl", x: -1, y: 1 },
  { key: "br", x: 1, y: 1 },
] as const;

function motionVariants(variant: DropZoneVariant, still: boolean) {
  const move = (value: string) => (still ? "none" : value);
  const rest = { opacity: 1, transform: move("translateY(0px) scale(1)") };
  const root = layer(
    {
      // Covers and the window zone fade and grow in from 98% as a drag enters.
      initial:
        variant === "inline"
          ? rest
          : { opacity: 0, transform: move("translateY(0px) scale(0.98)") },
      idle: rest,
      ready: rest,
      // A slight lift toward the cursor; the window zone is the window itself and stays put.
      hover: {
        opacity: 1,
        transform: move(`translateY(${variant === "inline" ? -2 : 0}px) scale(1)`),
      },
      recede: { opacity: 0.5, transform: move("translateY(0px) scale(0.99)") },
      refuse: rest,
      // Taken: the zone contracts. A cover or the window zone goes with it, fading a beat later
      // so the contraction reads; an inline zone comes back as the page shows the result.
      absorb:
        variant === "inline"
          ? { opacity: 1, transform: move("translateY(0px) scale(0.975)") }
          : {
              opacity: 0,
              transform: move("translateY(0px) scale(0.965)"),
              transition: { transform: ease(0.3), opacity: ease(0.22, 0.08) },
            },
      exit: { opacity: 0, transform: move("translateY(0px) scale(0.99)") },
    },
    {
      ready: variant === "inline" ? timing.ready : 0.24,
      absorb: variant === "inline" ? 0.16 : 0.3,
    },
  );
  const scaleRest = { transform: move("scale(1)") };
  const tile = layer(
    {
      initial: scaleRest,
      idle: scaleRest,
      ready: scaleRest,
      hover: scaleRest,
      recede: scaleRest,
      refuse: scaleRest,
      absorb: { transform: move("scale(0.92)") },
    },
    { absorb: 0.16 },
  );
  // The tray's mark nudges toward the zone as the cursor arrives and sinks in on the drop.
  const at = (px: number) => ({ opacity: 1, transform: move(`translateY(${px}px)`) });
  const mark = layer(
    {
      initial: at(0),
      idle: at(0),
      ready: at(0),
      hover: at(2),
      recede: at(0),
      refuse: { opacity: 0, transform: move("translateY(0px)") },
      absorb: { opacity: 0.5, transform: move("translateY(6px)") },
    },
    { hover: 0.22 },
  );
  const brackets = corners.map(({ x, y }) => {
    const shift = (px: number) => move(`translate(${x * px}px, ${y * px}px)`);
    const away = { opacity: 0, transform: shift(6) };
    return layer(
      {
        initial: away,
        idle: away,
        ready: away,
        // Lock on: the brackets slide in to the zone's corners.
        hover: { opacity: 1, transform: shift(0) },
        recede: away,
        refuse: away,
        // Taken: they close in a little more as they fade.
        absorb: { opacity: 0, transform: shift(-5) },
        exit: { opacity: 0 },
      },
      { idle: 0.14, ready: 0.14, hover: 0.28, recede: 0.14, refuse: 0.14, absorb: 0.24 },
    );
  });
  return { root, tile, mark, brackets };
}

const sizes = {
  md: { tile: 44, cut: "[--cut:7px]", mark: 22, arm: 12, thick: 2, offset: 5 },
  lg: { tile: 56, cut: "[--cut:8px]", mark: 28, arm: 22, thick: 3, offset: 6 },
} as const;

/** Text that changes in place: the old line fades out under the new one, nothing moves. */
function Swap({ text, className }: { text: string; className: string }) {
  return (
    <span className={cn("grid min-h-[1lh] w-full justify-items-center", className)}>
      <AnimatePresence initial={false}>
        <motion.span
          key={text}
          className="col-start-1 row-start-1 max-w-full min-w-0 truncate"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: ease(0.18, 0.04) }}
          exit={{ opacity: 0, transition: ease(0.1) }}
        >
          {text}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

export function DropZone({
  state,
  title,
  hint,
  icon,
  variant = "inline",
  children,
  below,
  dimmed = false,
  className,
  ref,
  ...props
}: DropZoneProps) {
  // Reduced motion and Performance mode: fades only, no movement.
  const still = !useMotionPrefs().layout;
  const variants = useMemo(() => motionVariants(variant, still), [variant, still]);
  const large = variant === "window";
  const size = sizes[large ? "lg" : "md"];
  const text = large ? "text-[14px] leading-[1.45]" : "text-[13px] leading-[1.45]";
  // The window-wide frame is far from everything else: a little stronger to read as a frame.
  const stroke = large ? "border-2" : "border-[1.5px]";
  return (
    // The box stretches the zone to its own size (an inset cover, a minimum height).
    <div
      ref={ref}
      data-drop-zone={variant}
      data-state={state}
      className={cn("relative flex", className)}
      {...props}
    >
      <motion.div
        className="flex w-full"
        initial={false}
        animate={{ opacity: dimmed ? 0.5 : 1, transition: ease(0.18) }}
      >
        {/* Inside an `EntranceGate`, a cover or the window zone holds its entrance until its
            layer is on screen (docs/design.md §5). */}
        <GatedMotion.div
          initial={variant === "inline" ? false : "initial"}
          animate={state}
          exit="exit"
          variants={variants.root}
          className={cn(
            "relative isolate flex w-full flex-col items-center justify-center text-center",
            large ? "rounded-[14px]" : "rounded-[var(--radius-panel)]",
            variant === "inline" && "px-6 py-10",
          )}
        >
          {variant === "inline" ? (
            <motion.span
              aria-hidden
              variants={liftShadow}
              className="pointer-events-none absolute inset-0 rounded-[inherit] shadow-[0_18px_44px_rgb(34_6_6/0.45)]"
            />
          ) : null}
          <span
            aria-hidden
            className={cn(
              "pointer-events-none absolute inset-0 rounded-[inherit]",
              variant === "inline" && "bg-obsidian-950/30",
              variant === "cover" && "bg-obsidian-950/92",
            )}
          />
          <motion.span
            aria-hidden
            variants={wash}
            className={cn(
              "pointer-events-none absolute inset-0 rounded-[inherit]",
              large
                ? "bg-[radial-gradient(620px_circle_at_50%_50%,rgb(232_154_44/0.1),transparent_72%)]"
                : "bg-[radial-gradient(70%_80%_at_50%_45%,rgb(232_154_44/0.1),transparent_75%)]",
            )}
          />
          <motion.span
            aria-hidden
            variants={neutralFrame}
            className={cn(
              "pointer-events-none absolute inset-0 rounded-[inherit] border-dashed border-white/26",
              stroke,
            )}
          />
          <motion.span
            aria-hidden
            variants={goldFrame}
            className={cn(
              "pointer-events-none absolute inset-0 rounded-[inherit] border-dashed border-gold-400/90",
              stroke,
            )}
          />
          {corners.map((corner, index) => (
            <motion.span
              key={corner.key}
              aria-hidden
              variants={variants.brackets[index]}
              className="pointer-events-none absolute"
              style={{
                width: size.arm,
                height: size.arm,
                [corner.y < 0 ? "top" : "bottom"]: -size.offset,
                [corner.x < 0 ? "left" : "right"]: -size.offset,
              }}
            >
              <span
                className="absolute w-full bg-gold-300"
                style={{ height: size.thick, [corner.y < 0 ? "top" : "bottom"]: 0 }}
              />
              <span
                className="absolute h-full bg-gold-300"
                style={{ width: size.thick, [corner.x < 0 ? "left" : "right"]: 0 }}
              />
            </motion.span>
          ))}

          <div className="relative flex w-full max-w-[560px] flex-col items-center px-6">
            <motion.span
              aria-hidden
              variants={variants.tile}
              className={cn(
                "chamfer relative flex shrink-0 items-center justify-center overflow-hidden bg-obsidian-800",
                size.cut,
              )}
              style={{ width: size.tile, height: size.tile }}
            >
              <motion.span
                variants={tileGold}
                className="absolute inset-0 bg-[radial-gradient(circle_at_50%_35%,rgb(232_154_44/0.32),rgb(27_24_32/0.95)_72%)]"
              />
              <motion.span variants={variants.mark} className="relative flex text-gold-300">
                {icon ?? <TrayArrowDown size={size.mark} weight="duotone" />}
              </motion.span>
              <motion.span variants={refuseMark} className="absolute flex text-ash-400">
                <Prohibit size={size.mark} />
              </motion.span>
            </motion.span>
            <Swap
              text={title}
              className={cn(
                "mt-4 font-display font-semibold text-bone-50",
                large ? "text-[24px] leading-[1.15]" : "text-[16px] leading-[1.3]",
              )}
            />
            {hint === undefined ? null : hint === null ? (
              // Still reading the files (usually milliseconds): a quiet bar only when it is not.
              <span className={cn("mt-1.5 flex min-h-[1lh] items-center", text)}>
                <motion.span
                  aria-hidden
                  className="block h-2.5 w-40 rounded-full bg-white/7"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1, transition: ease(0.2, 0.16) }}
                />
              </span>
            ) : (
              <Swap text={hint} className={cn("mt-1.5 text-ash-300", text)} />
            )}
            {children ? (
              <motion.div variants={extras} className="mt-5 flex flex-wrap justify-center gap-2">
                {children}
              </motion.div>
            ) : null}
            {below ? (
              <div className="absolute inset-x-6 top-full mt-4 flex flex-col items-center">
                {below}
              </div>
            ) : null}
          </div>
        </GatedMotion.div>
      </motion.div>
    </div>
  );
}
