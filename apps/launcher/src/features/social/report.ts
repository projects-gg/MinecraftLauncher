// "Rapor et" (MOD1), pure rules (report.test.ts): which reasons fit which target (the service
// refuses others), the note's length as the service counts it, and who may be blocked with it.
import type { ReportReason, ReportTarget } from "@/lib/api/social-types";

export type ReportKind = ReportTarget["kind"];

/** The reasons each target takes, in the dialog's order. */
export const REPORT_REASONS: Record<ReportKind, readonly ReportReason[]> = {
  player: ["name", "profile", "harassment", "spam", "scam", "other"],
  crew: ["name", "harassment", "spam", "scam", "other"],
  message: ["harassment", "inappropriate", "spam", "scam", "other"],
};

/** A plan's maker (FIX2) also takes "Uygunsuz not" (the plan's note), a reaction's giver (CHAT2)
 * "Uygunsuz tepki" (its emoji). */
const PLAN_REASONS: readonly ReportReason[] = [
  "name",
  "profile",
  "harassment",
  "inappropriate",
  "spam",
  "scam",
  "other",
];

/** SOC-NOTE: a friend whose status note shows also takes "Uygunsuz durum notu", beside the
 * other "Uygunsuz …" reasons. */
const NOTE_REASONS: readonly ReportReason[] = [
  "name",
  "profile",
  "inappropriate",
  "harassment",
  "spam",
  "scam",
  "other",
];

/** The reasons a target takes, in the dialog's order. BAN-TYPES: a player reported from a voice
 * room (`voice`) is offered "Sesli sohbet ihlali" first; nothing else is. */
export function reasonsFor(target: ReportTarget): readonly ReportReason[] {
  if (target.kind !== "player") return REPORT_REASONS[target.kind];
  const reasons =
    target.plan || target.reaction
      ? PLAN_REASONS
      : target.note
        ? NOTE_REASONS
        : REPORT_REASONS.player;
  return target.voice ? ["voice", ...reasons] : reasons;
}

/** The words of a reason for a target ("Uygunsuz ad" for a player, "… ya da slogan" for a crew,
 * "Uygunsuz not" from a plan, "Uygunsuz durum notu" for a status note). */
export function reasonKey(
  kind: ReportKind,
  reason: ReportReason,
  plan = false,
  reaction = false,
  statusNote = false,
): string {
  if (reason === "name")
    return kind === "crew" ? "reports.reasons.crewName" : "reports.reasons.playerName";
  if (plan && reason === "inappropriate") return "fix2.report.noteReason";
  if (statusNote && reason === "inappropriate") return "socNote.report.reason";
  if (reaction && reason === "inappropriate") return "chat2.report.reactionReason";
  return `reports.reasons.${reason}`;
}

/** A note's characters (the service collapses spaces and counts code points). */
export const NOTE_MAX = 200;

export function noteLength(note: string): number {
  return [...note.replace(/\s+/gu, " ").trim()].length;
}

/** The note as it goes: undefined when empty. */
export function noteOf(note: string): string | undefined {
  const clean = note.replace(/\s+/gu, " ").trim();
  return clean ? clean : undefined;
}

/** "Ayrıca engelle" is offered for players and a message's sender, never for a crew. */
export function canBlock(kind: ReportKind): boolean {
  return kind !== "crew";
}
