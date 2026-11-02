// "Who speaks" to the launcher (§6.5): only changes, at most ten a second. A change arms a single
// timer for the rest of the 100 ms; a quiet room arms nothing.

export const SPEAKING_GAP_MS = 100;

export interface Timers {
  now(): number;
  later(run: () => void, ms: number): () => void;
}

export const realTimers: Timers = {
  now: () => performance.now(),
  later(run, ms) {
    const id = setTimeout(run, ms);
    return () => clearTimeout(id);
  },
};

export class SpeakingReporter {
  private ids = new Set<string>();
  private me = false;
  private sentIds = "";
  private sentMe = false;
  private lastAt = Number.NEGATIVE_INFINITY;
  private cancel: (() => void) | null = null;

  constructor(
    private readonly send: (ids: string[], me: boolean) => void,
    private readonly timers: Timers = realTimers,
  ) {}

  /** The people who speak now (ids) and whether the player's microphone sends speech. */
  set(ids: Iterable<string>, me: boolean): void {
    this.ids = new Set(ids);
    this.me = me;
    if (this.cancel) return;
    if ([...this.ids].sort().join("\n") === this.sentIds && me === this.sentMe) return;
    const wait = this.lastAt + SPEAKING_GAP_MS - this.timers.now();
    if (wait <= 0) this.flush();
    else this.cancel = this.timers.later(() => this.flush(), wait);
  }

  /** Nobody speaks any more (the stay ended): said once if something was lit. */
  clear(): void {
    this.cancel?.();
    this.cancel = null;
    this.ids.clear();
    this.me = false;
    if (this.sentIds !== "" || this.sentMe) this.flush();
  }

  private flush(): void {
    this.cancel = null;
    const ids = [...this.ids].sort();
    const key = ids.join("\n");
    if (key === this.sentIds && this.me === this.sentMe) return;
    this.sentIds = key;
    this.sentMe = this.me;
    this.lastAt = this.timers.now();
    this.send(ids, this.me);
  }
}
