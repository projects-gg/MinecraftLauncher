// A limited account ("Sınırlı hesap", SPAM1) and moderators' measures (MOD1), pure rules
// (limits.test.ts): what is limited right now, when a limit opens again, how dates are said.
import type { TFunction } from "i18next";
import type { LimitedKey, SocialMe } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";

/** Where the community rules are (the restriction screen and the warning link to them). */
export const COMMUNITY_RULES_URL = "https://mc.projects.gg/sozlesme";

/** One of the account's limits while it lasts (`until` Unix seconds), else null. */
export function limitOf(
  me: Pick<SocialMe, "limited"> | undefined,
  key: LimitedKey,
  nowMs: number,
): { until: number; perDay?: number } | null {
  const limited = me?.limited;
  if (!limited || limited.until * 1000 <= nowMs) return null;
  const item = limited.items.find((one) => one.key === key);
  return item ? { until: limited.until, perDay: item.perDay } : null;
}

/** The account is limited right now (the chip shows). */
export function isLimited(me: Pick<SocialMe, "limited"> | undefined, nowMs: number): boolean {
  return Boolean(me?.limited && me.limited.until * 1000 > nowMs && me.limited.items.length);
}

/** "3 gün sonra açılır." / "5 saat sonra açılır." / "Birazdan açılır.". */
export function opensIn(t: TFunction, untilSec: number, nowMs: number): string {
  const left = untilSec * 1000 - nowMs;
  if (left <= 3_600_000) return t("safety.limited.opensSoon");
  if (left < 86_400_000) {
    return t("safety.limited.opensHours", { count: Math.ceil(left / 3_600_000) });
  }
  return t("safety.limited.opensDays", { count: Math.ceil(left / 86_400_000) });
}

/** "28 Ekim" (with the year when it is not this one), in the interface's language. */
export function dayMonth(unixSec: number, nowMs: number = Date.now()): string {
  const at = new Date(unixSec * 1000);
  const sameYear = at.getFullYear() === new Date(nowMs).getFullYear();
  return dateTimeFormat(
    sameYear
      ? { day: "numeric", month: "long" }
      : { day: "numeric", month: "long", year: "numeric" },
  ).format(at);
}
