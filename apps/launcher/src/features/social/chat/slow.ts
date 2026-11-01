// "Yavaş mod" (CHAT-CALM), pure rules (slow.test.ts): a crew's leader or one of its moderators lets
// each member write at most once every few seconds in the crew's chat; they never wait themselves.
// The composer counts the wait down from the player's newest message there (sent, or on its way;
// one that did not go does not count). The service decides anyway: this only shows the wait.
import type { TFunction } from "i18next";
import type { ChatMessage, SocialTeam } from "@/lib/api/social-types";

/** The steps a leader picks from (seconds; 0 is off), as the service takes them. Up to half a
 * minute: older phones read a longer wait as the day's limit. */
export const SLOW_STEPS = [0, 5, 10, 30] as const;

export type SlowStep = (typeof SLOW_STEPS)[number];

/** The crew's slow mode for the player (seconds): 0 for its leader and moderators, and off. */
export function slowFor(crew: Pick<SocialTeam, "role" | "moderator" | "slow"> | undefined): number {
  if (!crew || crew.role === "founder" || crew.moderator) return 0;
  return crew.slow ?? 0;
}

/** When the player may write again (Unix ms), or null: a while after their newest message. */
export function slowUntil(
  messages: readonly Pick<ChatMessage, "mine" | "state" | "at">[] | undefined,
  slow: number,
): number | null {
  if (slow <= 0 || !messages) return null;
  let newest = Number.NEGATIVE_INFINITY;
  for (const message of messages) {
    if (message.mine && message.state !== "failed" && message.at > newest) newest = message.at;
  }
  return Number.isFinite(newest) ? newest + slow * 1000 : null;
}

/** Whole seconds left (rounded up); 0 once the player may write. */
export function slowLeft(until: number | null, nowMs: number): number {
  return until === null ? 0 : Math.max(0, Math.ceil((until - nowMs) / 1000));
}

/** A step as a label says it: "Kapalı", "5 sn" (the short units of `units`). */
export function slowWords(t: TFunction, seconds: number): string {
  if (seconds <= 0) return t("chatCalm.slow.off");
  return t("units.seconds", { seconds });
}

/** A step as a sentence says it: "30 saniye" (design.md §7: whole words). */
export function slowSpan(t: TFunction, seconds: number): string {
  return t("chatCalm.slow.spanSeconds", { count: seconds });
}
