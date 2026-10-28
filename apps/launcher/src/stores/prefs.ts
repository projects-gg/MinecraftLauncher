import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { LanguageChoice } from "@/lib/i18n";

export type GameStartBehavior = "keep" | "minimize" | "tray" | "close";
/**
 * Settings → Genel → "Pencereyi kapatınca launcher": "quit" unless a game or AFK accounts keep it
 * (a job the player started finishes first), or always stay in the tray (shell/close-button.ts).
 */
export type CloseBehavior = "quit" | "tray";
export type CleanupSetDays = 30 | 60 | 90;
export type UpdateChannel = "stable" | "beta";
/**
 * The one-time look at this computer (features/perf): "todo" until it is decided, "switched"
 * while the launcher has turned Performance mode on and not yet said so, "done" afterwards.
 */
export type PerfAdvice = "todo" | "switched" | "done";

/**
 * Launcher preferences. UI-only ones stay here; the ones the backend needs (channel, Discord,
 * download limits) are mirrored to the Rust settings file when that command lands.
 */
interface PrefsState {
  performanceMode: boolean;
  /** The player switched Performance mode themselves: the launcher never changes or offers it. */
  perfTouched: boolean;
  perfAdvice: PerfAdvice;
  particles: boolean;
  sounds: boolean;
  soundVolume: number;
  /** Settings → Genel → Dil: "auto" (Turkish in Turkey or on a Turkish system) or the player's. */
  language: LanguageChoice;
  selectedInstanceId: string | null;
  autoJoin: boolean;
  /**
   * AUTOJOIN: where each instance goes on Projects after the lobby's login (instance id → Studio's
   * sub-server id, "play.projects.gg → SkyBlock"); missing: the lobby. Kept per instance: the pick
   * belongs to the address beside it.
   */
  autoJoinRoutes: Record<string, string>;
  onGameStart: GameStartBehavior;
  startOnBoot: boolean;
  closeBehavior: CloseBehavior;
  /** System notifications while the launcher is in the background (long installs, friends). */
  desktopNotifications: boolean;
  /** Settings → Genel: every screenshot taken in game goes to the clipboard (src-tauri shot_copy). */
  copyScreenshots: boolean;
  /**
   * Settings → Genel "Oyun içi katman" (OVL2): the official pack installs the overlay's mod and
   * each game start gets a bridge to it (features/overlay, src-tauri overlay.rs).
   */
  inGameOverlay: boolean;
  /**
   * Settings → Genel "Sohbette emoji seçici" (CHAT-EMOJI): the chat screen's emoji button in the
   * game. The overlay's mod goes into an install for it too; with the overlay off it runs alone.
   */
  chatEmoji: boolean;
  updateChannel: UpdateChannel;
  autoDownloadUpdates: boolean;
  discordEnabled: boolean;
  discordShowServer: boolean;
  concurrentDownloads: number;
  verifyOnLaunch: boolean;
  /** Settings → İndirmeler: the selected instance downloads while the launcher is idle. */
  backgroundPrepare: boolean;
  /** The v1 import was offered once (first start). */
  legacyOffered: boolean;
  /** The first-run welcome (account, v1 import, Projects instance) was completed or skipped. */
  onboarded: boolean;
  /** "Still running in the tray" was explained once. */
  trayHintShown: boolean;
  /** Maximum memory for new instances; `null` = recommended for this machine. */
  defaultMaxMemoryMb: number | null;
  /** Extra JVM arguments for new instances. */
  defaultJvmArgs: string;
  /** Snapshot (development) versions are listed when creating instances and switching versions. */
  showSnapshots: boolean;
  /** Settings → Veri → Temizlik: version sets not played for this many days are offered. */
  cleanupSetDays: CleanupSetDays;
  /** When the cleanup reminder was last shown (ms), and the gain it showed. */
  cleanupRemindedAt: number | null;
  cleanupRemindedBytes: number;
  set: <K extends keyof PrefValues>(key: K, value: PrefValues[K]) => void;
  selectInstance: (id: string) => void;
  setAutoJoin: (on: boolean) => void;
  /** AUTOJOIN: the instance's sub-server (its id), or `null` for the lobby. */
  setAutoJoinRoute: (instanceId: string, subServer: string | null) => void;
  /** The player's own switch (Settings, the palette, a toast's button): marks `perfTouched`. */
  setPerformanceMode: (on: boolean) => void;
}

export type PrefValues = Omit<
  PrefsState,
  "set" | "selectInstance" | "setAutoJoin" | "setAutoJoinRoute" | "setPerformanceMode"
>;

export const usePrefs = create<PrefsState>()(
  persist(
    (set) => ({
      performanceMode: false,
      perfTouched: false,
      perfAdvice: "todo",
      particles: true,
      sounds: true,
      soundVolume: 35,
      language: "auto",
      selectedInstanceId: null,
      autoJoin: true,
      autoJoinRoutes: {},
      onGameStart: "minimize",
      startOnBoot: false,
      closeBehavior: "quit",
      desktopNotifications: true,
      copyScreenshots: false,
      inGameOverlay: true,
      chatEmoji: true,
      updateChannel: "stable",
      autoDownloadUpdates: true,
      discordEnabled: true,
      discordShowServer: true,
      concurrentDownloads: 16,
      verifyOnLaunch: false,
      backgroundPrepare: true,
      legacyOffered: false,
      onboarded: false,
      trayHintShown: false,
      defaultMaxMemoryMb: null,
      defaultJvmArgs: "",
      showSnapshots: false,
      cleanupSetDays: 60,
      cleanupRemindedAt: null,
      cleanupRemindedBytes: 0,
      set: (key, value) => set({ [key]: value } as Partial<PrefsState>),
      selectInstance: (selectedInstanceId) => set({ selectedInstanceId }),
      setAutoJoin: (autoJoin) => set({ autoJoin }),
      setAutoJoinRoute: (instanceId, subServer) =>
        set((state) => {
          const { [instanceId]: _previous, ...rest } = state.autoJoinRoutes ?? {};
          return {
            autoJoinRoutes: subServer === null ? rest : { ...rest, [instanceId]: subServer },
          };
        }),
      setPerformanceMode: (performanceMode) => set({ performanceMode, perfTouched: true }),
    }),
    { name: "pl.prefs", version: 4, migrate: migratePrefs },
  ),
);

/**
 * Saved preferences of older versions.
 *
 * Version 3 (L1, 18 Oct 2026): the language is automatic (Turkish in Turkey or on a Turkish
 * system, English otherwise) unless the player picks one. A saved "tr" was never a choice
 * (Turkish was the only language then), so it becomes "auto": players in Turkey see no change,
 * players abroad on an English system get English. A saved "en" stays the player's.
 *
 * Version 4 (TRAY1, 20 Oct 2026): the switch "Kapatınca tepside çalışmaya devam etsin"
 * (`closeToTray`, on by default) gives way to `closeBehavior`, and everyone starts on the new
 * default "quit". The store saves every value, so a saved `true` cannot tell the untouched
 * default from a choice; nearly all of them are the default, and a saved `false` already meant
 * quitting.
 */
export function migratePrefs(persisted: unknown, version: number): PrefValues {
  const prefs = {
    ...(persisted as Partial<PrefValues> & { language?: string; closeToTray?: unknown }),
  };
  if (version < 3) prefs.language = prefs.language === "en" ? "en" : "auto";
  if (version < 4) {
    delete prefs.closeToTray;
    prefs.closeBehavior = "quit";
  }
  return prefs as PrefValues;
}
