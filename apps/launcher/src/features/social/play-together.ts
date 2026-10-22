// "Birlikte oyna" (play together), pure rules (tested in play-together.test.ts): when the player
// may invite, where an invite's "Katıl" connects and what pressing it does. The dialog lives in
// join-dialog.tsx, the launch in join.ts.
//
// An invite carries the inviter's server (the friends service adds it when it is sent): projects.gg
// always, another server only while the inviter shares its address with friends. So inviting is
// offered only from a server the invite will carry; without one the friend would get an invite
// with nowhere to join.
import { parseServerAddress } from "@/features/library/quick-play";
import type { SocialActivity, SocialGame } from "@/lib/api/social-types";
import type { GameNow } from "./connect";
import { isProjectsHost } from "./model";

/** "Birlikte oyna" is offered: on projects.gg always, on another server while its address is shared. */
export function canInvite(activity: SocialActivity, shareServer: boolean): boolean {
  if (activity.state !== "playing" || !activity.host) return false;
  return isProjectsHost(activity.host) || shareServer;
}

/** Where an invite's "Katıl" connects: projects.gg, or another server's address. */
export type JoinTarget =
  | { kind: "projects" }
  | { kind: "server"; host: string; port: number | null; address: string };

export function joinTarget(server: SocialGame | undefined): JoinTarget | null {
  if (!server || server.where === "singleplayer") return null;
  if (server.where === "projects" || isProjectsHost(server.host)) return { kind: "projects" };
  if (!server.host) return null;
  const parsed = parseServerAddress(server.host);
  return parsed ? { kind: "server", ...parsed, address: server.host } : null;
}

/** How the target is named to the player: "Projects", or the server's address. */
export function targetName(target: JoinTarget): string {
  return target.kind === "projects" ? "Projects" : target.address;
}

/** The address a player types in the game's "Doğrudan Bağlan". */
export function targetAddress(target: JoinTarget): string {
  return target.kind === "projects" ? "play.projects.gg" : target.address;
}

/** The player is already on `target` (their own game, as the friends service sees it). */
export function onTarget(target: JoinTarget, activity: SocialActivity): boolean {
  if (activity.state !== "playing" || !activity.host) return false;
  if (target.kind === "projects") return isProjectsHost(activity.host);
  const here = parseServerAddress(activity.host);
  return (
    here !== null &&
    here.host.toLowerCase() === target.host.toLowerCase() &&
    (here.port ?? 25565) === (target.port ?? 25565)
  );
}

/**
 * What "Katıl" on an invite or a call does (FIX2 "Bağlan"): nothing when the player is already
 * there; with the game closed, ask once and start it straight into the server; with a game open,
 * ask once and take it there (the overlay in game, else the game closes the way its close button
 * does, so nothing is lost, and starts again into the server); while a game is prepared or opens,
 * ask once and start again into the server (nothing to save yet).
 */
export type JoinStep = "already" | "confirm" | "restart" | "reopen";

export function joinStep(target: JoinTarget, game: GameNow, activity: SocialActivity): JoinStep {
  if (game === "closed") return "confirm";
  if (game === "opening") return "reopen";
  return onTarget(target, activity) ? "already" : "restart";
}

/**
 * The instance "Katıl" starts for another server: the one OYNA would start (the selected one),
 * or null when there is none (the caller falls back to the Projects instance, as OYNA does).
 * Invites carry no game version, so nothing better can be picked.
 */
export function playInstance(
  instances: ReadonlyArray<{ id: string }>,
  selected: string | null,
): string | null {
  return selected && instances.some((i) => i.id === selected) ? selected : null;
}

/**
 * The instance "Bağlan" starts again after the game closed (FIX2): the one that ran, where it can
 * go: another server takes the instance the player was playing; projects.gg only one that joins
 * it by itself (the Projects instance, or the player's own set to join it). Null: the instance
 * "Katıl" picks with the game closed.
 */
export function restartInstance(
  target: JoinTarget,
  running: string | null,
  instances: ReadonlyArray<{ id: string; autoJoin?: string }>,
): string | null {
  const instance = running ? instances.find((i) => i.id === running) : undefined;
  if (!instance) return null;
  if (target.kind === "server") return instance.id;
  return isProjectsHost(instance.autoJoin) ? instance.id : null;
}
