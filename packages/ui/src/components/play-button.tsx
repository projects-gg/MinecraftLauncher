import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, type RefObject, useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { easeOutExpo } from "../motion";
import { useInteractionMotion, usePerf } from "../perf";
import { usePressGuard } from "../press-guard";

export type PlayState =
  | { kind: "ready" }
  | { kind: "update" }
  | { kind: "working"; percent: number; label: string }
  /** The files are ready and the game is starting: its window is not on screen yet. */
  | { kind: "opening"; label: string }
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
  /** Stops the launch: while files are prepared and while the game opens. */
  onCancel?: () => void;
  labels: PlayButtonLabels;
  className?: string;
  /** `aria-keyshortcuts` of the app's shortcut that presses the button (the launcher's Ctrl+Enter). */
  keyShortcuts?: string;
}

/**
 * The button's accessible name says what a press does. While a launch is prepared or the game
 * opens the button shows that, and once a press would cancel it the name leads with that
 * ("İptal, Oyun dosyaları indiriliyor %42", "İptal, Açılıyor"); in the moment after OYNA, when a
 * press does nothing, it is the state alone.
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
    case "opening":
      return cancelReady ? `${labels.cancel}, ${state.label}` : state.label;
    case "running":
      return labels.running;
    case "unavailable":
      return state.label;
  }
}

/** What a press on OYNA does now. */
export type PlayPress = "play" | "cancel" | "showCancel" | "nothing";

/**
 * What a press on OYNA does (tests: apps/launcher/src/shell/play-button.test.ts); "play" and
 * "cancel" only if the press guard lets the press through (`usePressGuard`: never the rest of
 * the double click that started or cancelled a launch). `armed`: the guard takes presses now.
 * `cancelShown`: "İptal" may show under the pointer. A pointer press on a launch whose "İptal"
 * is not shown yet (the pointer rested where it pressed OYNA, so the button says "%42" or
 * "Açılıyor") shows it instead of cancelling: a press never does what the button did not say.
 * The keyboard, whose name for the button already leads with "İptal", cancels.
 */
export function playPress(
  kind: PlayState["kind"],
  press: { cancellable: boolean; armed: boolean; cancelShown: boolean; pointer: boolean },
): PlayPress {
  if (kind === "ready" || kind === "update") return "play";
  if ((kind !== "working" && kind !== "opening") || !press.cancellable) return "nothing";
  return press.pointer && press.armed && !press.cancelShown ? "showCancel" : "cancel";
}

/**
 * How the faces and the fill change (the label cross-fades on its own, quicker). A gentle ease-out:
 * with `easeOutExpo` the gold was gone in 70 ms and "Açılıyor" → "Oyunda" read as a cut.
 */
const FACE_S = 0.34;
const FACE_EASE = [0.33, 1, 0.68, 1] as const;

/**
 * The one loud element: a gold, chamfered PLAY button on a darker socket, with a soft glow and a
 * sheen that crosses the gold once when the pointer (or the keyboard's focus) arrives. While the
 * files are prepared a gold fill shows how far; once they are ready the fill is full and, while
 * the game opens, a light crosses it and its glow swells every 2.4 s ("Açılıyor"); when the game's
 * window is on screen the gold gives way to the dark "Oyunda" face with one jade bloom. The faces
 * are stacked layers that cross-fade (a gradient cannot transition). Effects respect reduced
 * motion and Performance mode.
 */
