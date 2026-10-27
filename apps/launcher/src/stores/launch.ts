import { toast } from "@pl/ui";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";
import { commands } from "@/bindings";
import { confirmAfkBeforePlay } from "@/features/accounts/afk-guard";
import { launchErrorCode } from "@/features/disk/launch-error";
import { askGameSwitch } from "@/features/game-switch/store";
import { useJavaPrompt } from "@/features/java/prompt-store";
import { announceShotNotCopied } from "@/features/library/shot-copy";
import { announceModsNotice, announceVersionDropped } from "@/features/modcenter/notices";
import { announceModWarnings, useModFlow } from "@/features/modhealth/flow";
import { loadNotes } from "@/features/updates/notes";
import {
  api,
  type GameAccount,
  type LaunchEvent,
  type LaunchOptions,
  type LaunchStage,
} from "@/lib/api";
import { notify, notifyWhenAway } from "@/lib/attention";
import i18n from "@/lib/i18n";
import { setPageDuty } from "@/lib/page-duties";
import { isTauri } from "@/lib/platform";
import { playSound } from "@/lib/sound";
import { refreshWindowAway } from "@/lib/window-away";
import { inProgress, isMine, launchOwner, planPlay, runningPhase } from "./launch-owner";
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

/**
 * A game of this launcher that runs (PLAY-AGAIN: several may, of one account or of several): from
 * its process start until it ended.
 */
export interface RunningGame {
  /** The page's number for the launch that started it (adopted games get one too). */
  run: number;
  /** The launcher's number for it (closing it, moving it in game); null while not known. */
  game: number | null;
  instanceId: string;
  /** The account it plays with; null for a game an older launcher left running. */
  account: GameAccount | null;
  /** When its process started; once its window is on screen, when that showed. */
  since: number;
  shown: boolean;
  firstStart: boolean;
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
  /**
   * The launch the store follows: the one on its way (files prepared, the game opening), else
   * the newest game that runs. With games running its phase stays "running" until the last one
   * ends, so "a game runs" reads the same as with one game. The bar of an account reads its own
   * launch through `ownLaunch` (stores/launch-owner.ts).
   */
  instanceId: string | null;
  phase: LaunchPhase;
  /**
   * The account the followed launch's game plays with (PLAY-ACCT), from its start on. None while
   * its files are prepared (it starts with the account selected then, unless the launch named
   * one), and for a game an older launcher left running.
   */
  account: GameAccount | null;
  /** Every game of this launcher that runs, oldest first (PLAY-AGAIN). */
  games: RunningGame[];
  /** The page's number of the launch on its way (files prepared, the game opening); 0: none. */
  progress: number;
  /** The page's number of the launch the fields above show; 0: none. */
  followed: number;
  /**
   * The selected account (the shell mirrors the account list: `useSelectedAccountMirror`):
   * `undefined` while it loads, `null` with none. The play rules of code outside React read it.
   */
  selected: string | null | undefined;
  offline: boolean;
  crash: CrashInfo | null;
  /** Changelog of the official pack update applied by the last launch, when opened. */
  packNotes: PackNotes | null;
  /** Set when a session that took screenshots ended (the window is back by then). */
  newShots: NewScreenshots | null;
  /** `ignoreModIssues`: "Yine de başlat" after the mod check stopped a launch. */
  play: (instanceId: string, options?: LaunchOptions) => Promise<void>;
  /**
   * Starts at once, without the rules of `play` (the question that asked them was answered:
   * features/game-switch).
   */
  start: (instanceId: string, options?: LaunchOptions) => Promise<void>;
  /** "İptal": stops the launch while files are prepared or while the game opens, never later. */
  cancel: () => Promise<void>;
  dismissCrash: () => void;
  showPackNotes: (notes: PackNotes | null) => void;
  clearNewShots: () => void;
  /** The error of the followed launch was told: back to the games that run, else idle. */
  reset: () => void;
}

/** "Oyunda" settles on OYNA (its cross-fade) before the launcher steps aside for the game. */
const STEP_ASIDE_MS = 450;

/** The backend's name for the end of a game an earlier start left running (`launch.rs`). */
const ADOPTED_GAME_EXITED = "adopted-game-exited";

