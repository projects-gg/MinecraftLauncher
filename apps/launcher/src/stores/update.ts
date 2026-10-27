import { Channel } from "@tauri-apps/api/core";
import { create } from "zustand";
import { commands, type UpdateProgress, type UpdateStep } from "@/bindings";
import { keepBeta } from "@/features/updates/beta-gate";
import { isTauri } from "@/lib/platform";
import { usePrefs } from "./prefs";

/**
 * - `available`: found but not downloaded (background downloads are off); the title bar offers it.
 * - `error`: the last check or download failed; `version` says whether an update was found.
 */
export type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "applying"
  | "error";

/** What a check found (for the manual "Güncellemeleri denetle" button). */
export type CheckResult =
  | "upToDate"
  | "downloading"
  | "ready"
  | "available"
  | "unavailable"
  | "busy";

/** Where the question about open AFK accounts was asked (the pill or Settings → Güncellemeler). */
export type AskFrom = "pill" | "settings";

interface UpdateState {
  phase: UpdatePhase;
  version: string | null;
  notes: string | null;
  mandatory: boolean;
  percent: number;
  error: string | null;
  /** The player asked for this check: its download shows in the title bar. */
  manual: boolean;
  /** While installing: open AFK accounts stop first, then the installer starts. */
  step: UpdateStep | null;
  /** The question before installing over open AFK accounts, where it is open. */
  asking: { from: AskFrom; count: number } | null;
  /** `manual`: the player asked, so an available update downloads even without auto-download. */
  check: (manual?: boolean) => Promise<CheckResult>;
  /**
   * Installs the downloaded update; returns only when that failed (the launcher restarts).
   * `afkOpen`: AFK accounts are open, so the first step is stopping them (no other label flashes).
   */
  apply: (options?: { afkOpen?: boolean }) => Promise<void>;
}

const NOTES_KEY = "pl.pendingNotes";

/** The backend's code for "the downloaded update is gone": download it again. */
const NOT_READY = "GUNCELLEME-HAZIR-DEGIL";

/** The page the player is on (the hash route), to come back to after the restart. */
function currentRoute(): string | null {
  const route = window.location.hash.slice(1);
  return route.startsWith("/") ? route : null;
}

/** Background launcher updates: check, download silently, install on "restart". */
export const useUpdate = create<UpdateState>()((set, get) => ({
  phase: "idle",
  version: null,
  notes: null,
  mandatory: false,
  percent: 0,
  error: null,
  manual: false,
  step: null,
  asking: null,
  async check(manual = false) {
    if (!isTauri()) return "upToDate";
    const current = get().phase;
    if (current === "downloading") return "downloading";
    if (current === "ready" || current === "applying") return "ready";
    if (current === "checking") return "busy";
    const { autoDownloadUpdates } = usePrefs.getState();
    // A background look at an update the title bar already offers keeps the offer in place.
    const quiet = !manual && current === "available";
    if (!quiet) set({ phase: "checking", error: null, manual });
    try {
      // BETA-GATE: the beta channel asks first whether this launcher may stay on it.
      const updateChannel =
        usePrefs.getState().updateChannel === "beta" ? await keepBeta() : "stable";
      const status = await commands.updateCheck(updateChannel);
      if (status.status === "error") throw new Error(status.error.message);
      const s = status.data;
      if (s.state === "unavailable") {
        set({ phase: "idle", manual: false });
        return "unavailable";
      }
      if (s.state !== "available" && s.state !== "mandatory") {
        // Nothing for this launcher (up to date, or held back by the staged rollout).
        set({ phase: "idle", manual: false, version: null, notes: null, mandatory: false });
        return "upToDate";
      }
      const mandatory = s.state === "mandatory";
      set({ version: s.version, notes: s.notes, mandatory });
      if (!autoDownloadUpdates && !mandatory && !manual) {
        set({ phase: "available", manual: false });
        return "available";
      }
      set({ phase: "downloading", percent: 0, manual });
      const progress = new Channel<UpdateProgress>();
      progress.onmessage = (p) => {
        if (!p.total) return;
        const percent = Math.min(100, (p.downloaded / p.total) * 100);
        // A message comes with every chunk; the screen only needs whole steps.
        if (Math.floor(percent) !== Math.floor(get().percent)) set({ percent });
      };
      const result = await commands.updateDownload(updateChannel, progress);
      if (result.status === "error") throw new Error(result.error.message);
      set({ phase: result.data ? "ready" : "idle", percent: 100 });
      return result.data ? "ready" : "upToDate";
    } catch (error) {
      set({ phase: "error", error: error instanceof Error ? error.message : String(error) });
      return "unavailable";
    }
  },
  async apply(options) {
    const { phase, notes, version } = get();
    // A second press (or the toast's button meanwhile) starts nothing more.
    if (phase !== "ready") return;
    // Shown once by "Yenilikler" after the restart. The backend keeps them too (its restart note);
    // this copy is for when that could not be written.
    try {
      localStorage.setItem(NOTES_KEY, JSON.stringify({ version, notes }));
    } catch {
      // Storage unavailable: the update still installs.
    }
    set({
      phase: "applying",
      step: options?.afkOpen ? "stoppingAfk" : null,
      error: null,
      asking: null,
    });
    const steps = new Channel<UpdateStep>();
    steps.onmessage = (step) => set({ step });
    const result = await commands.updateApply(currentRoute(), steps);
    // On Windows the installer takes over and the launcher quits: only a failure comes back.
    if (result.status === "error") {
      const lost = result.error.code === NOT_READY;
      set({ phase: lost ? "error" : "ready", step: null, error: result.error.message });
    }
  },
}));

/** Notes of the version we just updated to, consumed once. */
export function takePendingNotes(
  currentVersion: string,
  storage: Pick<Storage, "getItem" | "removeItem"> = localStorage,
): { version: string; notes: string } | null {
  try {
    const raw = storage.getItem(NOTES_KEY);
    if (!raw) return null;
    let parsed: { version?: unknown; notes?: unknown } | null = null;
    try {
      parsed = JSON.parse(raw);
    } catch {
      // Unreadable: dropped below, it can never show.
    }
    if (!parsed || typeof parsed !== "object" || typeof parsed.version !== "string") {
      storage.removeItem(NOTES_KEY);
      return null;
    }
    // Another version's notes wait for it (an update that did not install yet).
    if (parsed.version !== currentVersion) return null;
    storage.removeItem(NOTES_KEY);
    return typeof parsed.notes === "string" && parsed.notes.trim()
      ? { version: parsed.version, notes: parsed.notes }
      : null;
  } catch {
    // Storage unavailable.
    return null;
  }
}
