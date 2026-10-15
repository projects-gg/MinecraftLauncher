import { useReducedMotion } from "motion/react";
import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";

interface PerfState {
  /** User-chosen Performance mode: no ambient motion, no blur, no video. */
  performanceMode: boolean;
  /** Ambient motion is additionally paused while the game runs or the window is hidden. */
  ambientPaused: boolean;
}

const PerfContext = createContext<PerfState>({ performanceMode: false, ambientPaused: false });

export function PerfProvider({
  performanceMode,
  ambientPaused = false,
  children,
}: {
  performanceMode: boolean;
  ambientPaused?: boolean;
  children: ReactNode;
}) {
  const [hidden, setHidden] = useState(() =>
    typeof document === "undefined" ? false : document.hidden,
  );

  useEffect(() => {
    const onChange = () => setHidden(document.hidden);
    document.addEventListener("visibilitychange", onChange);
    return () => document.removeEventListener("visibilitychange", onChange);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.perf = performanceMode ? "on" : "off";
  }, [performanceMode]);

  const value = useMemo(
    () => ({ performanceMode, ambientPaused: ambientPaused || hidden }),
    [performanceMode, ambientPaused, hidden],
  );
  return <PerfContext.Provider value={value}>{children}</PerfContext.Provider>;
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
