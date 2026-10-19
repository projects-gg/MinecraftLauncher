import { toast } from "@pl/ui";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";
import { commands } from "@/bindings";
import { confirmAfkBeforePlay } from "@/features/accounts/afk-guard";
import { launchErrorCode } from "@/features/disk/launch-error";
import { useJavaPrompt } from "@/features/java/prompt-store";
import { announceShotNotCopied } from "@/features/library/shot-copy";
import { announceModsNotice, announceVersionDropped } from "@/features/modcenter/notices";
import { announceModWarnings, useModFlow } from "@/features/modhealth/flow";
import { loadNotes } from "@/features/updates/notes";
import { api, type LaunchEvent, type LaunchOptions, type LaunchStage } from "@/lib/api";
import { notifyWhenAway } from "@/lib/attention";
import i18n from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { playSound } from "@/lib/sound";
import { refreshWindowAway } from "@/lib/window-away";
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
  /** `code` picks the toast's action ("DISK-DOLU": "Yer aç"); it is never shown. */
  | { kind: "error"; code: string; message: string; details: string };

/** The diagnosis of a crash with its suspects and one-click actions (mod health flow). */
export type CrashInfo = Omit<Extract<LaunchEvent, { type: "crashed" }>, "type">;

export interface PackNotes {
  name: string;
  version: string;
  changelog: string;
}

/** Screenshots the last game session took ("N yeni ekran görüntüsü"). */
export interface NewScreenshots {
  instanceId: string;
  count: number;
  /** Image URLs of the newest ones (at most three, newest first). */
  recent: string[];
  /** File name of the newest one, for "Kopyala". */
  newest: string;
  /** Tells announcements of two sessions apart. */
  at: number;
}

interface LaunchState {
  instanceId: string | null;
  phase: LaunchPhase;
  offline: boolean;
  crash: CrashInfo | null;
  /** Changelog of the official pack update applied by the last launch, when opened. */
  packNotes: PackNotes | null;
  /** Set when a session that took screenshots ended (the window is back by then). */
  newShots: NewScreenshots | null;
  /** `ignoreModIssues`: "Yine de başlat" after the mod check stopped a launch. */
  play: (instanceId: string, options?: LaunchOptions) => Promise<void>;
  cancel: () => Promise<void>;
  dismissCrash: () => void;
  showPackNotes: (notes: PackNotes | null) => void;
  clearNewShots: () => void;
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
  await refreshWindowAway();
}

async function restoreWindow() {
  if (!isTauri()) return;
  // The shell shows, restores and brings the window forward where Windows allows it, else the
  // taskbar button flashes (shell::focus_main_window). Not the window's setFocus: on Windows it
  // presses Alt (a fake key in the app the player is in) when the foreground is refused.
  await commands.windowBringForward();
  await refreshWindowAway();
}

/**
 * The current launch. `cancel` moves past it, so what a cancelled launch still reports (a late
 * progress tick, its end) changes nothing; only a game that started anyway is followed.
 */
let launchRun = 0;
/**
 * OYNA was pressed and the launch has not begun yet: the AFK check asks the backend first (and
 * may ask the player). A second press meanwhile (a double click) starts nothing more.
 */
let starting = false;
const gameEvents = new Set<LaunchEvent["type"]>([
  "started",
  "windowShown",
  "crashed",
  "newScreenshots",
  "screenshotNotCopied",
  "exited",
]);

