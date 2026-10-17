// Unsaved edits that outlive the tab they were made in: an instance's settings form and its mod
// center choices stay until they are saved (applied) or discarded, across tab switches and
// page visits. Kept in memory only: a restart starts clean.
import { create } from "zustand";
import type { GraphicsPreset, InstanceSettings } from "@/lib/api";

/**
 * The settings form as edited, with the graphics preset the player picked (absent: the file's
 * graphics stay). The form keeps a pick only while it differs from what the file has, so a stale
 * copy never writes over what the player set in the game meanwhile.
 */
export type SettingsDraft = InstanceSettings & { graphics?: GraphicsPreset };

/** Mod center choices not applied yet. */
export interface ModDraft {
  /** Recommended item → switched on, only where it differs from the recorded choice. */
  choices: Record<string, boolean>;
  /** Items whose changed file goes back to the pack's copy. */
  revert: string[];
}

interface DraftState {
  /** Instance id → the settings form as edited (absent: nothing unsaved). */
  settings: Record<string, SettingsDraft>;
  /** Instance id → mod center choices (absent: nothing pending). */
  mods: Record<string, ModDraft>;
  setSettings: (instanceId: string, draft: SettingsDraft | null) => void;
  setMods: (instanceId: string, draft: ModDraft | null) => void;
}

function put<T>(map: Record<string, T>, key: string, value: T | null): Record<string, T> {
  const next = { ...map };
  if (value === null) delete next[key];
  else next[key] = value;
  return next;
}

export const useDrafts = create<DraftState>()((set) => ({
  settings: {},
  mods: {},
  setSettings: (instanceId, draft) =>
    set((s) => ({ settings: put(s.settings, instanceId, draft) })),
  setMods: (instanceId, draft) => set((s) => ({ mods: put(s.mods, instanceId, draft) })),
}));

/**
 * A save of `sent` finished: its draft goes, unless the form changed while the save ran (the
 * save waits for the refreshed instance, which can take a moment). Edits typed meanwhile stay a
 * draft, and the bar stays for them.
 */
export function settleSettingsDraft(instanceId: string, sent: SettingsDraft) {
  const state = useDrafts.getState();
  if (state.settings[instanceId] === sent) state.setSettings(instanceId, null);
}

const settingKeys: Array<keyof InstanceSettings> = [
  "javaMode",
  "javaPath",
  "minMemoryMb",
  "maxMemoryMb",
  "memoryAuto",
  "jvmArgs",
  "width",
  "height",
  "fullscreen",
  "preLaunch",
  "postExit",
  "wrapper",
  "notes",
];

/**
 * The settings form differs from what is saved, as far as the player can see: a Java path only
 * counts with "Özel" Java, memory values only with "Özel" memory (a detour through "Özel" and
 * back is no change). A picked graphics preset always counts (the form drops one that matches
 * the file).
 */
export function settingsChanged(draft: SettingsDraft, saved: InstanceSettings): boolean {
  if (draft.graphics !== undefined) return true;
  return settingKeys.some((key) => {
    switch (key) {
      case "javaPath":
        return draft.javaMode === "custom" && (draft.javaPath ?? "") !== (saved.javaPath ?? "");
      case "minMemoryMb":
      case "maxMemoryMb":
        return !draft.memoryAuto && draft[key] !== saved[key];
      default:
        return draft[key] !== saved[key];
    }
  });
}
