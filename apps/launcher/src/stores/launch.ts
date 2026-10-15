import { toast } from "@pl/ui";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";
import { api, type LaunchEvent, type LaunchStage } from "@/lib/api";
import i18n from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { usePrefs } from "./prefs";

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

export interface CrashInfo {
  title: string;
  hint: string;
  actions: string[];
  mods: string[];
  logFile: string | null;
}

interface LaunchState {
  instanceId: string | null;
  phase: LaunchPhase;
  offline: boolean;
  crash: CrashInfo | null;
  play: (instanceId: string) => Promise<void>;
  cancel: () => Promise<void>;
  dismissCrash: () => void;
  reset: () => void;
}

/** What the launcher window does once the game window appears (Settings > General). */
async function applyGameStartBehavior() {
  if (!isTauri()) return;
  const win = getCurrentWindow();
  switch (usePrefs.getState().onGameStart) {
    case "minimize":
      await win.minimize();
      break;
    case "tray":
    case "close":
      await win.hide();
      break;
    default:
      break;
  }
}

async function restoreWindow() {
  if (!isTauri()) return;
  const win = getCurrentWindow();
  await win.show();
  await win.unminimize();
  await win.setFocus();
}

/** One active launch at a time; the command bar and Discord presence read from here. */
export const useLaunch = create<LaunchState>()((set, get) => ({
  instanceId: null,
  phase: { kind: "idle" },
  offline: false,
  crash: null,
  async play(instanceId) {
    const { phase } = get();
    if (phase.kind === "working" || phase.kind === "launching" || phase.kind === "running") return;
    set({
      instanceId,
      offline: false,
      crash: null,
      phase: {
        kind: "working",
        stage: "metadata",
        percent: 0,
        doneFiles: 0,
        totalFiles: 0,
        bytesPerSec: 0,
      },
    });
    const onEvent = (event: LaunchEvent) => {
      switch (event.type) {
        case "progress":
          set({ phase: { kind: "working", ...event } });
          break;
        case "offline":
          set({ offline: true });
          toast(i18n.t("play.offlineMode"));
          break;
        case "launching":
          set({ phase: { kind: "launching" } });
          break;
        case "started":
          set({ phase: { kind: "running", since: Date.now() } });
          break;
        case "windowShown":
          void applyGameStartBehavior();
          break;
        case "crashed":
          set({
            crash: {
              title: event.title,
              hint: event.hint,
              actions: event.actions,
              mods: event.mods,
              logFile: event.logFile,
            },
          });
          break;
        case "exited":
          set({ phase: { kind: "idle" } });
          void restoreWindow();
          break;
        case "error":
          set({ phase: { kind: "error", message: event.message, details: event.details } });
          break;
      }
    };
    try {
      await api.launch(instanceId, onEvent);
    } catch {
      // Errors also arrive as events; anything else (cancel, transport) returns to idle.
      if (get().phase.kind !== "error") set({ phase: { kind: "idle" } });
    }
  },
  async cancel() {
    const { instanceId } = get();
    if (instanceId) await api.cancelLaunch(instanceId);
    set({ phase: { kind: "idle" } });
  },
  dismissCrash: () => set({ crash: null }),
  reset: () => set({ phase: { kind: "idle" } }),
}));
