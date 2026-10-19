// When the home's status notice asks the servers again while it is on screen (ST1): every few
// minutes, and once more shortly before it would expire. That look is the last one: when the bot
// renews the notice its `expiresAt` changes and the plan starts over; when it does not, the
// notice leaves by itself at its expiry. Pure, so the timing is tested without timers.

/** While a status notice is on screen the launcher looks again this often... */
export const RECHECK_MS = 5 * 60_000;
/** ...and once more this long before it would expire (the bot renews it while it lasts). */
export const RECHECK_BEFORE_EXPIRY_MS = 60_000;
/** Never sooner than this after the last look (or after the notice came). */
export const RECHECK_MIN_MS = 30_000;

export interface Look {
  /** Milliseconds from `now` to the look. */
  wait: number;
  /** The look before expiry: nothing is planned after it. */
  last: boolean;
}

/**
 * The next look for a notice that expires at `end`, `now` being the last look or the notice's
 * arrival; `null` when the notice leaves before a look could be made. A regular look that would
 * come within `RECHECK_MIN_MS` of the one before expiry is that one, and whether a look is the
 * last is decided here, not by the clock when its timer fires (a timer can fire a few
 * milliseconds early by the wall clock).
 */
export function nextLook(end: number, now: number): Look | null {
  if (!(end - now > RECHECK_MIN_MS)) return null;
  const toLast = end - RECHECK_BEFORE_EXPIRY_MS - now;
  if (toLast < RECHECK_MS + RECHECK_MIN_MS) {
    return { wait: Math.max(RECHECK_MIN_MS, toLast), last: true };
  }
  return { wait: RECHECK_MS, last: false };
}
