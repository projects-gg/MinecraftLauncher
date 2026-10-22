// "Bağlan" (FIX2): joining a friend's or a crew's server while a game is open. The question comes
// once; then the overlay may take the player there in game (OVL2, at most five seconds), else the
// game closes the way its own close button does (it leaves its server, a singleplayer world is
// saved) and starts again straight into the server. A game that does not close by itself is ended
// only at the player's word, after one sentence about what may be lost. Pure rules (tested in
// connect.test.ts); the dialog and its steps live in join-dialog.tsx.
import type { LaunchPhase } from "@/stores/launch";

/** How long the game has to close by itself before the player may end it. */
export const CLOSE_GRACE_MS = 20_000;
/** An end the player asked for that has not come by then goes back to the question. */
export const FORCE_GRACE_MS = 8_000;
/** The overlay answers within five seconds (the shell's limit); past this it counts as a no. */
export const IN_GAME_LIMIT_MS = 7_000;

/** The game as "Katıl" finds it: closed, on screen, or being prepared or opening. */
export type GameNow = "closed" | "open" | "opening";

export function gameNow(phase: LaunchPhase): GameNow {
  if (phase.kind === "idle" || phase.kind === "error") return "closed";
  return phase.kind === "running" && phase.shown ? "open" : "opening";
}

/** Where the player goes after "Bağlan", step by step. */
export type CloseStep =
  /** The overlay takes the player to the server in game (it answers within five seconds). */
  | { kind: "inGame" }
  /** The game was asked to close (`since`, ms); it starts again once it has ended. */
  | { kind: "closing"; since: number }
  /** It has not closed within `CLOSE_GRACE_MS`, or nothing of it could be asked. */
  | { kind: "overdue" }
  /** The player ended it (`since`, ms). */
  | { kind: "forcing"; since: number }
  /** A start that had not opened yet is stopped (nothing to save). */
  | { kind: "stopping" };

export type CloseEvent =
  | { type: "inGame"; done: boolean; now: number }
  | { type: "asked"; asked: boolean }
  | { type: "tick"; now: number }
  | { type: "wait"; now: number }
  | { type: "force"; now: number }
  | { type: "exited" };

/** The next step, or where it ends: in the server already ("connected"), or start the game. */
export type CloseNext = CloseStep | "connected" | "start";

export function closeNext(step: CloseStep, event: CloseEvent): CloseNext {
  // However it went, a game that ended starts again into the server.
  if (event.type === "exited") return "start";
  switch (step.kind) {
    case "inGame":
      if (event.type !== "inGame") return step;
      return event.done ? "connected" : { kind: "closing", since: event.now };
    case "closing":
      if (event.type === "asked") return event.asked ? step : { kind: "overdue" };
      if (event.type === "tick" && event.now - step.since >= CLOSE_GRACE_MS) {
        return { kind: "overdue" };
      }
      return step;
    case "overdue":
      if (event.type === "wait") return { kind: "closing", since: event.now };
      if (event.type === "force") return { kind: "forcing", since: event.now };
      return step;
    case "forcing":
      // It did not end: the question again (the player may try once more, or leave it).
      if (event.type === "tick" && event.now - step.since >= FORCE_GRACE_MS) {
        return { kind: "overdue" };
      }
      return step;
    case "stopping":
      return step;
  }
}
