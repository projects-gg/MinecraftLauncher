import { create } from "zustand";
import { persist } from "zustand/middleware";

export type GameStartBehavior = "keep" | "minimize" | "tray" | "close";
export type UpdateChannel = "stable" | "beta";

/**
 * Launcher preferences. UI-only ones stay here; the ones the backend needs (channel, Discord,
 * download limits) are mirrored to the Rust settings file when that command lands.
 */
interface PrefsState {
  performanceMode: boolean;
  particles: boolean;
  sounds: boolean;
  soundVolume: number;
  language: "tr" | "en";
  selectedInstanceId: string | null;
  autoJoin: boolean;
  onGameStart: GameStartBehavior;
  startOnBoot: boolean;
  closeToTray: boolean;
  updateChannel: UpdateChannel;
  autoDownloadUpdates: boolean;
  discordEnabled: boolean;
  discordShowServer: boolean;
  concurrentDownloads: number;
  verifyOnLaunch: boolean;
  /** The v1 import was offered once (first start). */
  legacyOffered: boolean;
  set: <K extends keyof PrefValues>(key: K, value: PrefValues[K]) => void;
  selectInstance: (id: string) => void;
  setAutoJoin: (on: boolean) => void;
  setPerformanceMode: (on: boolean) => void;
}

export type PrefValues = Omit<
  PrefsState,
  "set" | "selectInstance" | "setAutoJoin" | "setPerformanceMode"
>;

export const usePrefs = create<PrefsState>()(
  persist(
    (set) => ({
      performanceMode: false,
      particles: true,
      sounds: true,
      soundVolume: 35,
      language: "tr",
      selectedInstanceId: null,
      autoJoin: true,
      onGameStart: "minimize",
      startOnBoot: false,
      closeToTray: true,
      updateChannel: "stable",
      autoDownloadUpdates: true,
      discordEnabled: true,
      discordShowServer: true,
      concurrentDownloads: 16,
      verifyOnLaunch: false,
      legacyOffered: false,
      set: (key, value) => set({ [key]: value } as Partial<PrefsState>),
      selectInstance: (selectedInstanceId) => set({ selectedInstanceId }),
      setAutoJoin: (autoJoin) => set({ autoJoin }),
      setPerformanceMode: (performanceMode) => set({ performanceMode }),
    }),
    { name: "pl.prefs", version: 2 },
  ),
);
