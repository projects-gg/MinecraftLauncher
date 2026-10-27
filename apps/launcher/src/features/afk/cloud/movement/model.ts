// Movement of a cloud account (CAFK-FEAT §3.7, services/cloud-afk README "Movement"): what the
// "Hareket" panel holds from the live frames (`movst`, `pos`) and how it words the contract's
// states, refusals and ends (its "Texts in the apps" table, the same words as the phone). Pure, so
// the panel and its tests share it.
import type { CloudLiveEvent, CloudMovementError, CloudMovementState, CloudPos } from "@/bindings";

export type MoveDir = "forward" | "back" | "left" | "right";

/** The contract's `MoveOp`, as the panel sends it (Rust checks its shape again). */
export type MoveOp =
  | { action: "step"; dir: MoveDir; n: number }
  | { action: "jump" }
  | { action: "sneak"; on: boolean }
  | { action: "goto"; x: number; y: number; z: number }
  | { action: "approach"; name: string }
  | { action: "follow"; name: string }
  | { action: "stop" };

export type MoveResult = { ok: true } | { ok: false; error: CloudMovementError };

/** A step is 1 to 8 blocks; a held direction asks for 8 (the contract's fixed rules). */
export const STEP_MAX = 8;
/** A walk to coordinates goes at most this far. */
export const GOTO_MAX_BLOCKS = 256;
const COORD_MAX = 30_000_000;
const Y_MIN = -2048;
const Y_MAX = 2048;

export interface MovementState {
  /** The machine's word (`movst`); null until it comes after a watch. */
  move: CloudMovementState | null;
  /** Where the account stands (`pos`); only while it can move. */
  pos: CloudPos | null;
}

export const WAITING: MovementState = { move: null, pos: null };

/** The panel's state after one live event of account `acct` (others leave it as it is). */
export function reduceMovement(
  state: MovementState,
  event: CloudLiveEvent,
  acct: string,
): MovementState {
  switch (event.kind) {
    case "movementState":
      if (event.acct !== acct) return state;
      return { move: event.state, pos: event.state.state === "ok" ? state.pos : null };
    case "pos":
      if (event.acct !== acct) return state;
      return { ...state, pos: event.pos };
    default:
      return state;
  }
}

/** Whether the account walks somewhere or follows someone now: "Dur" shows. */
export function isGoing(move: CloudMovementState | null): boolean {
  return (
    move?.state === "ok" &&
    (move.doing === "step" ||
      move.doing === "goto" ||
      move.doing === "approach" ||
      move.doing === "follow")
  );
}

/** A coordinate as typed: a whole number in the contract's range for its axis, else null. */
export function coordinate(text: string, axis: "x" | "y" | "z"): number | null {
  const trimmed = text.trim().replace(/^[−–]/, "-");
  if (!/^-?\d{1,8}$/.test(trimmed)) return null;
  const value = Number(trimmed);
  if (axis === "y") return value >= Y_MIN && value <= Y_MAX ? value : null;
  return Math.abs(value) <= COORD_MAX ? value : null;
}

/**
 * How far a block is from where the account stands (block to block, both at their centre): the
 * machine's own check (`too_far` beyond 256), so the panel says it before sending.
 */
export function distance(
  pos: Pick<CloudPos, "x" | "y" | "z">,
  to: { x: number; y: number; z: number },
) {
  return Math.hypot(to.x - pos.x, to.y - pos.y, to.z - pos.z);
}

export type WordKey = { key: string; values?: Record<string, string | number> };

/** Why it cannot move now (`movst` off, or `movement_off`'s reason); `cloudAfkMove.<key>`. */
export function offKey(reason: string | null | undefined): string {
  switch (reason) {
    case "not_running":
      return "notRunning";
    case "setting":
      return "off";
    case "restart":
      return "restart";
    case "slots":
      return "noSlot";
    case "version":
      return "version";
    default:
      return "serviceOff";
  }
}

