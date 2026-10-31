// The floating day line (CHAT-UX2): while the player reads back, the day of the first row under the
// list's top edge floats there on a quiet plate ("Dün", "3 gün önce", "19 Ekim"), unless that
// row's own day line shows there already; at the newest messages nothing floats. It is read from
// the rows' places when the list scrolls or changes, never on a timer, and the plate sits over the
// list (nothing moves under it). Pure (floating-day.test.ts).

/** A row as the floating line reads it: its top edge (window px), its time, and whether it starts
 * a day (carries the day's line at its top). */
export interface DayRow {
  top: number;
  at: number;
  start: boolean;
}

/** A row's own day line this far over the edge (px) still shows, so nothing floats over it. */
export const LINE_SLACK = 14;

/** The first of `count` rows (in their order) whose bottom is under `edge`, or -1; `bottomOf`
 * measures one row, a handful of times for a long list. */
export function firstRowUnder(
  count: number,
  bottomOf: (index: number) => number,
  edge: number,
): number {
  let low = 0;
  let high = count - 1;
  let found = -1;
  while (low <= high) {
    const middle = (low + high) >> 1;
    if (bottomOf(middle) > edge) {
      found = middle;
      high = middle - 1;
    } else {
      low = middle + 1;
    }
  }
  return found;
}

/** The moment whose day floats over the list's top edge, or null (at the newest messages, or over
 * a day line that shows). */
export function floatingDayAt(
  row: DayRow | null,
  edge: number,
  readingBack: boolean,
): number | null {
  if (!readingBack || !row) return null;
  if (row.start && row.top >= edge - LINE_SLACK) return null;
  return row.at;
}
