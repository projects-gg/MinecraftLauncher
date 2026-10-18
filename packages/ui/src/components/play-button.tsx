import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { easeOutExpo } from "../motion";
import { useAmbientMotion, useInteractionMotion } from "../perf";

export type PlayState =
  | { kind: "ready" }
  | { kind: "update" }
  | { kind: "working"; percent: number; label: string }
  | { kind: "launching"; label: string }
  | { kind: "running" }
  | { kind: "unavailable"; label: string };

/** Visible labels (localized). */
export interface PlayButtonLabels {
  play: string;
  update: string;
  cancel: string;
  running: string;
  /**
   * Where the language puts the percent sign: before the figure in Turkish ("%42", the
   * default), after it in English ("42%").
   */
  percentFirst?: boolean;
}

function percentText(percent: number, labels: PlayButtonLabels): string {
  const figure = Math.floor(percent);
  return labels.percentFirst === false ? `${figure}%` : `%${figure}`;
}

interface PlayButtonProps {
  state: PlayState;
  onPlay: () => void;
  onCancel?: () => void;
  labels: PlayButtonLabels;
  className?: string;
  /** `aria-keyshortcuts` of the app's shortcut that presses the button (the launcher's Ctrl+Enter). */
  keyShortcuts?: string;
}

/**
 * The button's accessible name says what a press does. While a launch is prepared the button shows
 * its progress, and once a press would cancel it the name leads with that ("İptal, Oyun dosyaları
 * indiriliyor %42"); in the moment after OYNA, when a press does nothing, it is the progress alone.
 * Tests: apps/launcher/src/shell/play-button.test.ts.
 */
export function playButtonName(
  state: PlayState,
  labels: PlayButtonLabels,
  cancelReady: boolean,
): string {
  switch (state.kind) {
    case "ready":
      return labels.play;
    case "update":
      return labels.update;
    case "working": {
      const progress = `${state.label} ${percentText(state.percent, labels)}`;
      return cancelReady ? `${labels.cancel}, ${progress}` : progress;
    }
    case "running":
      return labels.running;
    case "launching":
    case "unavailable":
      return state.label;
  }
}

/**
 * The one loud element: a gold, chamfered PLAY button on a darker socket, with a soft glow
 * and a periodic sheen when ready. Ambient effects respect reduced motion and Performance mode.
 */
