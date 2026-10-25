// Moves on their way. Every move is sent at once and shown waiting until the machine answers and a
// frame shows what the server made of it; a refusal puts the view back and says one short line.
// Nothing is ever sent again by itself: after "too fast" the view waits out the machine's time and
// says so when the player clicks meanwhile.
import type { Step } from "./plan";
import type * as rules from "./rules";
import type { InventoryTexts } from "./texts";
import type { InvError, InvOp, InvResult } from "./types";

/** The lines a move's answer can bring. */
export type MoveMessage =
  | "slowDown"
  | "refused"
  | "tradeLocked"
  | "viewOnly"
  | "notRunning"
  | "unreachable"
  | "off";

/** No frame this long after an `ok`: the server changed nothing. */
export const SETTLE_MS = 1600;
/** No answer this long (the machine's own limit is 3 s): the account is out of reach. */
export const ANSWER_MS = 5000;

interface Pending {
  id: string;
  step: Step;
  ok: boolean;
  /** A frame came after the move was sent: the view shows the frame, not the guess. */
  framed: boolean;
  timer: ReturnType<typeof setTimeout> | null;
  chain: Chain | null;
}

interface Chain {
  steps: Step[];
  index: number;
}

export interface QueueHooks {
  send: (op: InvOp) => Promise<InvResult>;
  /** The player inventory as the last frame has it (a chain checks its cursor there). */
  truth: () => rules.InvState | null;
  changed: () => void;
  message: (message: MoveMessage) => void;
  speak: (text: keyof InventoryTexts, item: string) => void;
  /** Where the cursor's item came from (a pickup into an empty cursor). */
  origin: (key: string) => void;
  now?: () => number;
}

export function messageFor(error: InvError): MoveMessage | null {
  switch (error.code) {
    case "rate_limited":
      return "slowDown";
    case "busy_trade":
      return "tradeLocked";
    case "not_running":
      return "notRunning";
    case "inventory_off":
      return error.reason === "view" ? "viewOnly" : "off";
    case "host_offline":
    case "not_found":
      return "unreachable";
    default:
      // `stale` and `invalid`: the view simply redraws from the latest frame.
      return null;
  }
}

export class MoveQueue {
  private pending: Pending[] = [];
  private counter = 0;
  private coolUntil = 0;
  private disposed = false;

  constructor(private hooks: QueueHooks) {}

  private now(): number {
    return this.hooks.now?.() ?? Date.now();
  }

  /** Moves waiting, oldest first. */
  get waiting(): readonly Step[] {
    return this.pending.map((p) => p.step);
  }

  /** Whether the machine asked to wait ("too fast"). */
  coolingDown(): boolean {
    return this.now() < this.coolUntil;
  }

  /** The player inventory with the guesses of moves not yet framed, and the slots that wait. */
  shown(base: rules.InvState | null): { state: rules.InvState | null; keys: Set<string> } {
    const keys = new Set<string>();
    let state = base;
    for (const p of this.pending) {
      for (const key of p.step.keys) keys.add(key);
      if (state && p.step.predict && !p.framed) state = p.step.predict(state);
    }
    return { state, keys };
  }

  /** Sends a move or a chain. Returns false when it waits out "too fast" instead. */
  run(steps: Step[]): boolean {
    if (steps.length === 0 || this.disposed) return true;
    if (this.coolingDown()) {
      this.hooks.message("slowDown");
      return false;
    }
    this.send(steps[0] as Step, steps.length > 1 ? { steps, index: 0 } : null);
    return true;
  }

  /** A new frame arrived (inventory or window). */
  frame(): void {
    let changed = false;
    for (const p of [...this.pending]) {
      if (!p.framed) {
        p.framed = true;
        changed = true;
      }
      if (p.ok) this.settle(p, true);
    }
    if (changed) this.hooks.changed();
  }

  /** The view is gone: timers stop, moves on their way are forgotten. */
  dispose(): void {
    this.disposed = true;
    for (const p of this.pending) if (p.timer) clearTimeout(p.timer);
    this.pending = [];
  }

  /** The view is back (React mounts effects twice in development). */
  revive(): void {
    this.disposed = false;
  }

  private send(step: Step, chain: Chain | null) {
    this.counter = (this.counter + 1) % 0x7fffffff;
    const entry: Pending = {
      id: `m${this.counter.toString(36)}`,
      step,
      ok: false,
      framed: false,
      timer: null,
      chain,
    };
    this.pending.push(entry);
    if (step.origin) this.hooks.origin(step.origin);
    this.hooks.changed();
    let answered = false;
    entry.timer = setTimeout(() => {
      if (answered) return;
      answered = true;
      this.answer(entry, { error: { code: "host_offline" } });
    }, ANSWER_MS);
    this.hooks
      .send(step.op)
      .catch((): InvResult => ({ error: { code: "host_offline" } }))
      .then((result) => {
        if (answered) return;
        answered = true;
        this.answer(entry, result);
      });
  }

  private answer(entry: Pending, result: InvResult) {
    if (this.disposed || !this.pending.includes(entry)) return;
    if (entry.timer) clearTimeout(entry.timer);
    entry.timer = null;
    if (result.ok === true) {
      entry.ok = true;
      if (entry.framed) this.settle(entry, true);
      else entry.timer = setTimeout(() => this.settle(entry, false), SETTLE_MS);
      return;
    }
    const error = "error" in result ? result.error : { code: "invalid" };
    if (error.code === "rate_limited") {
      this.coolUntil = this.now() + Math.max(1, error.retryAfter ?? 1) * 1000;
    }
    this.remove(entry);
    const message = messageFor(error);
    if (message) this.hooks.message(message);
  }

  /** The move is over: a frame came after it (`seen`), or none came in time after its `ok`. */
  private settle(entry: Pending, seen: boolean) {
    if (!this.pending.includes(entry)) return;
    this.remove(entry);
    const { step, chain } = entry;
    if (chain) {
      const expected = step.expectCursor;
      const cursor = this.hooks.truth()?.cursor ?? null;
      const matches =
        seen &&
        (expected === undefined ||
          (expected === null
            ? cursor === null
            : cursor !== null && cursor.id === expected.id && cursor.count === expected.count));
      if (!matches) return;
      const next = chain.steps[chain.index + 1];
      if (next) {
        this.send(next, { steps: chain.steps, index: chain.index + 1 });
        return;
      }
    } else if (!seen && step.mustChange) {
      this.hooks.message("refused");
      return;
    }
    if (seen && step.speech) this.hooks.speak(step.speech.text, step.speech.item);
  }

  private remove(entry: Pending) {
    if (entry.timer) clearTimeout(entry.timer);
    entry.timer = null;
    this.pending = this.pending.filter((p) => p !== entry);
    this.hooks.changed();
  }
}
