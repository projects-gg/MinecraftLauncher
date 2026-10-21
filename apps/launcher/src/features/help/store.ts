// The troubleshooter's run (HELP1): one at a time, started when its section opens, when a launch
// fails (so the answers are there when the player taps "Sorunu bul") or when the player asks
// again; never in the background otherwise. Rows that settle at once are shown a beat apart, top
// to bottom (a calm cascade, not every row flipping in one frame), and the headline changes only
// once the last row is in.
import { create } from "zustand";
import { api, type HelpCheck, type HelpFinding, type HelpReport } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { GROUP_ORDER, GROUPS } from "./model";

/** The launch that failed before the troubleshooter opened. */
export interface LaunchFailure {
  code: string;
  message: string;
  details: string;
  at: number;
}

/** A run this recent for the same instance is shown again instead of checking anew. */
export const FRESH_MS = 45_000;
/** A failed launch stays on the panel this long. */
export const FAILURE_MS = 15 * 60_000;
/** The first row shows no sooner than this after the start, the next ones this far apart. */
const FIRST_MS = 220;
const STAGGER_MS = 45;

const ORDER: readonly string[] = GROUP_ORDER.flatMap((group) => GROUPS[group]);

interface HelpState {
  /** Grows with every run; answers of an older run are dropped. */
  run: number;
  instanceId: string | null;
  status: "idle" | "running" | "done";
  startedAt: number;
  /** The rows the run checks (the conditional ones come with their finding). */
  planned: HelpCheck[];
  findings: Partial<Record<HelpCheck, HelpFinding>>;
  report: HelpReport | null;
  /** A fix was started: the next visit checks again. */
  stale: boolean;
  failure: LaunchFailure | null;
  /** Checks everything for `instanceId` now (nothing while a run is on). */
  start(instanceId: string | null): void;
  /** Checks unless a fresh run for the same instance is there (or on its way). */
  ensure(instanceId: string | null): void;
  /** A fix went out: what it changed is checked at the next visit. */
  invalidate(): void;
  /** A launch failed: keep it for the panel and check right away. */
  launchFailed(instanceId: string | null, failure: Omit<LaunchFailure, "at">): void;
}

export const useHelp = create<HelpState>()((set, get) => ({
  run: 0,
  instanceId: null,
  status: "idle",
  startedAt: 0,
  planned: [],
  findings: {},
  report: null,
  stale: false,
  failure: null,

  start(instanceId) {
    if (get().status === "running") return;
    const run = get().run + 1;
    const startedAt = Date.now();
    set({
      run,
      instanceId,
      status: "running",
      startedAt,
      stale: false,
      report: null,
      // The rows of the instance checked before keep their place (they turn back to checking),
      // so nothing jumps when the plan comes.
      planned: get().instanceId === instanceId ? get().planned : [],
      findings: {},
    });
    let lastReveal = startedAt;
    const reveal = (finding: HelpFinding) => {
      const index = Math.max(0, ORDER.indexOf(finding.check));
      const at = Math.max(Date.now(), startedAt + FIRST_MS + index * STAGGER_MS);
      lastReveal = Math.max(lastReveal, at);
      window.setTimeout(() => {
        if (get().run !== run) return;
        set((state) => ({ findings: { ...state.findings, [finding.check]: finding } }));
      }, at - Date.now());
    };
    const finish = (report: HelpReport | null) => {
      const wait = Math.max(0, lastReveal - Date.now()) + 60;
      window.setTimeout(() => {
        if (get().run !== run) return;
        const findings: Partial<Record<HelpCheck, HelpFinding>> = { ...get().findings };
        for (const finding of report?.findings ?? []) findings[finding.check] = finding;
        // A run that broke off: the rows it never answered could not be checked.
        for (const check of get().planned) {
          findings[check] ??= { check, level: "skipped", code: "checkFailed", facts: {} };
        }
        set({ status: "done", findings, report });
      }, wait);
    };
    api.help
      .run(instanceId, (event) => {
        if (get().run !== run) return;
        if (event.type === "plan") set({ planned: event.checks });
        else reveal(event.finding);
      })
      .then(finish, () => finish(null));
  },

  ensure(instanceId) {
    const state = get();
    const fresh =
      !state.stale &&
      state.instanceId === instanceId &&
      state.status !== "idle" &&
      Date.now() - state.startedAt < FRESH_MS;
    if (!fresh) state.start(instanceId);
  },

  invalidate() {
    set({ stale: true });
  },

  launchFailed(instanceId, failure) {
    set({ failure: { ...failure, at: Date.now() } });
    if (get().status !== "running") get().start(instanceId);
  },
}));

// A game that got going leaves the failed launch behind: the panel no longer shows it.
useLaunch.subscribe((state, before) => {
  if (state.phase.kind === "running" && before.phase.kind !== "running") {
    if (useHelp.getState().failure) useHelp.setState({ failure: null });
  }
});

/** The failed launch while it is recent enough to matter. */
export function recentFailure(
  failure: LaunchFailure | null,
  now = Date.now(),
): LaunchFailure | null {
  return failure && now - failure.at < FAILURE_MS ? failure : null;
}

/**
 * Opens the troubleshooter (Settings → Sorun yardımcısı) from anywhere: a launch error's toast,
 * the crash dialog, the palette. The router is loaded on demand, as "Yer aç" does.
 */
export function openHelp() {
  void import("@/router").then(({ router }) =>
    router.navigate({ to: "/settings", search: { section: "help" } }),
  );
}
