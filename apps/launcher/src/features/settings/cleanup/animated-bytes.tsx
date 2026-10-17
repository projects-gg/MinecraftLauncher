import { usePerf } from "@pl/ui";
import { useReducedMotion } from "motion/react";
import { useEffect, useRef } from "react";
import { formatBytes } from "@/lib/format";

/**
 * A byte count that glides to its new value (ease-out, ~0.45 s, picks up from wherever it is
 * when the value changes again). The text is written straight to the DOM, so the card around it
 * never re-renders per frame. Reduced motion and Performance mode show the value at once. Screen
 * readers get the final value only.
 */
export function AnimatedBytes({
  value,
  from,
  className,
  duration = 450,
}: {
  value: number;
  /** Count up from this on mount (else the first value shows as is). */
  from?: number;
  className?: string;
  duration?: number;
}) {
  const reduce = useReducedMotion() ?? false;
  const { performanceMode } = usePerf();
  const ref = useRef<HTMLSpanElement>(null);
  const shown = useRef(from ?? value);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const origin = shown.current;
    if (reduce || performanceMode || origin === value) {
      shown.current = value;
      element.textContent = formatBytes(value);
      return;
    }
    let frame = 0;
    const start = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / duration);
      const eased = 1 - (1 - t) ** 3;
      shown.current = origin + (value - origin) * eased;
      element.textContent = formatBytes(Math.max(0, Math.round(shown.current)));
      if (t < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [value, duration, reduce, performanceMode]);

  return (
    <span className={className}>
      <span ref={ref} aria-hidden>
        {formatBytes(Math.round(shown.current))}
      </span>
      <span className="sr-only">{formatBytes(value)}</span>
    </span>
  );
}
