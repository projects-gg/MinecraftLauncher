// 7/24 Bulut moderation on the player's side, without React (services/cloud-afk README
// "Moderation player API", "Appeals", "Saving"): the card's one sentence, whether "İtiraz et" is
// offered, a paused row's category, a refused save's words, what "Yanlışsa bildir" sends and the
// moderation notice's line. The same words as afk.projects.gg; never a rule, a pattern, an id or a
// code. Pure, so it is tested on its own.
import type { TFunction } from "i18next";
import type { CloudAccount } from "./api";

const CATEGORIES = new Set([
  "advertising",
  "spam",
  "scam",
  "rmt",
  "impersonation",
  "harassment",
  "hate",
  "sexual",
  "minor_safety",
  "personal_info",
  "off_platform",
  "evasion",
  "automation",
  "other",
]);

const STEPS = new Set(["warn", "restrict_rows", "pause_ai", "suspend"]);

/** A category in the player's words; one this launcher does not know is a rule violation. */
export function categoryText(category: string | null | undefined, t: TFunction): string {
  return t(`afkMod.categories.${category && CATEGORIES.has(category) ? category : "other"}`);
}

export interface ModerationLine {
  text: string;
  /** "İtiraz et" is offered on the line (the site's form). */
  canAppeal: boolean;
  /** The appeal waits for staff. */
  appealOpen: boolean;
}

const NBSP = String.fromCharCode(0xa0);
// Turkish months with the case ending a day takes ("3 Kasım'a kadar", "29 Ekim'de"), as the site.
const MONTH_TO = [
  "Ocak'a",
  "Şubat'a",
  "Mart'a",
  "Nisan'a",
  "Mayıs'a",
  "Haziran'a",
  "Temmuz'a",
  "Ağustos'a",
  "Eylül'e",
  "Ekim'e",
  "Kasım'a",
  "Aralık'a",
];
const MONTH_AT = [
  "Ocak'ta",
  "Şubat'ta",
  "Mart'ta",
  "Nisan'da",
  "Mayıs'ta",
  "Haziran'da",
  "Temmuz'da",
  "Ağustos'ta",
  "Eylül'de",
  "Ekim'de",
  "Kasım'da",
  "Aralık'ta",
];

/**
 * A day for a sentence: Turkish with its case ending ("to": "3 Kasım'a", "at": "29 Ekim'de"),
 * English plain ("3 November"). No year (a deadline or an end within weeks); a no-break space
 * keeps the day and the month together.
 */
export function dayWords(secs: number, locale: string, form: "to" | "at"): string {
  const date = new Date(secs * 1000);
  const month = locale.startsWith("tr")
    ? (form === "to" ? MONTH_TO : MONTH_AT)[date.getMonth()]
    : new Intl.DateTimeFormat(locale, { month: "long" }).format(date);
  return `${date.getDate()}${NBSP}${month}`;
}

/** A moderation case's stop: the state line already says "Durduruldu: <category>". */
export function isModerationStop(account: Pick<CloudAccount, "moderation" | "hold">): boolean {
  return (
    account.moderation?.step === "suspend" &&
    account.hold?.kind === "suspended" &&
    Boolean(account.hold.case)
  );
}

/** The card's main button is "İtiraz et": a moderation stop that may be appealed. */
export function appealFirst(account: Pick<CloudAccount, "moderation" | "hold">): boolean {
  return isModerationStop(account) && Boolean(account.moderation?.appeal.can);
}

/** The card's line while a moderation step holds, or null (observe mode, nothing in force, a stop). */
export function moderationLine(
  account: Pick<CloudAccount, "moderation" | "hold">,
  t: TFunction,
): ModerationLine | null {
  const m = account.moderation;
  // A stop says it on the state line, its details in the line's hint (`stopHint`).
  if (!m || !STEPS.has(m.step) || isModerationStop(account)) return null;
  return {
    text: t(`afkMod.card.${m.step}`, { kind: categoryText(m.category, t) }),
    canAppeal: m.appeal.can,
    appealOpen: m.appeal.state === "open",
  };
}

