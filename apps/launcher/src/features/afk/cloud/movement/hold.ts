// A direction button's press (README "Movement" → `step`): a tap steps one block; held, it asks for
// eight blocks at once and again every 1.5 seconds, and letting go stops it. The same for the panel's
// W, A, S and D keys. One direction at a time: pressing another ends the first.
import { type MoveDir, type MoveOp, STEP_MAX } from "./model";

/** A press longer than this is a hold (a tap is shorter: the game's click). */
export const HOLD_MS = 320;
/** A held direction asks for eight blocks again this often (the contract's rhythm). */
export const HOLD_REPEAT_MS = 1500;

export interface Timers {
  set: (run: () => void, ms: number) => number;
  clear: (handle: number) => void;
}

const windowTimers: Timers = {
  set: (run, ms) => window.setTimeout(run, ms),
  clear: (handle) => window.clearTimeout(handle),
};

export interface Hold {
  /** A direction went down (a key repeat or the same button again does nothing). */
  press: (dir: MoveDir) => void;
  /** It came up: a tap steps one block, a hold stops. */
  release: (dir?: MoveDir) => void;
  /** The press was lost (the window lost focus, the panel closed): a hold stops, a tap is dropped. */
  cancel: () => void;
  /** The direction held now, if any. */
  readonly active: MoveDir | null;
}

export function createHold(send: (op: MoveOp) => void, timers: Timers = windowTimers): Hold {
  let dir: MoveDir | null = null;
  let holding = false;
  let timer: number | null = null;

  const clear = () => {
    if (timer !== null) timers.clear(timer);
    timer = null;
  };
  const end = (tap: boolean) => {
    if (!dir) return;
    const was = dir;
    clear();
    if (holding) send({ action: "stop" });
    else if (tap) send({ action: "step", dir: was, n: 1 });
    dir = null;
    holding = false;
  };
  const repeat = (held: MoveDir) => {
    timer = timers.set(() => {
      send({ action: "step", dir: held, n: STEP_MAX });
      repeat(held);
    }, HOLD_REPEAT_MS);
  };

  return {
    press(next) {
      if (dir === next) return;
      if (dir) end(true);
      dir = next;
      holding = false;
      timer = timers.set(() => {
        holding = true;
        send({ action: "step", dir: next, n: STEP_MAX });
        repeat(next);
      }, HOLD_MS);
    },
    release(which) {
      if (which && which !== dir) return;
      end(true);
    },
    cancel() {
      end(false);
    },
    get active() {
      return dir;
    },
  };
}
