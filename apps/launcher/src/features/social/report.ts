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

/** The words of a reason for a target ("Uygunsuz ad" for a player, "… ya da slogan" for a crew). */
export function reasonKey(kind: ReportKind, reason: ReportReason): string {
  if (reason === "name")
    return kind === "crew" ? "reports.reasons.crewName" : "reports.reasons.playerName";
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
