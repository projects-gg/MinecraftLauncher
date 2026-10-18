import { type CSSProperties, useState } from "react";
import { cn } from "../lib/cn";
import { useAmbientMotion } from "../perf";

/**
 * What a server's bars say: `pending` while the ping is out, `online` (lit by latency), `offline`
 * (unreachable: a small cross), `idle` (nothing being asked: dim bars).
 */
export type SignalState = "pending" | "online" | "offline" | "idle";

const BARS = [0, 1, 2, 3, 4] as const;

/** The cross over the short bars: 1 px squares, flat (a shadow smudged it at 1x). */
const CROSS_PATH = [
  [0, 0],
  [4, 0],
  [1, 1],
  [3, 1],
  [2, 2],
  [1, 3],
  [3, 3],
  [0, 4],
  [4, 4],
]
  .map(([x, y]) => `M${x} ${y}h1v1h-1z`)
  .join("");

/** How many bars Minecraft's server list lights for a latency (5 when it is not known). */
export function signalLevel(latencyMs: number | null | undefined): number {
  if (latencyMs == null || latencyMs < 150) return 5;
  if (latencyMs < 300) return 4;
  if (latencyMs < 600) return 3;
  if (latencyMs < 1000) return 2;
  return 1;
}

/**
 * Server reachability as the game's server-list connection bars, in gold: five pixel bars with a
 * one-pixel drop shadow, drawn on whole pixels (15 × 8). While the ping is out a highlight sweeps
 * back and forth (the game's "pinging" frames, 100 ms each, stopped after 12 s); an answer that
 * arrives on screen fills the bars from the left once. Both run only while ambient motion may
 * (not hidden, in the tray, in game, in Performance mode or under reduced motion); otherwise the
 * final picture shows at once. Decorative: the words beside it carry the state.
 */
export function SignalBars({
  state,
  latencyMs,
  className,
}: {
  state: SignalState;
  latencyMs?: number | null;
  className?: string;
}) {
  const motion = useAmbientMotion();
  const level = state === "online" ? signalLevel(latencyMs) : 0;
  // The fill plays when the answer comes in while the bars are on screen, never on mount.
  const [shown, setShown] = useState(state);
  const [fill, setFill] = useState(false);
  if (shown !== state) {
    setShown(state);
    setFill(state === "online" && motion);
  }
  return (
    <svg
      aria-hidden
      focusable="false"
      width={15}
      height={8}
      viewBox="0 0 15 8"
      shapeRendering="crispEdges"
      data-state={state}
      data-sweep={state === "pending" && motion ? "" : undefined}
      data-fill={fill ? "" : undefined}
      className={cn("signal-bars shrink-0", className)}
    >
      {BARS.map((i) => (
        <g
          key={i}
          className="signal-bar"
          data-on={i < level ? "" : undefined}
          style={{ "--i": i } as CSSProperties}
        >
          <rect className="signal-shade" x={3 * i + 1} y={5 - i} width={2} height={3 + i} />
          <rect className="signal-face" x={3 * i} y={4 - i} width={2} height={3 + i} />
        </g>
      ))}
      {state === "offline" ? <path className="signal-cross" d={CROSS_PATH} /> : null}
    </svg>
  );
}