export function PlayButton({
  state,
  onPlay,
  onCancel,
  labels,
  className,
  keyShortcuts,
}: PlayButtonProps) {
  const animate = useInteractionMotion();
  const { performanceMode, unseen } = usePerf();
  const [hover, setHover] = useState(false);

  const isGold = state.kind === "ready" || state.kind === "update";
  const isWorking = state.kind === "working";
  const isOpening = state.kind === "opening";
  const filled = isWorking || isOpening;
  const cancellable = filled && Boolean(onCancel);
  const disabled =
    state.kind === "running" || state.kind === "unavailable" || (isOpening && !onCancel);
  // "İptal" takes OYNA's place after a press, and OYNA takes "İptal"'s after a cancel: neither
  // takes the rest of that double click, then each does once the pointer has left the button or
  // 0.8 s later, also under a resting pointer (E2E2: the player who did not move could never
  // cancel).
  const guard = usePressGuard(isGold ? "play" : filled ? "cancel" : "rest");
  // A pointer resting where it pressed OYNA keeps seeing the progress ("%42", "Açılıyor" with its
  // light): "İptal" shows once it moves over the button, comes back to it or presses it.
  const [cancelShownFor, setCancelShownFor] = useState<number | null>(null);
  const cancelShown = guard.since === null || cancelShownFor === guard.since;
  const showCancelLabel = () => {
    if (guard.armed && !cancelShown) setCancelShownFor(guard.since);
  };
  const showCancel = hover && cancellable && guard.armed && cancelShown;

  // The swell behind the button and the light across it while the game opens.
  const swell = useRef<HTMLDivElement>(null);
  const sweep = useRef<HTMLSpanElement>(null);
  const wash = useRef<HTMLSpanElement>(null);
  useOpeningCycle(
    isOpening && !showCancel && !unseen,
    { sweep, wash, swell },
    animate && !performanceMode,
  );
  // The sheen across the gold when the pointer or the keyboard's focus arrives.
  const sheen = useRef<HTMLSpanElement>(null);
  const crossing = useRef<Animation | null>(null);
  const crossSheen = () => {
    const band = sheen.current;
    if (!isGold || !animate || performanceMode || !band || typeof band.animate !== "function") {
      return;
    }
    // A crossing under way finishes: the band never jumps back to start again.
    const under = crossing.current;
    if (under && (under.pending || under.playState === "running")) return;
    crossing.current = band.animate(SHEEN_FRAMES, {
      duration: SHEEN_SWEEP_MS,
      easing: SHEEN_EASE,
    });
  };
  // One jade bloom when the game's window shows (not when the page loads with a game on).
  const bloom = useRef<HTMLDivElement>(null);
  const previous = useRef(state.kind);
  useEffect(() => {
    const was = previous.current;
    previous.current = state.kind;
    const element = bloom.current;
    if (state.kind !== "running" || was !== "opening" || !element || unseen) return;
    if (typeof element.animate !== "function") return;
    element.animate([{ opacity: 0 }, { opacity: 1, offset: 0.18 }, { opacity: 0 }], {
      duration: 1100,
      easing: "ease-out",
    });
  }, [state.kind, unseen]);

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
    case "opening":
      text = showCancel ? labels.cancel : state.label;
      key = showCancel ? "cancel" : "opening";
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

  const handleClick = (event: { detail: number }) => {
    const does = playPress(state.kind, {
      cancellable,
      armed: guard.armed,
      cancelShown,
      pointer: event.detail > 0,
    });
    if (does === "showCancel") {
      showCancelLabel();
      return;
    }
    if (does === "nothing" || !guard.take(event)) return;
    if (does === "play") onPlay();
    else onCancel?.();
  };

  const face = { duration: animate ? FACE_S : 0.14, ease: FACE_EASE };
  // Over a cancel the fill steps back, so "İptal" reads on the dark face.
  const fillOpacity = filled ? (showCancel ? 0.16 : 1) : 0;
  // The fill never runs backwards: leaving (the game's window showed, a cancel) it fades where it
  // is, and a new launch starts it from nothing at once, while it is still invisible.
  const lastFill = useRef(0);
  const wasFilled = useRef(filled);
  const fillPercent = isOpening
    ? 100
    : isWorking
      ? Math.min(100, Math.max(0, state.percent))
      : lastFill.current;
  const fillJumps = filled && !wasFilled.current;
  useEffect(() => {
    if (filled) lastFill.current = fillPercent;
    wasFilled.current = filled;
  });

  return (
    <div className={cn("relative isolate h-[48px] w-[204px]", className)}>
      {/* Glow: not clipped, so it can bloom past the chamfered edges. */}
      <motion.div
        aria-hidden
        className="pointer-events-none absolute -inset-x-5 -inset-y-3 -z-20 rounded-[50%] bg-[radial-gradient(closest-side,rgb(240_184_73/0.55),rgb(232_154_44/0.18)_55%,transparent)] blur-xl"
        initial={false}
        animate={{
          opacity: isGold
            ? hover
              ? 1
              : 0.7
            : isOpening && !showCancel
              ? 0.42
              : isWorking && !showCancel
                ? 0.35
                : 0,
        }}
        transition={{ duration: 0.3 }}
      />
      {/* The opening swell (its opacity is only ever animated) and the "Oyunda" bloom. */}
      <div
        ref={swell}
        aria-hidden
        className="pointer-events-none absolute -inset-x-7 -inset-y-4 -z-20 rounded-[50%] bg-[radial-gradient(closest-side,rgb(246_213_142/0.6),rgb(232_154_44/0.22)_55%,transparent)] opacity-0 blur-xl"
      />
      <div
        ref={bloom}
        aria-hidden
        className="pointer-events-none absolute -inset-x-8 -inset-y-5 -z-20 rounded-[50%] bg-[radial-gradient(closest-side,rgb(95_203_142/0.62),rgb(95_203_142/0.22)_55%,transparent)] opacity-0 blur-xl"
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
        onKeyDown={guard.handlers.onKeyDown}
        onPointerEnter={() => {
          setHover(true);
          showCancelLabel();
          crossSheen();
        }}
        onFocus={(event) => {
          // The keyboard's focus only: a click focuses the button too, after the pointer came.
          if (event.currentTarget.matches(":focus-visible")) crossSheen();
        }}
        onPointerMove={showCancelLabel}
        onPointerLeave={(event) => {
          setHover(false);
          guard.handlers.onPointerLeave(event);
          // Back over the button, the pointer comes for "İptal".
          if (guard.since !== null) setCancelShownFor(guard.since);
        }}
        aria-label={playButtonName(state, labels, guard.armed && Boolean(onCancel))}
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
            "chamfer absolute inset-0 overflow-hidden bg-obsidian-800 [--cut:12px]",
            state.kind === "unavailable" && "opacity-80",
          )}
        >
          {/* "Oyunda": the dark face, a little lighter at the top. */}
          <motion.span
            className="absolute inset-0 bg-[linear-gradient(180deg,var(--color-obsidian-700),var(--color-obsidian-800))]"
            initial={false}
            animate={{ opacity: state.kind === "running" ? 1 : 0 }}
            transition={face}
          />
          {/* Ready: the gold face. */}
          <motion.span
            className="absolute inset-0 bg-[linear-gradient(180deg,var(--color-gold-200),var(--color-gold-400)_38%,var(--color-gold-500)_62%,var(--color-gold-600))] transition-[filter] duration-300 group-hover:brightness-[1.07]"
            initial={false}
            animate={{ opacity: isGold ? 1 : 0 }}
            transition={face}
          />
          {/* The fill slides in from the left as files arrive (transform only, no layout work per
              step) and is full while the game opens. */}
          <motion.span
            className="absolute inset-0 overflow-hidden bg-[linear-gradient(180deg,var(--color-gold-400),var(--color-gold-600))]"
            initial={false}
            animate={{ transform: `translateX(${fillPercent - 100}%)`, opacity: fillOpacity }}
            transition={{
              transform: { duration: fillJumps ? 0 : 0.35, ease: easeOutExpo },
              opacity: face,
            }}
          >
            {/* The opening light rests off the left edge; its crossings animate this transform
                (one property, so nothing adds a second offset). */}
            <span
              ref={sweep}
              className="pointer-events-none absolute inset-y-0 left-0 w-[32%] bg-[linear-gradient(90deg,transparent,rgb(255_246_222/0.38),transparent)]"
              style={{ transform: "translateX(-130%) skewX(-18deg)" }}
            />
            {/* Without movement (reduced motion, Performance mode) the gold brightens a little
                instead; its opacity is only ever animated. */}
            <span
              ref={wash}
              className="pointer-events-none absolute inset-0 bg-[linear-gradient(180deg,rgb(255_246_222/0.24),rgb(255_246_222/0.08))] opacity-0"
            />
          </motion.span>
          {/* Inner bevel */}
          <span className="chamfer absolute inset-[2px] [--cut:11px] shadow-[inset_0_1px_0_rgb(255_255_255/0.55),inset_0_-2px_0_rgb(63_10_10/0.35)]" />
          {/* The sheen. Leaving the gold face (OYNA pressed) fades a crossing under way with it. */}
          <motion.span
            aria-hidden
            className="pointer-events-none absolute inset-0"
            initial={false}
            animate={{ opacity: isGold ? 1 : 0 }}
            transition={face}
          >
            <span
              ref={sheen}
              className="absolute inset-y-0 -left-1/2 w-1/3 -skew-x-[18deg] bg-[linear-gradient(90deg,transparent,rgb(255_255_255/0.55),transparent)]"
            />
          </motion.span>
        </span>
        <span className="relative z-10 flex size-full flex-col items-center justify-center">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span
              key={key}
              // The old word leaves quickly and the new one comes a beat later, so two words never
              // blur into each other. Reduced motion keeps the cross-fade, without the rise.
              initial={{ opacity: 0, y: animate ? 8 : 0 }}
              animate={{
                opacity: 1,
                y: 0,
                transition: { duration: animate ? 0.22 : 0.14, ease: easeOutExpo, delay: 0.05 },
              }}
              exit={{
                opacity: 0,
                y: animate ? -6 : 0,
                transition: { duration: 0.1, ease: easeOutExpo },
              }}
              className={cn(
                "font-display leading-none font-bold tracking-[0.08em] uppercase",
                isGold && "text-[23px] text-gold-ink",
                !isGold && "text-[19px]",
                key === "cancel" && "text-flare-400",
                key === "percent" && "text-bone-50 tabular-nums",
                key === "opening" && "text-gold-ink",
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

/** One opening cycle: a light crosses the button and its glow swells, then nothing runs. */
const OPENING_CYCLE_MS = 2400;
const OPENING_SWEEP_MS = 1300;
/** The fill lands and the label settles before the first crossing. */
const OPENING_FIRST_MS = 450;

/**
 * While the game opens: every 2.4 s a soft light crosses the gold (left to right, as the fill
 * came) and the glow behind swells with it, each a short animation of its own with nothing
 * running in between (OPT1: an endless animation keeps WebView2 drawing every frame). Under
 * reduced motion and in Performance mode nothing moves: the gold brightens a little and the glow
 * swells (opacity changes). Nothing runs while the window is out of sight; a crossing under way
 * when the game's window shows simply ends with the fading fill.
 */
function useOpeningCycle(
  active: boolean,
  layers: {
    sweep: RefObject<HTMLSpanElement | null>;
    wash: RefObject<HTMLSpanElement | null>;
    swell: RefObject<HTMLDivElement | null>;
  },
  moving: boolean,
) {
  const { sweep, wash, swell } = layers;
  useEffect(() => {
    const glow = swell.current;
    if (!active || !glow || typeof glow.animate !== "function") return;
    const band = sweep.current;
    const brighten = wash.current;
    const timing = { duration: OPENING_SWEEP_MS, easing: "ease-in-out" };
    const cross = () => {
      if (moving && band) {
        band.animate(
          [
            { transform: "translateX(-130%) skewX(-18deg)" },
            { transform: "translateX(360%) skewX(-18deg)" },
          ],
          { duration: OPENING_SWEEP_MS, easing: "cubic-bezier(0.45, 0, 0.25, 1)" },
        );
      } else if (brighten) {
        brighten.animate([{ opacity: 0 }, { opacity: 1 }, { opacity: 0 }], timing);
      }
      glow.animate([{ opacity: 0 }, { opacity: 1 }, { opacity: 0 }], timing);
    };
    let every: number | undefined;
    const first = window.setTimeout(() => {
      cross();
      every = window.setInterval(cross, OPENING_CYCLE_MS);
    }, OPENING_FIRST_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(every);
    };
  }, [active, moving, sweep, wash, swell]);
}

/**
 * The sheen's one crossing when the pointer or the keyboard's focus arrives on the gold face (the
 * owner, MOTION1: it crossed every 7 s while somebody used the window, OPT2). The band starts just
 * off the left edge, so it shows at once, and leaves the right edge before the curve slows down.
 * Nothing runs between crossings, nothing at all under reduced motion or in Performance mode.
 */
const SHEEN_SWEEP_MS = 900;
const SHEEN_EASE = "cubic-bezier(0.4, 0, 0.2, 1)";
const SHEEN_FRAMES: Keyframe[] = [
  { transform: "translateX(40%) skewX(-18deg)" },
  { transform: "translateX(560%) skewX(-18deg)" },
];
