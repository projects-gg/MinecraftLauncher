import { toast } from "@pl/ui";
import { listen } from "@tauri-apps/api/event";
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
import { setPageDuty } from "@/lib/page-duties";
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
  /**
   * The game runs. Until its window is on screen (`shown` false) OYNA says "Açılıyor" and `since`
   * is when the process started; from then on it says "Oyunda" and `since` is when the window
   * showed. `firstStart`: the first start with this version and loader, which takes longer.
   */
  | { kind: "running"; since: number; shown: boolean; firstStart: boolean }
  /** `code` picks the toast's action ("DISK-DOLU": "Yer aç"); it is never shown. */
  | { kind: "error"; code: string; message: string; details: string };

/** The files are ready and the game is starting, its window not on screen yet: "Açılıyor". */
export function isOpening(phase: LaunchPhase): boolean {
  return phase.kind === "launching" || (phase.kind === "running" && !phase.shown);
}

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
  /** "İptal": stops the launch while files are prepared or while the game opens, never later. */
  cancel: () => Promise<void>;
  dismissCrash: () => void;
  showPackNotes: (notes: PackNotes | null) => void;
  clearNewShots: () => void;
  reset: () => void;
}

/** "Oyunda" settles on OYNA (its cross-fade) before the launcher steps aside for the game. */
const STEP_ASIDE_MS = 450;

/** The backend's name for the end of a game an earlier start left running (`launch.rs`). */
const ADOPTED_GAME_EXITED = "adopted-game-exited";

/**
 * "Kapansın" left this game alone (SMALL7): the launcher quit, or AFK accounts or the player's
 * jobs kept it in the tray for a while. Either way it does not come back when the game ends.
 */
let leftForGame = false;

/** What the launcher window does once the game window appears (Settings > General). */
async function applyGameStartBehavior() {
  if (!isTauri()) return;
  const behavior = usePrefs.getState().onGameStart;
  if (behavior === "keep") return;
  const win = getCurrentWindow();
  const [visible, minimized] = await Promise.all([win.isVisible(), win.isMinimized()]);
  const inSight = visible && !minimized;
  // Already out of sight (put in the tray or minimized while the game opened): it stays there.
  // Minimizing a hidden window would bring it back to the taskbar. "Kapansın" leaves anyway.
  if (!inSight && behavior !== "close") return;
  if (inSight) await new Promise((resolve) => setTimeout(resolve, STEP_ASIDE_MS));
  // The game closed meanwhile (a crash right after its window showed): the launcher stays.
  const { phase } = useLaunch.getState();
  if (phase.kind !== "running" || !phase.shown) return;
  if (behavior === "close") {
    // The game plays on by itself; the shell quits, unless AFK accounts or the player's jobs keep
    // it in the tray a while (it says so through `window-closed`, shell/close-button.ts).
    leftForGame = true;
    try {
      await commands.launcherLeaveForGame();
      return;
    } catch {
      // The shell did not answer: the launcher steps aside as "Tepsiye gizlensin" does.
      leftForGame = false;
    }
  }
  if (behavior === "minimize") await win.minimize();
  else await win.hide();
  await refreshWindowAway();
}

/**
 * A game an earlier start left running when it quit ("Kapansın", SMALL7): the backend follows it
 * again (src-tauri `commands::launch::adopt_detached`), and the bar shows it as "Oyunda" until it
 * ends, without its log or crash help. `onEnded` runs when it has (its play time is recorded by
 * then). Returns the stop function.
 */
export function followAdoptedGame(onEnded: (instanceId: string) => void): () => void {
  if (!isTauri()) return () => {};
  let adopted: string | null = null;
  let stopped = false;
  const stop = listen<{ instanceId: string }>(ADOPTED_GAME_EXITED, ({ payload }) => {
    const { phase, instanceId } = useLaunch.getState();
    // Only the adopted game's bar goes back to OYNA (it is the one game, launches wait for it).
    if (adopted === payload.instanceId && instanceId === adopted && phase.kind === "running") {
      useLaunch.setState({ phase: { kind: "idle" } });
    }
    adopted = null;
    onEnded(payload.instanceId);
  });
  // Asked once the end can be heard, so an end in between is never missed.
  void stop.then(async () => {
    const game = await commands.gameAdopted();
    if (stopped || !game || useLaunch.getState().phase.kind !== "idle") return;
    adopted = game.instanceId;
    useLaunch.setState({
      instanceId: game.instanceId,
      phase: { kind: "running", since: game.sinceMs, shown: true, firstStart: false },
    });
  });
  return () => {
    stopped = true;
    void stop.then((fn) => fn());
  };
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
 * progress tick, its end) changes nothing; only a game whose window showed anyway is followed.
 */