/** Why the switch cannot be turned on now (`features.movement.slot`, `limit_movement`). */
export function slotKey(scope: string | null | undefined, max?: number | null): WordKey {
  if (scope !== "player") return { key: "noSlot" };
  return max && max > 1 ? { key: "maxAccounts", values: { max } } : { key: "oneAccount" };
}

/** "Ana hesabın yakında değil." or, for another own account, "{name} yakında değil." */
function notNear(name: string | null | undefined, main: string | null | undefined): WordKey {
  return !name || name.toLowerCase() === (main ?? "").toLowerCase()
    ? { key: "notNear" }
    : { key: "notNearNamed", values: { name } };
}

/**
 * A refused operation in one line; null when there is nothing to say (`stale`, `invalid`: the next
 * frames redraw). `name`: the account an approach or a follow went to; `main` the main name.
 */
export function refusalKey(
  error: CloudMovementError,
  name: string | null | undefined,
  main: string | null | undefined,
): WordKey | null {
  switch (error.code) {
    case "stale":
    case "invalid":
      return null;
    case "movement_off":
      return { key: offKey(error.reason) };
    case "not_running":
      return { key: "notRunning" };
    case "busy_trade":
      return { key: "trade" };
    case "too_far":
      return { key: "far" };
    case "not_allowed":
      return { key: "notAllowed" };
    case "not_visible":
      return notNear(name, main);
    case "blocked":
      return { key: "blocked" };
    case "loop":
      return { key: "loop" };
    case "rate_limited":
      return { key: "slow" };
    case "host_offline":
      return { key: "machineAway" };
    default:
      return { key: "unreachable" };
  }
}

/** How the last walk or follow ended by itself (`movst` `ended`); null for none (or a stop). */
export function endedKey(
  ended: string | null | undefined,
  name: string | null | undefined,
  main: string | null | undefined,
): WordKey | null {
  switch (ended) {
    case "arrived":
      return { key: "arrived" };
    case "blocked":
      return { key: "blocked" };
    case "no_path":
      return { key: "noPath" };
    case "timeout":
      return { key: "timeout" };
    case "limit":
      return { key: "limit" };
    case "out_of_sight":
      return notNear(name, main);
    case "reset":
      return { key: "reset" };
    case "service":
      return { key: "serviceOff" };
    case "trade":
      return { key: "tradeStarted" };
    default:
      return null;
  }
}

/** What it is doing now, in one line; null while it stands (or before the first word). */
export function doingKey(move: CloudMovementState | null): WordKey | null {
  if (move?.state !== "ok") return null;
  if (move.lock === "trade") return { key: "trade" };
  if (move.doing === "goto" && move.to)
    return { key: "going", values: { x: move.to.x, y: move.to.y, z: move.to.z } };
  if (move.doing === "approach" && move.name)
    return { key: "approaching", values: { name: move.name } };
  if (move.doing === "follow" && move.name)
    return { key: "following", values: { name: move.name } };
  return null;
}

/** The dimension's name key, or the id's last part for a server's own (`cloudAfkMove.dims.*`). */
export function dimension(dim: string): { key: string } | { text: string } {
  switch (dim) {
    case "minecraft:overworld":
      return { key: "overworld" };
    case "minecraft:the_nether":
      return { key: "nether" };
    case "minecraft:the_end":
      return { key: "end" };
    default: {
      const colon = dim.indexOf(":");
      return { text: colon >= 0 ? dim.slice(colon + 1) : dim };
    }
  }
}

/** Where it looks (`cloudAfkMove.facing.*`); an unknown word reads as south, as the game starts. */
export function facingKey(facing: string): "north" | "south" | "east" | "west" {
  return facing === "north" || facing === "east" || facing === "west" ? facing : "south";
}

let next = 0;

/** An operation's own id (`[A-Za-z0-9_-]`, unique among this launcher's pending ones). */
export function moveOpId(): string {
  next = (next + 1) % 1_000_000;
  return `w${Date.now().toString(36)}${next.toString(36)}`;
}
