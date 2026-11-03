// The microphone's meter (VOICE-CALL; the owner: Steam's lines whose green part moves with the
// voice): a row of thin bars that light from the left as the player speaks, jade while what the
// microphone picks up goes out, bone while it is heard but not sent (under the sensitivity, alone in
// the room, push-to-talk up). With a hand-set sensitivity in "Konuşunca" the bars under it are
// short, so the step where they grow is the line to speak past. One component everywhere: five bars
// beside the microphone in the title bar's pill and the room panel, a long one in Settings (the mic
// test, and the sensitivity drawn on it).
//
// Motion: each bar's opacity only, on or off at once like a meter's segments (nothing moves), written
// straight to the element as levels come (no render); at rest nothing changes. Reduced motion and
// Performance mode: no levels at all, every bar fades in together while the microphone sends and
// out after.
import { cn, useMotionPrefs } from "@pl/ui";
import { useEffect, useRef } from "react";
import { voiceApi } from "@/lib/api/voice-api";
import { MIC, usePainter } from "./levels";
import { useVoiceUi } from "./store";

/** The part of the 0–1 scale a meter spans: a short meter shows speech, a long one all of it. */
export interface MeterRange {
  from: number;
  to: number;
}

/** Speech, about -42 to -6 dBFS: a room's quiet lights nothing, a clear voice most of it. */
export const SPEECH_RANGE: MeterRange = { from: 0.3, to: 0.9 };
/** The whole scale (the settings, where the sensitivity is drawn on it). */
export const FULL_RANGE: MeterRange = { from: 0, to: 1 };

/** How many bars a level lights (0–`bars`). */
export function litBars(level: number, bars: number, range: MeterRange): number {
  const fill = (level - range.from) / (range.to - range.from);
  if (!(fill > 0)) return 0;
  return Math.min(bars, Math.ceil(fill * bars - 0.0001));
}

/**
 * How many bars sit under a hand-set sensitivity (`threshold`, 0–1 on the same scale): their
 * middle is below it. None without one.
 */
export function shortBars(threshold: number | null, bars: number, range: MeterRange): number {
  if (threshold === null) return 0;
  let short = 0;
  for (let i = 0; i < bars; i++) {
    const middle = range.from + ((i + 0.5) / bars) * (range.to - range.from);
    if (middle < threshold) short += 1;
  }
  return short;
}

/** An unlit bar's strength, and a lit one's. */
export const UNLIT = 0.2;
const LIT = 1;

export function VoiceMeter({
  bars = 5,
  source = "call",
  threshold = null,
  range,
  className,
  barClassName,
}: {
  bars?: number;
  /** The call's microphone (`voice-levels`), or the settings page's (`voice-level`). */
  source?: "call" | "settings";
  /** A hand-set sensitivity, 0–1 (null: automatic, or push-to-talk). */
  threshold?: number | null;
  /** Speech for a short meter, the whole scale for a long one. */
  range?: MeterRange;
  className?: string;
  /** The bars' size ("h-[10px] w-[2px]" by default). */
  barClassName?: string;
}) {
  const { reduce } = useMotionPrefs();
  const span = range ?? (bars > 8 ? FULL_RANGE : SPEECH_RANGE);
  const row = useRef<HTMLSpanElement>(null);
  const lit = useRef(0);
  const short = shortBars(threshold, bars, span);

  const paint = (level: number, open: boolean) => {
    const element = row.current;
    if (!element) return;
    const sending = open && level > 0;
    element.toggleAttribute("data-open", sending);
    const count = reduce ? (sending ? bars : 0) : litBars(level, bars, span);
    if (count === lit.current) return;
    lit.current = count;
    const children = element.children;
    for (let i = 0; i < children.length; i++) {
      (children[i] as HTMLElement).style.opacity = String(i < count ? LIT : UNLIT);
    }
  };
  const painter = useRef(paint);
  painter.current = paint;

  // The call's microphone: its live level while the meters run.
  const live = source === "call" && !reduce;
  usePainter(live ? MIC : null, (level, open) => painter.current(level, open));
  // Reduced motion: only whether it sends (the speaking frames), all bars at once.
  const talking = useVoiceUi((s) => s.meTalking);
  useEffect(() => {
    if (source === "call" && reduce) painter.current(talking ? 1 : 0, talking);
  }, [source, reduce, talking]);
  // The settings page's level (Settings → Sesli sohbet, while it is open).
  useEffect(() => {
    if (source !== "settings") return;
    return voiceApi.onLevel(({ level, open }) => painter.current(level, open));
  }, [source]);

  return (
    <span
      ref={row}
      aria-hidden
      data-voice-meter=""
      className={cn(
        "inline-flex shrink-0 items-center gap-[2px] text-bone-100/70 data-[open]:text-jade-400",
        className,
      )}
    >
      {Array.from({ length: bars }, (_, i) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: a bar is its place, they never move
          key={i}
          style={{ opacity: UNLIT, transform: i < short ? "scaleY(0.5)" : undefined }}
          className={cn(
            "block h-[10px] w-[2px] rounded-[1px] bg-current",
            // Live, a bar is on or off at once, as a level meter's segments are (and one paint per
            // level, not a fade's frames); reduced motion: all of them fade together.
            // A short meter's bars are tiny layers of their own: a level only changes the
            // compositor's opacity, nothing repaints (the title bar's pill is glass).
            reduce
              ? "transition-[opacity,transform] duration-150"
              : cn("transition-transform duration-150", bars <= 8 && "will-change-[opacity]"),
            barClassName,
          )}
        />
      ))}
    </span>
  );
}
