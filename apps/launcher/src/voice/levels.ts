// The live meters' levels to the launcher (VOICE-CALL): the player's microphone and the people who
// make sound now, at most sixteen a second and only when something moved. A change arms one timer
// for the rest of the gap; a quiet room arms nothing (the worklets post nothing then either).
import type { Timers } from "./speaking";

/** The shortest time between two reports (sixteen a second). */
export const LEVELS_GAP_MS = 60;

/** One report: the microphone's level and whether it goes out, and the people's levels. */
export interface LevelsFrame {
  me: number;
  open: boolean;
  ids: string[];
  levels: number[];
}

export class LevelsReporter {
  private me = 0;
  private open = false;
  private people: ReadonlyMap<string, number> = new Map();
  private sent = keyOf(0, false, new Map());
  private lastAt = Number.NEGATIVE_INFINITY;
  private cancel: (() => void) | null = null;

  constructor(
    private readonly send: (frame: LevelsFrame) => void,
    private readonly timers: Timers,
  ) {}

  /** The microphone's level (0: off or quiet) and whether it goes out. */
  setMe(level: number, open: boolean): void {
    this.me = level;
    this.open = open;
    this.arm();
  }

  /** Everyone's level now, person by person (the quiet left out). */
  setPeople(people: ReadonlyMap<string, number>): void {
    this.people = people;
    this.arm();
  }

  /** The stay ended: everything rests, nothing is said (the launcher drops levels without a room). */
  reset(): void {
    this.cancel?.();
    this.cancel = null;
    this.me = 0;
    this.open = false;
    this.people = new Map();
    this.sent = keyOf(0, false, this.people);
  }

  private arm(): void {
    if (this.cancel) return;
    if (keyOf(this.me, this.open, this.people) === this.sent) return;
    const wait = this.lastAt + LEVELS_GAP_MS - this.timers.now();
    if (wait <= 0) this.flush();
    else this.cancel = this.timers.later(() => this.flush(), wait);
  }

  private flush(): void {
    this.cancel = null;
    const key = keyOf(this.me, this.open, this.people);
    if (key === this.sent) return;
    this.sent = key;
    this.lastAt = this.timers.now();
    const ids = [...this.people.entries()]
      .filter(([, level]) => level > 0)
      .map(([id]) => id)
      .sort();
    this.send({
      me: this.me,
      open: this.open,
      ids,
      levels: ids.map((id) => this.people.get(id) ?? 0),
    });
  }
}

function keyOf(me: number, open: boolean, people: ReadonlyMap<string, number>): string {
  const parts = [...people.entries()]
    .filter(([, level]) => level > 0)
    .map(([id, level]) => `${id}:${level}`)
    .sort();
  return `${me}|${open ? 1 : 0}|${parts.join(",")}`;
}
