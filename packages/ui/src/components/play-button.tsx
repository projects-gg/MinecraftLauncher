import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
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

interface PlayButtonProps {
  state: PlayState;
  onPlay: () => void;
  onCancel?: () => void;
  /** Visible labels (localized). */
  labels: { play: string; update: string; cancel: string; running: string };
  className?: string;
}

/**
 * The one loud element: a gold, chamfered PLAY button on a darker socket, with a soft glow
 * and a periodic sheen when ready. Ambient effects respect reduced motion and Performance mode.
 */
export function PlayButton({ state, onPlay, onCancel, labels, className }: PlayButtonProps) {
  const ambient = useAmbientMotion();
  const animate = useInteractionMotion();
  const [hover, setHover] = useState(false);

  const isGold = state.kind === "ready" || state.kind === "update";
  const isWorking = state.kind === "working";
  const disabled = state.kind === "launching" || state.kind === "running" || state.kind === "unavailable";

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
      // Turkish puts the percent sign first; Tektur's % glyph is ornate, so it is set in Figtree.
      text =
        hover && onCancel ? (
          labels.cancel
        ) : (
          <>
            <span className="mr-0.5 font-sans text-[19px] font-bold">%</span>
            {Math.floor(state.percent)}
          </>
        );
      key = hover && onCancel ? "cancel" : "percent";
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
    if (isWorking) onCancel?.();
    else if (isGold) onPlay();
  };

  return (
    <div className={cn("relative isolate h-16 w-[236px]", className)}>
      {/* Glow: not clipped, so it can bloom past the chamfered edges. */}
      <motion.div
        aria-hidden
        className="pointer-events-none absolute -inset-x-6 -inset-y-4 -z-20 rounded-[50%] bg-[radial-gradient(closest-side,rgb(240_184_73/0.55),rgb(232_154_44/0.18)_55%,transparent)] blur-xl"
        initial={false}
        animate={{ opacity: isGold ? (hover ? 1 : 0.7) : isWorking ? 0.35 : 0 }}
        transition={{ duration: 0.3 }}
      />
      {/* Socket plate */}
      <div
        aria-hidden
        className="chamfer absolute -inset-[5px] -z-10 bg-[linear-gradient(180deg,rgb(63_10_10/0.9),rgb(10_9_12/0.95))] [--cut:17px]"
      />
      <button
        type="button"
        disabled={disabled}
        onClick={handleClick}
        onPointerEnter={() => setHover(true)}
        onPointerLeave={() => setHover(false)}
        aria-label={
          typeof text === "string"
            ? text
            : isWorking
              ? `${state.label} %${Math.floor(state.percent)}`
              : undefined
        }
        className={cn(
          "group relative block size-full cursor-pointer select-none focus-visible:outline-none",
          "transition-transform duration-150 enabled:active:scale-[0.98] enabled:hover:scale-[1.015]",
          "disabled:cursor-default",
        )}
      >
        <span
          aria-hidden
          className="chamfer absolute -inset-[4px] bg-gold-200 opacity-0 transition-opacity [--cut:16px] group-focus-visible:opacity-100"
        />
        <span
          aria-hidden
          className={cn(
            "chamfer absolute inset-0 overflow-hidden [--cut:14px] transition-[background,filter] duration-300",
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
          <span className="chamfer absolute inset-[2px] [--cut:13px] shadow-[inset_0_1px_0_rgb(255_255_255/0.55),inset_0_-2px_0_rgb(63_10_10/0.35)]" />
          {isWorking ? (
            <motion.span
              className="absolute inset-y-0 left-0 bg-[linear-gradient(180deg,var(--color-gold-400),var(--color-gold-600))]"
              initial={false}
              animate={{ width: `${Math.min(100, Math.max(0, state.percent))}%` }}
              transition={{ duration: 0.35, ease: easeOutExpo }}
            />
          ) : null}
          {isGold && ambient ? (
            <span className="ambient-motion pointer-events-none absolute inset-y-0 -left-1/2 w-1/3 -skew-x-[18deg] bg-[linear-gradient(90deg,transparent,rgb(255_255_255/0.55),transparent)] animate-[play-sheen_7s_ease-in-out_infinite]" />
          ) : null}
        </span>
        <span className="relative z-10 flex size-full flex-col items-center justify-center">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span
              key={key}
              initial={animate ? { opacity: 0, y: 8 } : false}
              animate={{ opacity: 1, y: 0 }}
              exit={animate ? { opacity: 0, y: -6 } : undefined}
              transition={{ duration: 0.22, ease: easeOutExpo }}
              className={cn(
                "font-display leading-none font-bold tracking-[0.08em] uppercase",
                isGold && "text-[26px] text-gold-ink",
                state.kind === "launching" && "text-[18px] tracking-[0.06em] text-gold-ink",
                !isGold && state.kind !== "launching" && "text-[22px]",
                isWorking && (key === "cancel" ? "text-flare-400" : "text-bone-50 tabular-nums"),
                state.kind === "running" && "text-jade-400",
                state.kind === "unavailable" && "text-[16px] text-ash-400",
              )}
            >
              {text}
            </motion.span>
          </AnimatePresence>
        </span>
      </button>
      <style>{`@keyframes play-sheen { 0%, 72% { transform: translateX(0) skewX(-18deg); } 100% { transform: translateX(560%) skewX(-18deg); } }`}</style>
    </div>
  );
}