let launchRun = 0;
/**
 * OYNA was pressed and the launch has not begun yet: the AFK check asks the backend first (and
 * may ask the player). A second press meanwhile (a double click) starts nothing more.
 */
let starting = false;
/** The launch whose game the bar follows: started (or its window showed) and not exited yet. */
let gameRun = 0;
/**
 * A game that "İptal" closed while it opened is still exiting. The backend runs one game at a
 * time, so the next launch waits for that exit (at most `CLOSE_WAIT_MS`).
 */
let closing: { run: number; done: Promise<void>; settle: () => void } | null = null;
const CLOSE_WAIT_MS = 5000;
/**
 * While the game opens the page answers its window ("Oyunda", the launcher's own window): the
 * backend must not suspend it meanwhile (src-tauri/src/webview_rest.rs).
 */
const OPENING_DUTY = "game-opening";

function settleClosing(run: number) {
  if (closing?.run !== run) return;
  closing.settle();
  closing = null;
}

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
      // A game "İptal" closed a moment ago is still exiting: this one starts after it.
      if (closing) await closing.done;
      go = await confirmAfkBeforePlay(useLaunch);
    } finally {
      starting = false;
    }
    if (!go) return;
    playSound("play");
    launchRun += 1;
    leftForGame = false;
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
      if (event.type === "exited") settleClosing(run);
      if (run !== launchRun && run !== gameRun && !gameEvents.has(event.type)) return;
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
          // Cancelled meanwhile: the backend closes it (should its window come first, that
          // event brings it back).
          if (run !== launchRun) break;
          gameRun = run;
          set({
            phase: {
              kind: "running",
              since: Date.now(),
              shown: false,
              firstStart: event.firstStart,
            },
          });
          setPageDuty(OPENING_DUTY, true);
          break;
        case "windowShown":
          // Also a game "İptal" could not stop in time: it is on screen, the bar follows it.
          gameRun = run;
          set({
            instanceId,
            phase: { kind: "running", since: Date.now(), shown: true, firstStart: false },
          });
          playSound("success");
          void applyGameStartBehavior().finally(() => setPageDuty(OPENING_DUTY, false));
          break;
        case "crashed": {
          if (run !== gameRun) break;
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
        case "keyClash":
          // The pack's mod keys clash in an older options.txt (asked once per session): the
          // game was not started, the player picks "Düzelt" or "Yine de başlat". A launch from
          // the tray or a shortcut brings the window forward for the question.
          set({ phase: { kind: "idle" } });
          void restoreWindow();
          useModFlow.getState().open(instanceId, "blocked", { kind: "keys", check: event.check });
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
          if (run !== gameRun) break;
          shots = {
            instanceId,
            count: event.count,
            recent: event.recent,
            newest: event.newest,
          };
          break;
        case "screenshotNotCopied":
          if (run !== gameRun) break;
          // "Ekran görüntülerini panoya kopyala" failed during the game (told once per game).
          announceShotNotCopied();
          break;
        case "exited":
          // A game "İptal" closed while it opened: the bar went back to OYNA already.
          if (run !== gameRun) break;
          gameRun = 0;
          setPageDuty(OPENING_DUTY, false);
          set({ phase: { kind: "idle" }, newShots: shots ? { ...shots, at: Date.now() } : null });
          shots = null;
          // "Kapansın" left the game alone: the launcher does not come back for its end.
          if (leftForGame) leftForGame = false;
          else void restoreWindow();
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
    const { instanceId, phase } = get();
    // A game on screen is never closed from the launcher.
    if (phase.kind === "running" && phase.shown) return;
    if (phase.kind === "running") {
      // The game opens: the backend closes it; the next launch waits for its exit.
      let settle = () => {};
      const exited = new Promise<void>((resolve) => {
        settle = resolve;
      });
      const waited = new Promise<void>((resolve) => setTimeout(resolve, CLOSE_WAIT_MS));
      closing = { run: gameRun, done: Promise.race([exited, waited]), settle };
      gameRun = 0;
      setPageDuty(OPENING_DUTY, false);
    }
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
