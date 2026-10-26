// Bulk management (CAFK-FEAT, the contract's "Bulk management"): several cloud accounts at once
// with the routes that exist, one account after another in the list's order, and one sentence for
// the result ("3 hesap başlatıldı, 1 hesap başlatılamadı: süresi dolmuş."). No bulk delete.
export type BulkAction = "start" | "stop" | "restart" | "copy";

/** The reason words of the contract's table, by the launcher's error code (`BULUT-…`). */
const REASONS: Record<string, string> = {
  "BULUT-SURE": "expired",
  "BULUT-ASKIDA": "staff",
  "BULUT-UZAK": "staff",
  "BULUT-ENGEL": "blocked",
  "BULUT-DOLU": "full",
  "BULUT-SUNUCU-DEGISTI": "serverChanges",
  "BULUT-OYUNDA-DEGIL": "notRunning",
  "BULUT-SINIR": "rateLimited",
  "BULUT-DEGISTI": "changed",
  "BULUT-SUNUCU": "unreachable",
  "BULUT-AG": "unreachable",
  "BULUT-YOK": "unreachable",
  /** Ours: the same account runs on this computer (the two would keep kicking each other). */
  here: "here",
};

/** `cloudAfkFeat.bulk.reasons.<key>`; anything else is "bir sorun çıktı". */
export function reasonKey(code: string | null | undefined): string {
  return (code && REASONS[code]) || "other";
}

/** A command error's code (the backend's `CommandError`, or a preview's look-alike). */
export function errorCode(error: unknown): string | null {
  if (error instanceof Error && "code" in error && typeof error.code === "string") {
    return error.code;
  }
  return null;
}

export interface BulkOutcome {
  done: number;
  failed: number;
  /** The first failure's reason key. */
  reason: string | null;
}

/** Tallies results in the list's order: how many went through, how many not, and the first reason. */
export function tally(results: Array<{ ok: boolean; code?: string | null }>): BulkOutcome {
  let done = 0;
  let failed = 0;
  let reason: string | null = null;
  for (const result of results) {
    if (result.ok) done++;
    else {
      failed++;
      reason ??= reasonKey(result.code);
    }
  }
  return { done, failed, reason };
}

/** `/login ******` → `/login` (a secret row's preview without its hidden words). */
export function rowName(preview: string): string {
  return preview.replace(/\s*\*+$/, "") || preview;
}

/** The command a secret row keeps in view (`/login` of `/login ******`), else "". */
export function commandOf(preview: string): string {
  const name = rowName(preview);
  return name !== preview && name.startsWith("/") ? name : "";
}

/**
 * What a copied secret row becomes for a target: its command and the typed words (the field asks
 * for the password only), or a whole command when one was typed.
 */
export function lineOf(preview: string, text: string): string {
  const words = text.trim();
  const command = commandOf(preview);
  return !command || words.startsWith("/") ? words : `${command} ${words}`;
}

/** What an action needs to know of an account. */
interface BulkTarget {
  desired: string;
  hold: unknown;
  expired: boolean;
}

/** Which of the chosen accounts an action applies to (the rest are skipped, not counted). */
export function appliesTo(action: Exclude<BulkAction, "copy">, account: BulkTarget): boolean {
  switch (action) {
    case "start":
      return account.desired !== "run" || Boolean(account.hold) || account.expired;
    case "stop":
      return account.desired === "run";
    case "restart":
      return account.desired === "run" && !account.hold && !account.expired;
  }
}
