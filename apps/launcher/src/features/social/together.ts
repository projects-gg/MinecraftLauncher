// "Birlikte oynanan süre" (TEAM5): kept by the launcher on this computer only (pl_social's
// TogetherLog, while the player and a crewmate are on Projects at once), shown on the crew card
// ("Bu hafta birlikte 6 sa 20 dk") and on a member's card ("Bu ay birlikte 3 sa") once it is worth
// saying. Off, nothing is kept and nothing shows.
import { formatDuration } from "@/lib/format";

/** Less than this together says nothing (seconds). */
export const TOGETHER_MIN_SECS = 5 * 60;

/** The time together in whole minutes ("6 sa 20 dk", "45 dk"), or null while it is too little. */
export function togetherTime(seconds: number): string | null {
  if (!(seconds >= TOGETHER_MIN_SECS)) return null;
  return formatDuration(Math.floor(seconds / 60) * 60);
}
