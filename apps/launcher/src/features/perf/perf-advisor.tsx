// The one-time look at this computer (suggest.ts), mounted by the backdrop; renders nothing.
// A very weak computer gets Performance mode as soon as its memory is known (the welcome flow
// already runs lighter) and hears about it on Home; a weak one is measured on Home and offered
// the mode only when the frames really are slow. Both wait for a calm moment: Home after its
// first-arrival motion, the window in sight, no game, not in do-not-disturb.
import { toast, usePerf, useResting } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { shouldShowOnboarding } from "@/features/onboarding/onboarding";
import { useSocialUi } from "@/features/social/store";
import { api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import {
  type FirstStep,
  firstStep,
  frameStats,
  framesLookSlow,
  hardwareTier,
  parsePerfPreview,
} from "./suggest";

/**
 * Home's first arrival settles within 900 ms (the boot moment) and a page change within 240 ms;
 * the margin covers a slow computer's first renders.
 */
const SETTLE_MS = 1500;
const MEASURE_MS = 3000;
/** Long enough to read and decide, like the cleanup reminder. */
const TOAST_MS = 15_000;
const TOAST_ID = "perf-advice";

// The browser preview never judges the computer it runs on: only `?perf=…` stands in for one,
// and every load starts undecided.
const preview = isTauri() ? null : parsePerfPreview(window.location.search);
if (preview) usePrefs.setState({ perfAdvice: "todo", perfTouched: false, performanceMode: false });

/**
 * Animation frame intervals over `ms`, or null when `signal` aborts first. `loadMs` of busy work
 * per frame stands in for a weak computer in the browser preview.
 */
function measureFrames(ms: number, signal: AbortSignal, loadMs = 0): Promise<number[] | null> {
  return new Promise((resolve) => {
    if (signal.aborted) {
      resolve(null);
      return;
    }
    const deltas: number[] = [];
    let start = -1;
    let last = 0;
    let frame = 0;
    const stop = () => {
      cancelAnimationFrame(frame);
      resolve(null);
    };
    const tick = (now: number) => {
      if (start < 0) start = now;
      else deltas.push(now - last);
      last = now;
      if (now - start >= ms) {
        signal.removeEventListener("abort", stop);
        resolve(deltas);
        return;
      }
      if (loadMs > 0) {
        const until = performance.now() + loadMs;
        while (performance.now() < until) {
          // Preview only: the stand-in work.
        }
      }
      frame = requestAnimationFrame(tick);
    };
    signal.addEventListener("abort", stop, { once: true });
    frame = requestAnimationFrame(tick);
  });
}

/** The launcher turned the mode on by itself: say so once, with the way back. */
function tellSwitched() {
  const prefs = usePrefs.getState();
  prefs.set("perfAdvice", "done");
  // Turned off meanwhile (a restored settings backup): nothing to tell.
  if (!prefs.performanceMode || prefs.perfTouched) return;
  toast(i18n.t("tuning.perf.switched"), {
    id: TOAST_ID,
    duration: TOAST_MS,
    action: {
      label: i18n.t("tuning.perf.undo"),
      onClick: () => usePrefs.getState().setPerformanceMode(false),
    },
  });
}

/** After a full measurement: the verdict is final, the offer comes only for slow frames. */
function offerAfter(deltas: number[]) {
  const prefs = usePrefs.getState();
  prefs.set("perfAdvice", "done");
  if (prefs.perfTouched || prefs.performanceMode || !framesLookSlow(frameStats(deltas))) return;
  toast(i18n.t("tuning.perf.suggest"), {
    id: TOAST_ID,
    duration: TOAST_MS,
    action: {
      label: i18n.t("tuning.perf.turnOn"),
      onClick: () => usePrefs.getState().setPerformanceMode(true),
    },
  });
}

export function PerfAdvisor({ home }: { home: boolean }) {
  const advice = usePrefs((s) => s.perfAdvice);
  const touched = usePrefs((s) => s.perfTouched);
  const performanceMode = usePrefs((s) => s.performanceMode);
  const onboarding = shouldShowOnboarding(usePrefs((s) => s.onboarded));
  const active = !touched && advice !== "done" && (isTauri() || preview !== null);
  const { data: info } = useQuery({
    queryKey: ["app-info"],
    queryFn: () => api.appInfo(),
    enabled: active && !preview,
    staleTime: Number.POSITIVE_INFINITY,
  });
  const [step, setStep] = useState<FirstStep | null>(null);

  // The machine, once per start while undecided (a weak one stays undecided until measured).
  useEffect(() => {
    if (!active || step || (!preview && !info)) return;
    const prefs = usePrefs.getState();
    if (prefs.perfAdvice !== "todo") return;
    const hardware = preview?.hardware ?? {
      memoryMb: info?.systemMemoryMb,
      cores: navigator.hardwareConcurrency,
    };
    const next = firstStep(hardwareTier(hardware), prefs.performanceMode);
    setStep(next);
    if (next === "switch") usePrefs.setState({ performanceMode: true, perfAdvice: "switched" });
    else if (next === "none") prefs.set("perfAdvice", "done");
  }, [active, step, info]);

  // `ambientPaused`: the window is in the tray, minimized or hidden, or the game runs. `resting`:
  // nobody uses the window, so the motion whose frames are measured does not run.
  const { ambientPaused } = usePerf();
  const resting = useResting();
  const idle = useLaunch((s) => s.phase.kind === "idle");
  const dnd = useSocialUi((s) => s.view.me?.status === "dnd");
  const calm = active && home && !onboarding && !ambientPaused && !resting && idle && !dnd;
  const task =
    advice === "switched" ? "tell" : step === "measure" && !performanceMode ? "measure" : null;

  useEffect(() => {
    if (!calm || !task) return;
    let stop = new AbortController();
    let timer = 0;
    // A resize (layout work on every frame) starts the wait over; leaving Home, hiding the
    // window or starting a game ends this effect, and the next calm moment tries again.
    const arm = () => {
      stop.abort();
      stop = new AbortController();
      const { signal } = stop;
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        if (task === "tell") {
          tellSwitched();
          return;
        }
        void measureFrames(MEASURE_MS, signal, preview?.frameLoadMs).then((deltas) => {
          if (deltas) offerAfter(deltas);
        });
      }, SETTLE_MS);
    };
    arm();
    window.addEventListener("resize", arm);
    return () => {
      window.removeEventListener("resize", arm);
      window.clearTimeout(timer);
      stop.abort();
    };
  }, [calm, task]);

  return null;
}
