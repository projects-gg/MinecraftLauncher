import { MotionConfig, useReducedMotion } from "motion/react";
import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";

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

export function PerfProvider({
  performanceMode,
  ambientPaused = false,
  unseen = false,
  children,
}: {
  performanceMode: boolean;
  ambientPaused?: boolean;
  /** The window is out of sight (the page being hidden is added here). */
  unseen?: boolean;
  children: ReactNode;
}) {
  const [hidden, setHidden] = useState(() =>
    typeof document === "undefined" ? false : document.hidden,
  );
  const [nonce] = useState(styleNonce);

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
  // `reducedMotion="user"`: under the OS setting, transform and layout animations that a
  // component did not already calm down itself jump to their end, while fades still play.
  return (
    <PerfContext.Provider value={value}>
      <MotionConfig nonce={nonce} reducedMotion="user">
        {children}
      </MotionConfig>
    </PerfContext.Provider>
  );
}

export function usePerf() {
  return useContext(PerfContext);
}

/** True when decorative, user-independent motion (embers, Ken Burns, sheen) may run. */
export function useAmbientMotion() {
  const reduce = useReducedMotion();
  const { performanceMode, ambientPaused } = usePerf();
  return !reduce && !performanceMode && !ambientPaused;
}

/** True when transitions that answer user actions may animate. */
export function useInteractionMotion() {
  return !useReducedMotion();
}