export function PlayButton({
  state,
  onPlay,
  onCancel,
  labels,
  className,
  keyShortcuts,
}: PlayButtonProps) {
  const ambient = useAmbientMotion();
  const animate = useInteractionMotion();
  const [hover, setHover] = useState(false);
  // "İptal" waits after OYNA: until the pointer has left the button once (or, from the keyboard,
  // a moment), so a double click or a resting pointer never cancels the launch it just started.
  const [cancelArmed, setCancelArmed] = useState(true);
  const pointerInside = useRef(false);
  useEffect(() => {
    if (cancelArmed) return;
    const id = window.setTimeout(() => {
      if (!pointerInside.current) setCancelArmed(true);
    }, 800);
    return () => window.clearTimeout(id);
  }, [cancelArmed]);

  const isGold = state.kind === "ready" || state.kind === "update";
  const isWorking = state.kind === "working";
  const disabled =
    state.kind === "launching" || state.kind === "running" || state.kind === "unavailable";
  const showCancel = hover && Boolean(onCancel) && cancelArmed;

  let text: ReactNode;
  let key: string;
  switch (state.kind) {
    case "ready":
      text = labels.play;
      key = "play";
      break;
    case "update":
      text = labels.update;
      key = "update";
      break;
    case "working":
      // Turkish puts the percent sign first, English after the figure; Tektur's % glyph is
      // ornate, so it is set in Figtree.
      text = showCancel ? (
        labels.cancel
      ) : labels.percentFirst === false ? (
        <>
          {Math.floor(state.percent)}
          <span className="ml-0.5 font-sans text-[16px] font-bold">%</span>
        </>
      ) : (
        <>
          <span className="mr-0.5 font-sans text-[16px] font-bold">%</span>
          {Math.floor(state.percent)}
        </>
      );
      key = showCancel ? "cancel" : "percent";
      break;
    case "launching":
      text = state.label;
      key = "launching";
      break;
    case "running":
      text = labels.running;
      key = "running";
      break;
    case "unavailable":
      text = state.label;
      key = "unavailable";
      break;
  }

  const handleClick = () => {
    if (isWorking) {
      if (cancelArmed) onCancel?.();
    } else if (isGold) {
      setCancelArmed(false);
      onPlay();
    }
  };

  return (
    <div className={cn("relative isolate h-[48px] w-[204px]", className)}>
      {/* Glow: not clipped, so it can bloom past the chamfered edges. */}
      <motion.div
        aria-hidden
        className="pointer-events-none absolute -inset-x-5 -inset-y-3 -z-20 rounded-[50%] bg-[radial-gradient(closest-side,rgb(240_184_73/0.55),rgb(232_154_44/0.18)_55%,transparent)] blur-xl"
        initial={false}
        animate={{ opacity: isGold ? (hover ? 1 : 0.7) : isWorking ? 0.35 : 0 }}
        transition={{ duration: 0.3 }}
      />
      {/* Socket plate */}
      <div
        aria-hidden
        className="chamfer absolute -inset-[4px] -z-10 bg-[linear-gradient(180deg,rgb(63_10_10/0.9),rgb(10_9_12/0.95))] [--cut:15px]"
      />
      <button
        type="button"
        disabled={disabled}
        onClick={handleClick}
        onPointerEnter={() => {
          pointerInside.current = true;
          setHover(true);
        }}
        onPointerLeave={() => {
          pointerInside.current = false;
          setHover(false);
          setCancelArmed(true);
        }}
        aria-label={playButtonName(state, labels, cancelArmed && Boolean(onCancel))}
        aria-keyshortcuts={keyShortcuts}
        // What a press does now: the app's shortcut presses only "ready" and "update".
        data-play-state={state.kind}
        className={cn(
          "group relative block size-full cursor-pointer select-none focus-visible:outline-none",
          "transition-transform duration-150 enabled:active:scale-[0.98] enabled:hover:scale-[1.015]",
          "disabled:cursor-default",
        )}
      >
        <span
          aria-hidden
          className="chamfer absolute -inset-[4px] bg-gold-200 opacity-0 transition-opacity [--cut:14px] group-focus-visible:opacity-100"
        />
        <span
          aria-hidden
          className={cn(
            "chamfer absolute inset-0 overflow-hidden [--cut:12px] transition-[background,filter] duration-300",
            isGold &&
              "bg-[linear-gradient(180deg,var(--color-gold-200),var(--color-gold-400)_38%,var(--color-gold-500)_62%,var(--color-gold-600))] group-hover:brightness-[1.07]",
            isWorking && "bg-obsidian-800",
            state.kind === "launching" &&
              "bg-[linear-gradient(180deg,var(--color-gold-400),var(--color-gold-600))] brightness-90",
            state.kind === "running" &&
              "bg-[linear-gradient(180deg,var(--color-obsidian-700),var(--color-obsidian-800))]",
            state.kind === "unavailable" && "bg-obsidian-800 opacity-80",
          )}
        >
          {/* Inner bevel */}
          <span className="chamfer absolute inset-[2px] [--cut:11px] shadow-[inset_0_1px_0_rgb(255_255_255/0.55),inset_0_-2px_0_rgb(63_10_10/0.35)]" />
          {isWorking ? (
            // The fill slides in from the left (transform only, no layout work per step).
            <motion.span
              className="absolute inset-0 bg-[linear-gradient(180deg,var(--color-gold-400),var(--color-gold-600))]"
              initial={false}
              animate={{
                transform: `translateX(${Math.min(100, Math.max(0, state.percent)) - 100}%)`,
              }}
              transition={{ duration: 0.35, ease: easeOutExpo }}
            />
          ) : null}
          {isGold && ambient ? <PlaySheen /> : null}
        </span>
        <span className="relative z-10 flex size-full flex-col items-center justify-center">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span
              key={key}
              // Reduced motion keeps the cross-fade, without the rise.
              initial={{ opacity: 0, y: animate ? 8 : 0 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: animate ? -6 : 0 }}
              transition={{ duration: animate ? 0.22 : 0.14, ease: easeOutExpo }}
              className={cn(
                "font-display leading-none font-bold tracking-[0.08em] uppercase",
                isGold && "text-[23px] text-gold-ink",
                state.kind === "launching" && "text-[16px] tracking-[0.06em] text-gold-ink",
                !isGold && state.kind !== "launching" && "text-[19px]",
                isWorking && (key === "cancel" ? "text-flare-400" : "text-bone-50 tabular-nums"),
                state.kind === "running" && "text-jade-400",
                state.kind === "unavailable" && "text-[14px] text-ash-300",
              )}
            >
              {text}
            </motion.span>
          </AnimatePresence>
        </span>
      </button>
    </div>
  );
}

/** The sheen crosses the button in the last 28 % of every 7 s; the rest of the time it waits. */
const SHEEN_CYCLE_MS = 7000;
const SHEEN_SWEEP_MS = 1960;

/**
 * The periodic sheen across a ready OYNA. Each crossing is a short animation of its own and
 * nothing runs in between: as one endless keyframe animation that stood still for 5 s of every
 * 7, it kept the compositor drawing a frame per display refresh on every page, about 6 % of a
 * CPU core for a picture that did not change (OPT1).
 */
function PlaySheen() {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const sheen = ref.current;
    if (!sheen || typeof sheen.animate !== "function") return;
    let crossing: Animation | undefined;
    const cross = () => {
      crossing = sheen.animate(
        [
          { transform: "translateX(0) skewX(-18deg)" },
          { transform: "translateX(560%) skewX(-18deg)" },
        ],
        { duration: SHEEN_SWEEP_MS, easing: "ease-in-out" },
      );
    };
    let every: number | undefined;
    const first = window.setTimeout(() => {
      cross();
      every = window.setInterval(cross, SHEEN_CYCLE_MS);
    }, SHEEN_CYCLE_MS - SHEEN_SWEEP_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(every);
      crossing?.cancel();
    };
  }, []);
  return (
    <span
      ref={ref}
      className="ambient-motion pointer-events-none absolute inset-y-0 -left-1/2 w-1/3 -skew-x-[18deg] bg-[linear-gradient(90deg,transparent,rgb(255_255_255/0.55),transparent)]"
    />
  );
}