/**
 * Under a moderation stop's state line: when the account opens again, where the appeal stands and
 * what happens to the days while the decision may still be lifted. Null for anything else.
 */
export function stopHint(
  account: Pick<CloudAccount, "moderation" | "hold">,
  t: TFunction,
  lang: string,
): string | null {
  const m = account.moderation;
  if (!m || !isModerationStop(account)) return null;
  const appeal =
    m.appeal.can && m.appeal.until
      ? t("afkMod.hint.appeal", { date: dayWords(m.appeal.until, lang, "to") })
      : m.appeal.state === "open"
        ? t("afkMod.hint.open")
        : m.appeal.state === "upheld"
          ? t("afkMod.hint.upheld")
          : null;
  const sentences = [
    m.until ? t("afkMod.hint.until", { date: dayWords(m.until, lang, "at") }) : null,
    appeal,
    m.appeal.can || m.appeal.state === "open" ? t("afkMod.hint.days") : null,
  ].filter((sentence): sentence is string => Boolean(sentence));
  return sentences.length ? sentences.join(" ") : null;
}

/** The rows a case paused, by id, with their category (the config view's `paused`). */
export function pausedRows(
  account: Pick<CloudAccount, "config"> | null | undefined,
): Map<string, string> {
  const out = new Map<string, string>();
  for (const row of account?.config.commands ?? []) {
    if (row.paused) out.set(row.id, row.paused.category);
  }
  for (const rule of account?.config.rules ?? []) {
    if (rule.paused) out.set(rule.id, rule.paused.category);
  }
  return out;
}

/** An issue's problem a content refusal carries (pl-cloud-afk: "content:<category>"). */
export function refusedCategory(problem: string): string | null {
  return problem.startsWith("content:") ? problem.slice("content:".length) : null;
}

/** A refused save in the player's words, the category inside the sentence. */
export function refusedText(category: string, t: TFunction, locale: string): string {
  return t("afkMod.refused", { kind: categoryText(category, t).toLocaleLowerCase(locale) });
}

/**
 * What "Yanlışsa bildir" sends under a row a save refused for its content (README "Saving" →
 * Refusal reports): the row's plain text as it was refused (an edit clears the refusal, and the
 * link with it), or null. The service reads only plain texts, so a secret row is never refused.
 */
export function refusalReportText(
  issue: { problem: string } | undefined,
  words: { kind: string; text?: string },
): string | null {
  if (!issue || refusedCategory(issue.problem) === null) return null;
  return reportedWords(words);
}

/** A row's words as "Yanlışsa bildir" sends and compares them: the plain text, trimmed; else null. */
export function reportedWords(words: { kind: string; text?: string }): string | null {
  if (words.kind !== "plain") return null;
  const text = words.text?.trim() ?? "";
  return text ? text : null;
}

/**
 * The save's issues once "Yanlışsa bildir" answered that the row's text saves now (README "Saving"
 * → Refusal reports, `not_refused`): that row's content refusal goes, and with it the field's mark,
 * its question's "!" and the bar's "Önce işaretli alanları düzelt.", so Kaydet opens again. Every
 * other issue stays.
 */
export function withoutRefusal<
  T extends { place: string; index: number | null; field: string; problem: string },
>(issues: T[], at: { place: string; index: number; field: string }): T[] {
  return issues.filter(
    (issue) =>
      !(
        issue.place === at.place &&
        issue.index === at.index &&
        issue.field === at.field &&
        refusedCategory(issue.problem) !== null
      ),
  );
}

const NOTICE_STEPS = new Set([
  "warn",
  "restrict_rows",
  "pause_ai",
  "suspend",
  "appeal_received",
  "upheld",
  "overturned",
  // Staff lifted a decision no appeal asked about ("Haksız bul" in Studio).
  "lifted",
]);

/** The `moderation` notice's line (the account's name is its title); null for a step it does not know. */
export function moderationNoticeText(
  data: Record<string, string | undefined>,
  t: TFunction,
): string | null {
  const step = data.step ?? "";
  if (!NOTICE_STEPS.has(step)) return null;
  return t(`afkMod.notice.${step}`, { kind: categoryText(data.category, t) });
}
