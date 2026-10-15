import { Channel } from "@tauri-apps/api/core";
import { create } from "zustand";
import { commands, type UpdateProgress } from "@/bindings";
import { isTauri } from "@/lib/platform";
import { usePrefs } from "./prefs";

type Phase = "idle" | "checking" | "downloading" | "ready" | "applying" | "error";

interface UpdateState {
  phase: Phase;
  version: string | null;
  notes: string | null;
  mandatory: boolean;
  percent: number;
  error: string | null;
  check: () => Promise<void>;
  apply: () => Promise<void>;
}

const NOTES_KEY = "pl.pendingNotes";

/** Background launcher updates: check, download silently, install on "restart". */
export const useUpdate = create<UpdateState>()((set, get) => ({
  phase: "idle",
  version: null,
  notes: null,
  mandatory: false,
  percent: 0,
  error: null,
  async check() {
    if (!isTauri() || get().phase === "downloading" || get().phase === "ready") return;
    const { updateChannel, autoDownloadUpdates } = usePrefs.getState();
    set({ phase: "checking", error: null });
    try {
      const status = await commands.updateCheck(updateChannel);
      if (status.status === "error") throw new Error(status.error.message);
      const s = status.data;
      if (s.state !== "available" && s.state !== "mandatory") {
        set({ phase: "idle" });
        return;
      }
      const mandatory = s.state === "mandatory";
      set({ version: s.version, notes: s.notes, mandatory });
      if (!autoDownloadUpdates && !mandatory) {
        set({ phase: "idle" });
        return;
      }
      set({ phase: "downloading", percent: 0 });
      const progress = new Channel<UpdateProgress>();
      progress.onmessage = (p) => {
        if (p.total) set({ percent: Math.min(100, (p.downloaded / p.total) * 100) });
      };
      const result = await commands.updateDownload(updateChannel, progress);
      if (result.status === "error") throw new Error(result.error.message);
      set({ phase: result.data ? "ready" : "idle", percent: 100 });
    } catch (error) {
      set({ phase: "error", error: error instanceof Error ? error.message : String(error) });
    }
  },
  async apply() {
    const { notes, version } = get();
    // Shown once by the "What's new" dialog after the restart.
    try {
      localStorage.setItem(NOTES_KEY, JSON.stringify({ version, notes }));
    } catch {
      // Storage unavailable: the update still installs.
    }
    set({ phase: "applying" });
    const result = await commands.updateApply();
    if (result.status === "error") set({ phase: "ready", error: result.error.message });
  },
}));

/** Notes of the version we just updated to, consumed once. */
export function takePendingNotes(currentVersion: string): { version: string; notes: string } | null {
  try {
    const raw = localStorage.getItem(NOTES_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as { version: string | null; notes: string | null };
    if (parsed.version !== currentVersion) return null;
    localStorage.removeItem(NOTES_KEY);
    return parsed.notes ? { version: parsed.version, notes: parsed.notes } : null;
  } catch {
    return null;
  }
}