/**
 * "Kapansın" left the games alone (SMALL7): the launcher quit, or AFK accounts or the player's
 * jobs kept it in the tray for a while. Either way it does not come back when a game ends.
 */
let leftForGame = false;

/** What the launcher window does once a game's window appears (Settings > General). */
async function applyGameStartBehavior(run: number) {
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
  if (!useLaunch.getState().games.some((game) => game.run === run && game.shown)) return;
  if (behavior === "close") {
    // The games play on by themselves; the shell quits, unless AFK accounts or the player's jobs
    // keep it in the tray a while (it says so through `window-closed`, shell/close-button.ts).
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

/** The page's numbers for launches (and for games an earlier start left running). */
let runs = 0;

/** Follows `game`, or with none the store goes idle (its last launch's instance stays). */
function follow(game: RunningGame | undefined) {
  if (!game) {
    useLaunch.setState({ followed: 0, phase: { kind: "idle" } });
    return;
  }
  useLaunch.setState({
    followed: game.run,
    instanceId: game.instanceId,
    account: game.account,
    phase: runningPhase(game),
  });
}

/** The newest game on screen (else the newest that runs): what the store follows between launches. */
function newestGame(): RunningGame | undefined {
  const { games } = useLaunch.getState();
  return games.findLast((game) => game.shown) ?? games.at(-1);
}

/**
 * Games an earlier start left running when it quit ("Kapansın", SMALL7): the backend follows them
 * again (src-tauri `commands::launch::adopt_detached`), and they are "Oyunda" until they end,
 * without their log or crash help. `onEnded` runs when one has (its play time is recorded by
 * then). Returns the stop function.
 */
export function followAdoptedGame(onEnded: (instanceId: string) => void): () => void {
  if (!isTauri()) return () => {};
  let stopped = false;
  const stop = listen<{ game: number; instanceId: string }>(ADOPTED_GAME_EXITED, ({ payload }) => {
    const { games, followed } = useLaunch.getState();
    const gone = games.find((game) => game.game === payload.game && game.run < 0);
    if (gone) {
      useLaunch.setState({ games: games.filter((game) => game !== gone) });
      if (followed === gone.run) follow(newestGame());
    }
    onEnded(payload.instanceId);
  });
  // Asked once the end can be heard, so an end in between is never missed.
  void stop.then(async () => {
    const adopted = await commands.gameAdopted();
    if (stopped || adopted.length === 0) return;
    // Their own numbers below zero: no launch of this page reports for them.
    const games: RunningGame[] = adopted.map((game) => {
      runs += 1;
      return {
        run: -runs,
        game: game.game,
        instanceId: game.instanceId,
        account: game.account ?? null,
        since: game.sinceMs,
        shown: true,
        firstStart: false,
      };
    });
    const state = useLaunch.getState();
    useLaunch.setState({ games: [...games, ...state.games] });
    if (state.followed === 0 && state.progress === 0) follow(newestGame());
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
 * OYNA was pressed and the launch has not begun yet: the AFK check asks the backend first (and
 * may ask the player). A second press meanwhile (a double click) starts nothing more.
 */
let starting = false;
/** "Oyna" while something runs: the selected account (or the memory) is being read. */
let switching = false;

/** The selected account as the game would play with it (`undefined`: not known now). */
async function selectedAccount(): Promise<GameAccount | null | undefined> {
  try {
    const active = (await api.accounts()).find((a) => a.active);
    return active ? { id: active.id, name: active.username } : null;
  } catch {
    return undefined;
  }
}

/** Whether one more game of `instanceId` fits in the free memory (lead, 27 Oct: ask first). */
async function memoryFits(instanceId: string): Promise<boolean> {
  try {
    return await api.gameMemoryFits(instanceId);
  } catch {
    // Not known: nothing to say.
    return true;
  }
}

/**
 * "Oyna" while a launch is on its way or games run (PLAY-ACCT, PLAY-AGAIN). The selected
 * account's launch of this instance on its way: nothing more (OYNA says so). Another launch on its
 * way: "Durdur ve oyna" first (one launch at a time). This account in game: the owner's question
 * ("Bu adla oyun zaten açık"), then one more game, which joins no server by itself (the same name
 * there would push one out). Only other accounts in game: this one starts beside them. Before any
 * extra game, the free memory: when it would not hold one more, the question says so.
 */
async function playWhileBusy(instanceId: string, options: LaunchOptions | undefined) {
  if (switching) return;
  switching = true;
  let selected: GameAccount | null | undefined;
  try {
    selected = await selectedAccount();
  } finally {
    switching = false;
  }
  const launch = useLaunch.getState();
  const id = selected === undefined ? undefined : (selected?.id ?? null);
  const plan = planPlay(launch, instanceId, id);
  // Everything ended meanwhile: this press starts it.
  if (plan === "play") return launch.start(instanceId, options);
  if (plan === "nothing") return;
  if (plan === "switch") {
    // "Vazgeç" takes the selection back to the launch this account has on its way.
    const mine = launchOwner(launch.phase, launch.account, id) === "mine";
    askGameSwitch({
      kind: "stop",
      instanceId,
      options,
      account: selected ?? null,
      restore: mine ? launch.instanceId : null,
    });
    return;
  }
  // It plays with the account selected now, whatever happens to the selection meanwhile.
  const pinned: LaunchOptions = { ...options, account: options?.account ?? selected ?? undefined };
  const again = plan === "again";
  const extra: LaunchOptions = again ? { ...pinned, autoJoin: false } : pinned;
  if (switching) return;
  switching = true;
  let fits = true;
  try {
    fits = await memoryFits(instanceId);
  } finally {
    switching = false;
  }
  if (!again && fits) return useLaunch.getState().start(instanceId, extra);
  askGameSwitch({
    kind: again ? "again" : "memory",
    memoryShort: !fits,
    instanceId,
    options: extra,
    account: selected ?? null,
    restore: null,
  });
}

/**
 * A game that "İptal" closed while it opened is still exiting. The next launch waits for that
 * exit (at most `CLOSE_WAIT_MS`), so it never starts beside its own leftover.
 */
let closing: { run: number; done: Promise<void>; settle: () => void } | null = null;
const CLOSE_WAIT_MS = 5000;
/**
 * While a game opens the page answers its window ("Oyunda", the launcher's own window): the
 * backend must not suspend it meanwhile (src-tauri/src/webview_rest.rs).
 */
const OPENING_DUTY = "game-opening";

function settleClosing(run: number) {
  if (closing?.run !== run) return;
  closing.settle();
  closing = null;
}

/** Crashes of other games waiting for the crash help (one dialog at a time, oldest first). */
const waitingCrashes: CrashInfo[] = [];

function showCrash(crash: CrashInfo) {
  useLaunch.setState({ crash });
  // The crash opens the mod health flow (suspects, "Onar", memory, "Yeniden yükle").
  useModFlow.getState().open(crash.instanceId, "crash", { kind: "crash" });
}

/** One launch at a time on its way, games on screen beside it; the command bar reads from here. */
export const useLaunch = create<LaunchState>()((set, get) => ({
  instanceId: null,
  phase: { kind: "idle" },
  account: null,
  games: [],
  progress: 0,
  followed: 0,
  selected: undefined,
  offline: false,
  crash: null,
  packNotes: null,
  newShots: null,
  async play(instanceId, options) {
    // Something on its way, or games on screen: the rules first.
    const { phase, games } = get();
    if (inProgress(phase) || games.length > 0) {
      await playWhileBusy(instanceId, options);
      return;
    }
    await get().start(instanceId, options);
  },
  async start(instanceId, options) {
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
    runs += 1;
    leftForGame = false;
    const run = runs;
    set({
      instanceId,
      // Known from the start when the launch names it (a game "Bağlan" starts again, one more
      // game beside others).
      account: options?.account ?? null,
      offline: false,
      progress: run,
      followed: run,
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
    // The account this launch's game plays with, once it started (PLAY-ACCT), and the launcher's
    // number for the game.
    let gameAccount: GameAccount | null = options?.account ?? null;
    let gameNumber: number | null = null;
    // This game's crash, told after its end when another game is on screen.
    let crashed: CrashInfo | null = null;
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
    /** This launch's game in the list, if it runs. */
    const mine = () => get().games.find((game) => game.run === run);
    /** Changes this launch's game (and the followed phase while this launch is followed). */
    const update = (change: Partial<RunningGame>) => {
      const games = get().games.map((game) => (game.run === run ? { ...game, ...change } : game));
      set({ games });
      const game = games.find((g) => g.run === run);
      if (game && get().followed === run) {
        set({ instanceId: game.instanceId, account: game.account, phase: runningPhase(game) });
      }
    };
    const onEvent = (event: LaunchEvent) => {
      if (event.type === "exited") settleClosing(run);
      // Kept even for a cancelled launch: its game may still come on screen (`windowShown`).
      if (event.type === "started") {
        gameAccount = event.account;
        gameNumber = event.game ?? null;
      }
      const current = get().progress === run;
      switch (event.type) {
        case "progress":
          if (current) set({ phase: { kind: "working", ...event } });
          break;
        case "offline":
          if (!current) break;
          set({ offline: true });
          toast(i18n.t("play.offlineMode"));
          break;
        case "launching":
          if (!current) break;
          set({ phase: { kind: "launching" } });
          tellIfAway(i18n.t("attention.ready.title"), i18n.t("attention.ready.body"));
          break;
        case "started":
          // Cancelled meanwhile: the backend closes it (should its window come first, that
          // event brings it back).
          if (!current) break;
          set({
            games: [
              ...get().games,
              {
                run,
                game: gameNumber,
                instanceId,
                account: gameAccount,
                since: Date.now(),
                shown: false,
                firstStart: event.firstStart,
              },
            ],
          });
          update({});
          setPageDuty(OPENING_DUTY, true);
          break;
        case "windowShown": {
          // Also a game "İptal" could not stop in time: it is on screen, and followed unless
          // another launch is on its way.
          if (!mine()) {
            set({
              games: [
                ...get().games,
                {
                  run,
                  game: gameNumber,
                  instanceId,
                  account: gameAccount,
                  since: Date.now(),
                  shown: true,
                  firstStart: false,
                },
              ],
            });
          }
          if (get().progress === run) set({ progress: 0 });
          if (get().progress === 0) set({ followed: run });
          update({ shown: true, since: Date.now(), firstStart: false, account: gameAccount });
          playSound("success");
          void applyGameStartBehavior(run).finally(() => setPageDuty(OPENING_DUTY, false));
          break;
        }
        case "crashed": {
          if (!mine()) break;
          const { type: _type, ...crash } = event;
          crashed = crash;
          // One crash help at a time: a second waits until the first is dismissed.
          if (get().crash) waitingCrashes.push(crash);
          else showCrash(crash);
          break;
        }
        case "modsBlocked":
          if (!current) break;
          // The game was not started: the player picks "Onar", "Yine de başlat" or "İptal".
          set({ progress: 0 });
          follow(newestGame());
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
          if (!current) break;
          // The pack's mod keys clash in an older options.txt (asked once per session): the
          // game was not started, the player picks "Düzelt" or "Yine de başlat". A launch from
          // the tray or a shortcut brings the window forward for the question.
          set({ progress: 0 });
          follow(newestGame());
          void restoreWindow();
          useModFlow.getState().open(instanceId, "blocked", { kind: "keys", check: event.check });
          break;
        case "javaMismatch": {
          if (!current) break;
          // The custom Java does not suit the game: nothing was downloaded yet; the player picks
          // "Kur ve oyna", "Yine de dene" or cancels (features/java). A launch from the tray or a
          // shortcut brings the window forward for the question.
          const { type: _type, ...prompt } = event;
          set({ progress: 0 });
          follow(newestGame());
          void restoreWindow();
          useJavaPrompt.getState().open({ prompt, options });
          break;
        }
        case "newScreenshots":
          if (!mine()) break;
          shots = {
            instanceId,
            count: event.count,
            recent: event.recent,
            newest: event.newest,
          };
          break;
        case "screenshotNotCopied":
          if (!mine()) break;
          // "Ekran görüntülerini panoya kopyala" failed during the game (told once per game).
          announceShotNotCopied();
          break;
        case "exited": {
          const game = mine();
          // A game "İptal" closed while it opened: the bar went back already.
          if (!game) break;
          const games = get().games.filter((g) => g.run !== run);
          set({ games });
          if (get().progress === run) set({ progress: 0 });
          if (get().followed === run) follow(newestGame());
          setPageDuty(
            OPENING_DUTY,
            games.some((g) => !g.shown),
          );
          if (shots) set({ newShots: { ...shots, at: Date.now() } });
          shots = null;
          const onScreen = games.some((g) => g.shown);
          // "Kapansın" left the game alone: the launcher does not come back for its end.
          if (leftForGame) {
            if (!onScreen) leftForGame = false;
          } else if (!onScreen) {
            // The last game ended: the launcher comes back.
            void restoreWindow();
          } else if (crashed) {
            // Another game is on screen: the launcher does not jump over it; the crash help
            // waits behind a notification that brings it.
            void notify(crashed.title, i18n.t("playAgain.crashBody"), () => void restoreWindow());
          }
          if (unseenNotes) {
            announcePack(unseenNotes, unseenNotes.name, unseenNotes.version);
            unseenNotes = null;
          }
          break;
        }
        case "error":
          if (!current) break;
          set({
            progress: 0,
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
          // Stopped from somewhere else than `cancel` (it already moved on): no message.
          if (!current) break;
          set({ progress: 0 });
          follow(newestGame());
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
        case "worldFromTitle":
          // The game opens on its main menu instead of the world (OVL-FIX): what to do, and why
          // under it.
          toast.info(i18n.t("worldEntry.fromTitle"), {
            // A mod's name never breaks across lines.
            description: i18n.t("worldEntry.fromTitleWhy", {
              mods: event.mods.map((name) => name.replaceAll(" ", " ")).join(", "),
            }),
            // No word left alone on the last line (design.md: paragraphs wrap `pretty`).
            classNames: { description: "text-pretty" },
          });
          break;
      }
    };
    try {
      await api.launch(instanceId, onEvent, options);
    } catch {
      // Errors also arrive as events; anything else (cancel, transport) moves on.
      if (get().progress === run && get().phase.kind !== "error") {
        set({ progress: 0 });
        follow(newestGame());
      }
    }
  },
  async cancel() {
    const { instanceId, phase, progress } = get();
    // A game on screen is never closed from the launcher.
    if (!progress || (phase.kind === "running" && phase.shown)) return;
    const opening = get().games.find((game) => game.run === progress);
    if (opening) {
      // The game opens: the backend closes it; the next launch waits for its exit.
      let settle = () => {};
      const exited = new Promise<void>((resolve) => {
        settle = resolve;
      });
      const waited = new Promise<void>((resolve) => setTimeout(resolve, CLOSE_WAIT_MS));
      closing = { run: progress, done: Promise.race([exited, waited]), settle };
      set({ games: get().games.filter((game) => game !== opening) });
      setPageDuty(OPENING_DUTY, false);
    }
    // Calm and at once: the bar moves on right away and nothing the stopping launch still
    // reports brings it back (the launcher stops its downloads within a second).
    set({ progress: 0 });
    follow(newestGame());
    if (instanceId) await api.cancelLaunch(instanceId);
  },
  dismissCrash: () => {
    set({ crash: null });
    const next = waitingCrashes.shift();
    if (next) showCrash(next);
  },
  showPackNotes: (packNotes) => set({ packNotes }),
  clearNewShots: () => set({ newShots: null }),
  reset: () => {
    if (get().progress) return;
    follow(newestGame());
  },
}));

/** Whether the selected account has a launch or a game (the rules' `isMine`), for code outside React. */
export function ownBusy(instanceId?: string): boolean {
  const { phase, account, games, selected, instanceId: launching } = useLaunch.getState();
  if (inProgress(phase) && isMine(account, selected)) {
    return instanceId === undefined || launching === instanceId;
  }
  return games.some(
    (game) =>
      game.shown &&
      isMine(game.account, selected) &&
      (instanceId === undefined || game.instanceId === instanceId),
  );
}