/** One active launch at a time; the command bar and Discord presence read from here. */
export const useLaunch = create<LaunchState>()((set, get) => ({
  instanceId: null,
  phase: { kind: "idle" },
  offline: false,
  crash: null,
  packNotes: null,
  newShots: null,
  async play(instanceId, options) {
    const { phase, instanceId: current } = get();
    if (phase.kind === "working" || phase.kind === "launching" || phase.kind === "running") {
      // One game at a time. Another instance's "Oyna" says so (instead of doing nothing) and the
      // selection its caller made goes back to the game that is on, which the bar still shows.
      if (current && current !== instanceId) {
        toast(i18n.t("play.oneAtATime"));
        usePrefs.getState().selectInstance(current);
      }
      return;
    }
    if (starting) return;
    // Same name as a running AFK session: the player chooses first (accounts/afk-guard).
    starting = true;
    let go = false;
    try {
      go = await confirmAfkBeforePlay(useLaunch);
    } finally {
      starting = false;
    }
    if (!go) return;
    playSound("play");
    launchRun += 1;
    const run = launchRun;
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
    // The launcher usually hides once the game window shows, so a pack update is announced
    // again when the player comes back.
    let unseenNotes: PackNotes | null = null;
    // Announced once the window is back, after the game closed.
    let shots: Omit<NewScreenshots, "at"> | null = null;
    // A long first install: a player who went elsewhere hears when it is over.
    const startedAt = Date.now();
    const tellIfAway = (title: string, body: string) =>
      void notifyWhenAway(Date.now() - startedAt, title, body);
    const announcePack = (notes: PackNotes | null, name: string, version: string) => {
      // "Yenilikler" opens the changelog with its text in place: its renderer loads meanwhile.
      if (notes) void loadNotes();
      toast.success(i18n.t("play.packUpdated", { name, version }), {
        duration: 8000,
        action: notes
          ? {
              label: i18n.t("play.packWhatsNew"),
              onClick: () => {
                unseenNotes = null;
                set({ packNotes: notes });
              },
            }
          : undefined,
      });
    };
    const onEvent = (event: LaunchEvent) => {
      if (run !== launchRun && !gameEvents.has(event.type)) return;
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
          tellIfAway(i18n.t("attention.ready.title"), i18n.t("attention.ready.body"));
          break;
        case "started":
          set({ phase: { kind: "running", since: Date.now() } });
          playSound("success");
          break;
        case "windowShown":
          void applyGameStartBehavior();
          break;
        case "crashed": {
          const { type: _type, ...crash } = event;
          set({ crash });
          // The crash opens the mod health flow (suspects, "Onar", memory, "Yeniden yükle").
          useModFlow.getState().open(instanceId, "crash", { kind: "crash" });
          break;
        }
        case "modsBlocked":
          // The game was not started: the player picks "Onar", "Yine de başlat" or "İptal".
          set({ phase: { kind: "idle" } });
          playSound("error");
          tellIfAway(i18n.t("attention.blocked.title"), i18n.t("attention.blocked.body"));
          useModFlow
            .getState()
            .open(instanceId, "blocked", { kind: "blocked", check: event.check });
          break;
        case "modsWarning":
          announceModWarnings(instanceId);
          break;
        case "javaMismatch": {
          // The custom Java does not suit the game: nothing was downloaded yet; the player picks
          // "Kur ve oyna", "Yine de dene" or cancels (features/java). A launch from the tray or a
          // shortcut brings the window forward for the question.
          const { type: _type, ...prompt } = event;
          set({ phase: { kind: "idle" } });
          void restoreWindow();
          useJavaPrompt.getState().open({ prompt, options });
          break;
        }
        case "newScreenshots":
          shots = {
            instanceId,
            count: event.count,
            recent: event.recent,
            newest: event.newest,
          };
          break;
        case "screenshotNotCopied":
          // "Ekran görüntülerini panoya kopyala" failed during the game (told once per game).
          announceShotNotCopied();
          break;
        case "exited":
          set({ phase: { kind: "idle" }, newShots: shots ? { ...shots, at: Date.now() } : null });
          shots = null;
          void restoreWindow();
          if (unseenNotes) {
            announcePack(unseenNotes, unseenNotes.name, unseenNotes.version);
            unseenNotes = null;
          }
          break;
        case "error":
          set({
            phase: {
              kind: "error",
              code: launchErrorCode(event.details),
              message: event.message,
              details: event.details,
            },
          });
          playSound("error");
          tellIfAway(i18n.t("attention.failed.title"), event.message);
          break;
        case "cancelled":
          // Stopped from somewhere else than `cancel` (it already went idle): no message.
          set({ phase: { kind: "idle" } });
          break;
        case "packUpdated": {
          const { name, version, changelog } = event;
          unseenNotes = changelog ? { name, version, changelog } : null;
          announcePack(unseenNotes, name, version);
          break;
        }
        case "modsNotice":
          announceModsNotice(instanceId, event);
          break;
        case "packVersionDropped":
          announceVersionDropped(instanceId, event.minecraft, event.recommended);
          break;
      }
    };
    try {
      await api.launch(instanceId, onEvent, options);
    } catch {
      // Errors also arrive as events; anything else (cancel, transport) returns to idle.
      if (run === launchRun && get().phase.kind !== "error") set({ phase: { kind: "idle" } });
    }
  },
  async cancel() {
    const { instanceId } = get();
    // Calm and at once: the bar is idle right away and nothing the stopping launch still
    // reports brings it back (the launcher stops its downloads within a second).
    launchRun += 1;
    set({ phase: { kind: "idle" } });
    if (instanceId) await api.cancelLaunch(instanceId);
  },
  dismissCrash: () => set({ crash: null }),
  showPackNotes: (packNotes) => set({ packNotes }),
  clearNewShots: () => set({ newShots: null }),
  reset: () => set({ phase: { kind: "idle" } }),
}));
