import { MotionConfig } from "motion/react";
import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { useReduceMotion } from "./reduced-motion";

/**
 * The page's CSP nonce (Tauri puts one on its own styles). Motion injects a style sheet for
 * AnimatePresence's `popLayout`; under the app's CSP an injected sheet without the nonce is
 * blocked and leaving elements stay in the flow, so motion gets the nonce.
 */
function styleNonce(): string | undefined {
  if (typeof document === "undefined") return undefined;
  const element = document.querySelector<HTMLElement>("style[nonce], script[nonce]");
  return element?.nonce || undefined;
}

interface PerfState {
  /** User-chosen Performance mode: no ambient motion, no blur, no video. */
  performanceMode: boolean;
  /** Ambient motion is additionally paused while the game runs or the window is hidden. */
  ambientPaused: boolean;
  /**
   * Nobody can see the page (the window is in the tray or minimized, or the page is hidden):
   * live readouts such as a countdown stop ticking. Every change on screen keeps WebView2
   * drawing frames for about a second, so a once-a-second tick nobody saw cost ~1.5 % of a CPU
   * core in the tray (OPT1).
   */
  unseen: boolean;
}

const PerfContext = createContext<PerfState>({
  performanceMode: false,
  ambientPaused: false,
  unseen: false,
});

/**
 * Nobody is using the window right now: no input for a few seconds, or another window took the
 * focus. Its own context: it changes whenever the player stops or starts moving, and only the
 * ambient motion reads it (the many `usePerf` readers do not render again for it).
 */
const RestingContext = createContext(false);

export function PerfProvider({
  performanceMode,
  ambientPaused = false,
  resting = false,
  unseen = false,
  children,
}: {
  performanceMode: boolean;
  ambientPaused?: boolean;
  /**
   * Nobody is using the window right now (no input for a while, another window in front).
   * Ambient motion winds down slowly then (the embers die down, the art's drift holds) and comes
   * back with the next input. Every frame on screen costs WebView2 a composite of the whole
   * scene, so motion that ran for as long as the window was visible cost 89 % of a CPU core at
   * 144 Hz with nobody looking (OPT2).
   */
  resting?: boolean;
  /** The window is out of sight (the page being hidden is added here). */
  unseen?: boolean;
  children: ReactNode;
}) {
  const [hidden, setHidden] = useState(() =>
    typeof document === "undefined" ? false : document.hidden,
  );
  const [nonce] = useState(styleNonce);
  const reduce = useReduceMotion();

  useEffect(() => {
    const onChange = () => setHidden(document.hidden);
    document.addEventListener("visibilitychange", onChange);
    return () => document.removeEventListener("visibilitychange", onChange);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.perf = performanceMode ? "on" : "off";
  }, [performanceMode]);

  const value = useMemo(
    () => ({
      performanceMode,
      ambientPaused: ambientPaused || hidden,
      unseen: unseen || hidden,
    }),
    [performanceMode, ambientPaused, unseen, hidden],
  );
  // While motion is reduced (the system's setting or the app's own, reduced-motion.ts), transform
  // and layout animations that a component did not already calm down itself jump to their end,
  // while fades still play. "always"/"never", not "user": "user" reads the system alone.
  return (
    <PerfContext.Provider value={value}>
      <RestingContext.Provider value={resting}>
        <MotionConfig nonce={nonce} reducedMotion={reduce ? "always" : "never"}>
          {children}
        </MotionConfig>
      </RestingContext.Provider>
    </PerfContext.Provider>
  );
}

export function usePerf() {
  return useContext(PerfContext);
}

/**
 * motion's config for a layer that sets its own (a sheet, a dialog, a picker): reduced motion as
 * the app answers it (reduced-motion.ts), never motion's "user", which reads the system's setting
 * alone and so kept sliding under "Animasyonları azalt" and Performance mode (BETA-FB).
 */
export function ReducedMotionConfig({ children }: { children: ReactNode }) {
  const reduce = useReduceMotion();
  return <MotionConfig reducedMotion={reduce ? "always" : "never"}>{children}</MotionConfig>;
}

/** True while nobody uses the window (see `PerfProvider`'s `resting`). */
export function useResting() {
  return useContext(RestingContext);
}

/**
 * True when decorative, user-independent motion (embers, the art's drift) may run: not under
 * reduced motion or in Performance mode, the window in sight, no game, and somebody using the
 * window. OYNA's sheen is not ambient any more: it crosses when the pointer arrives (MOTION1).
 */
export function useAmbientMotion() {
  const reduce = useReduceMotion();
  const { performanceMode, ambientPaused } = usePerf();
  const resting = useResting();
  return !reduce && !performanceMode && !ambientPaused && !resting;
}

/**
 * True when motion that follows something happening (a ping under way, an answer arriving, the
 * pointer) may run: not under reduced motion or in Performance mode, the window in sight, no
 * game. Unlike ambient motion it does not wait for the player; what drives it also ends it.
 */
export function useLiveMotion() {
  const reduce = useReduceMotion();
  const { performanceMode, ambientPaused } = usePerf();
  return !reduce && !performanceMode && !ambientPaused;
}

/** True when transitions that answer user actions may animate. */
export function useInteractionMotion() {
  return !useReduceMotion();
}
