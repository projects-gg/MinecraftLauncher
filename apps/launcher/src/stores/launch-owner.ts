// Whose the launch is (PLAY-ACCT; owner, 26 Oct 2026: with another account selected, OYNA said
// "Oyunda" while a game ran, though that account was not in game). The game knows the account it
// plays with from its start (`started`, and the note of a game an earlier start left running), so
// OYNA says "Oyunda" for that account alone and the play action for every other one. One game at
// a time stays: "Oyna" for another account, or another installation, asks to close (or stop) the
// game first (features/game-switch). Pure rules, tested in launch-owner.test.ts.
import type { GameAccount } from "@/lib/api";
import type { LaunchPhase } from "./launch";

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

/** Whose the launch is, seen from the selected account. */
export type LaunchOwner = "none" | "mine" | "other";

/**
 * Whose the launch is for the selected account (`selected`: its id; `undefined` while the
 * accounts load, `null` with none). Files being prepared are every account's: the game starts
 * with the account selected when it starts, as before. From its start the game is its account's
 * alone. A game whose account is not known (an older launcher left it running), or with no
 * account (yet) to tell, counts as the selected one's: there is no other account to play with.
 */
export function launchOwner(
  phase: LaunchPhase,
  account: GameAccount | null,
  selected: string | null | undefined,
): LaunchOwner {
  if (gameStage(phase) === null) return "none";
  if (!account?.id || !selected) return "mine";
  return account.id === selected ? "mine" : "other";
}

/** What "Oyna" for an instance does with the selected account. */
export type PlayPlan =
  /** Nothing runs: the launch starts. */
  | "play"
  /** This account's game of this instance is on, or on its way: OYNA says so already. */
  | "nothing"
  /** Another account's game, or another instance's, runs: the question first. */
  | "switch";

export function planPlay(
  launch: { phase: LaunchPhase; instanceId: string | null; account: GameAccount | null },
  instanceId: string,
  selected: string | null | undefined,
): PlayPlan {
  const owner = launchOwner(launch.phase, launch.account, selected);
  if (owner === "none") return "play";
  if (owner === "mine" && launch.instanceId === instanceId) return "nothing";
  return "switch";
}

/**
 * The account whose game is on screen, for the account center's and the account menu's
 * "Oyunda" (only once its window shows, as OYNA says it; none while it opens or when its
 * account is not known).
 */
export function playingAccountId(phase: LaunchPhase, account: GameAccount | null): string | null {
  return gameStage(phase) === "open" && account?.id ? account.id : null;
}
