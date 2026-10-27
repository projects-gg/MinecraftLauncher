// Whose the launch is (PLAY-ACCT; owner, 26 Oct 2026: with another account selected, OYNA said
// "Oyunda" while a game ran, though that account was not in game). The game knows the account it
// plays with from its start (`started`, and the note of a game an earlier start left running), so
// OYNA says "Oyunda" for that account alone and the play action for every other one.
//
// Several games at once (PLAY-AGAIN; owner, 26 Oct 2026: "Oyunda" can be pressed, and after "Bu
// adla oyun zaten açık. Yine de bir tane daha açılsın mı?" one more game opens with the same
// account). A launch on its way (files prepared, the game opening) stays one at a time; games on
// screen may be many: another account's starts beside them without a question, the same name asks
// first. Pure rules, tested in launch-owner.test.ts.
import type { GameAccount } from "@/lib/api";
import type { LaunchPhase, RunningGame } from "./launch";

/** How far a launch is: its files are prepared, the game opens, or its window is on screen. */
export type GameStage = "preparing" | "opening" | "open";

export function gameStage(phase: LaunchPhase): GameStage | null {
  switch (phase.kind) {
    case "working":
      return "preparing";
    case "launching":
      return "opening";
    case "running":
      return phase.shown ? "open" : "opening";
    default:
      return null;
  }
}

/** The launch is on its way: its files are prepared or its game opens (no window yet). */
export function inProgress(phase: LaunchPhase): boolean {
  const stage = gameStage(phase);
  return stage === "preparing" || stage === "opening";
}

/** What the rules read of the launch store. */
export interface LaunchView {
  /** The launch the store follows: the one on its way, else the newest game. */
  phase: LaunchPhase;
  instanceId: string | null;
  account: GameAccount | null;
  /** Every game that runs, oldest first. */
  games: readonly RunningGame[];
}

/** Whose the launch is, seen from the selected account. */
export type LaunchOwner = "none" | "mine" | "other";

/**
 * Whether a game (or a launch) with `account` counts as the selected account's (`selected`: its
 * id; `undefined` while the accounts load, `null` with none). A game whose account is not known
 * (an older launcher left it running), or with no account (yet) to tell, counts as the selected
 * one's: there is no other account to play with.
 */
export function isMine(account: GameAccount | null, selected: string | null | undefined): boolean {
  return !account?.id || !selected || account.id === selected;
}

/**
 * Whose the launch is for the selected account. Files being prepared are every account's: the
 * game starts with the account selected when it starts, as before (unless the launch named one).
 * From its start the game is its account's alone.
 */
export function launchOwner(
  phase: LaunchPhase,
  account: GameAccount | null,
  selected: string | null | undefined,
): LaunchOwner {
  if (gameStage(phase) === null) return "none";
  return isMine(account, selected) ? "mine" : "other";
}

/** The games of the selected account whose window is on screen, oldest first. */
export function ownGames(view: LaunchView, selected: string | null | undefined): RunningGame[] {
  return view.games.filter((game) => game.shown && isMine(game.account, selected));
}

/** One phase object per game object (the store replaces a game when it changes). */
const phases = new WeakMap<RunningGame, LaunchPhase>();

/**
 * The phase a running game shows. The same object for the same game, so a view that selects it
 * draws again only when the game changed.
 */
export function runningPhase(game: RunningGame): LaunchPhase {
  let phase = phases.get(game);
  if (!phase) {
    phase = { kind: "running", since: game.since, shown: game.shown, firstStart: game.firstStart };
    phases.set(game, phase);
  }
  return phase;
}

/** What OYNA and the other play actions show for the selected account. */
export interface OwnLaunch {
  /** "other": nothing of this account runs, but other accounts play (or start). */
  owner: LaunchOwner;
  /** The launch's phase for this account: idle while only other accounts' games run. */
  phase: LaunchPhase;
  /** The instance of this account's launch or game (null while it has none). */
  instanceId: string | null;
  /** This account's newest game on screen, when the bar shows one. */
  game: RunningGame | null;
}

const IDLE: LaunchPhase = { kind: "idle" };

/**
 * The selected account's view: its launch on its way (or one nobody's yet: files prepared), else
 * its newest game on screen, else nothing (while other accounts play, "other").
 */
export function ownLaunch(view: LaunchView, selected: string | null | undefined): OwnLaunch {
  // The launch on its way, or the error it just ended with (the bar tells it once, then resets).
  const pending = inProgress(view.phase) || view.phase.kind === "error";
  if (pending && isMine(view.account, selected)) {
    return { owner: "mine", phase: view.phase, instanceId: view.instanceId, game: null };
  }
  const mine = ownGames(view, selected).at(-1);
  if (mine) {
    return { owner: "mine", phase: runningPhase(mine), instanceId: mine.instanceId, game: mine };
  }
  if (inProgress(view.phase) || view.games.length > 0) {
    return { owner: "other", phase: IDLE, instanceId: null, game: null };
  }
  // Nothing runs: the store's own phase (idle, or the error of the last launch).
  return { owner: "none", phase: view.phase, instanceId: view.instanceId, game: null };
}

/** What "Oyna" for an instance does with the selected account. */
export type PlayPlan =
  /** Nothing runs: the launch starts. */
  | "play"
  /** This account's launch of this instance is on its way: OYNA says so already. */
  | "nothing"
  /** Another launch is on its way (one at a time): "Durdur ve oyna" first. */
  | "switch"
  /** This account has a game on screen: "Bu adla oyun zaten açık", then one more. */
  | "again"
  /** Only other accounts' games run: this one starts beside them (memory allowing). */
  | "beside";

export function planPlay(
  view: LaunchView,
  instanceId: string,
  selected: string | null | undefined,
): PlayPlan {
  if (inProgress(view.phase)) {
    return isMine(view.account, selected) && view.instanceId === instanceId ? "nothing" : "switch";
  }
  if (ownGames(view, selected).length > 0) return "again";
  return view.games.length > 0 ? "beside" : "play";
}

/**
 * The accounts whose game is on screen, for the account center's and the account menu's "Oyunda"
 * (only once a window shows, as OYNA says it; a game whose account is not known names none).
 */
export function playingAccountIds(view: LaunchView): string[] {
  const ids: string[] = [];
  for (const game of view.games) {
    const id = game.account?.id;
    if (game.shown && id && !ids.includes(id)) ids.push(id);
  }
  return ids;
}

/**
 * The names of the other accounts in game while the selected account has nothing running: the
 * command bar's quiet "X oyunda" (PLAY-AGAIN). Empty when the selected account plays or nobody
 * else does.
 */
export function othersInGame(view: LaunchView, selected: string | null | undefined): string[] {
  if (!selected || ownLaunch(view, selected).owner === "mine") return [];
  const names: string[] = [];
  for (const game of view.games) {
    const name = game.account?.name;
    if (game.shown && game.account?.id !== selected && name && !names.includes(name)) {
      names.push(name);
    }
  }
  return names;
}
