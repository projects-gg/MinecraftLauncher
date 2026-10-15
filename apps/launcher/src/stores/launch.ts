import { create } from "zustand";
import { api, type LaunchEvent, type LaunchStage } from "@/lib/api";

export type LaunchPhase =
  | { kind: "idle" }
  | {
      kind: "working";
      stage: LaunchStage;
      percent: number;
      doneFiles: number;
      totalFiles: number;
      bytesPerSec: number;
      etaSecs?: number;
    }
  | { kind: "launching" }
  | { kind: "running"; since: number }
  | { kind: "error"; message: string; details: string };

interface LaunchState {
  instanceId: string | null;
  phase: LaunchPhase;
  play: (instanceId: string, onEvent?: (e: LaunchEvent) => void) => Promise<void>;
  cancel: () => Promise<void>;
  reset: () => void;
}

/** One active launch at a time; the command bar and Discord presence read from here. */
export const useLaunch = create<LaunchState>()((set, get) => ({
  instanceId: null,
  phase: { kind: "idle" },
  async play(instanceId, onEvent) {
    const { phase } = get();
    if (phase.kind === "working" || phase.kind === "launching" || phase.kind === "running") return;
    set({ instanceId, phase: { kind: "working", stage: "metadata", percent: 0, doneFiles: 0, totalFiles: 0, bytesPerSec: 0 } });
    try {
      await api.launch(instanceId, (event) => {
        onEvent?.(event);
        switch (event.type) {
          case "progress":
            set({ phase: { kind: "working", ...event } });
            break;
          case "launching":
            set({ phase: { kind: "launching" } });
            break;
          case "started":
            set({ phase: { kind: "running", since: Date.now() } });
            break;
          case "exited":
            set({ phase: { kind: "idle" } });
            break;
          case "error":
            set({ phase: { kind: "error", message: event.message, details: event.details } });
            break;
        }
      });
    } catch {
      // Cancellation or a transport failure: return to idle; errors arrive as events.
      if (get().phase.kind !== "error") set({ phase: { kind: "idle" } });
    }
  },
  async cancel() {
    const { instanceId } = get();
    if (instanceId) await api.cancelLaunch(instanceId);
    set({ phase: { kind: "idle" } });
  },
  reset: () => set({ phase: { kind: "idle" } }),
}));
