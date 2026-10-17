// Offline player names: Minecraft's rules, checked as the player types, with a fix-it
// suggestion for the usual slips (Turkish letters, spaces).

/** Minecraft's rule for player names, enforced for offline accounts too. */
export const OFFLINE_NAME = /^[A-Za-z0-9_]{3,16}$/;
export const NAME_MIN = 3;
export const NAME_MAX = 16;

export type NameProblem =
  | "empty"
  | "tooShort"
  | "tooLong"
  | "turkish"
  | "spaces"
  | "characters"
  | "taken";

export interface NamedAccount {
  id: string;
  kind: "microsoft" | "offline";
  username: string;
}

export interface NameCheck {
  /** What would be saved: the input without surrounding spaces. */
  name: string;
  problem: NameProblem | null;
  /** A valid, free name close to the input (Turkish letters spelled out, spaces as `_`). */
  suggestion: string | null;
  /** The offline account that already has this name (ignoring case). */
  takenBy: NamedAccount | null;
}

const TURKISH: Record<string, string> = {
  ç: "c",
  Ç: "C",
  ğ: "g",
  Ğ: "G",
  ı: "i",
  İ: "I",
  ö: "o",
  Ö: "O",
  ş: "s",
  Ş: "S",
  ü: "u",
  Ü: "U",
};
const TURKISH_LETTER = /[çÇğĞıİöÖşŞüÜ]/;
const SPACE = /\s/;
const NOT_ALLOWED = /[^A-Za-z0-9_]/;

/** The offline account (other than `selfId`) whose name matches, ignoring ASCII case. */
export function findTakenName(
  name: string,
  accounts: readonly NamedAccount[],
  selfId?: string,
): NamedAccount | null {
  const wanted = name.toLowerCase();
  return (
    accounts.find(
      (a) => a.kind === "offline" && a.id !== selfId && a.username.toLowerCase() === wanted,
    ) ?? null
  );
}

/** Turkish letters spelled out, spaces as `_`, anything else dropped; null when unusable. */
export function suggestName(raw: string): string | null {
  const spelled = [...raw.trim()].map((c) => TURKISH[c] ?? c).join("");
  const cleaned = spelled
    .replace(/\s+/g, "_")
    .replace(/[^A-Za-z0-9_]/g, "")
    .slice(0, NAME_MAX);
  return OFFLINE_NAME.test(cleaned) ? cleaned : null;
}

/**
 * Checks an offline player name. While the player is still typing (`final` false), a name that
 * is only too short or empty is not an error yet; on submit (`final` true) it is.
 */
export function checkOfflineName(
  raw: string,
  accounts: readonly NamedAccount[],
  options: { selfId?: string; final?: boolean } = {},
): NameCheck {
  const name = raw.trim();
  const result = (problem: NameProblem | null, suggestion: string | null = null): NameCheck => {
    const free = suggestion && !findTakenName(suggestion, accounts, options.selfId);
    return {
      name,
      problem,
      suggestion: free ? suggestion : null,
      takenBy: problem === "taken" ? findTakenName(name, accounts, options.selfId) : null,
    };
  };

  if (!name) return result(options.final ? "empty" : null);
  if (TURKISH_LETTER.test(name)) return result("turkish", suggestName(name));
  if (SPACE.test(name)) return result("spaces", suggestName(name));
  if (NOT_ALLOWED.test(name)) return result("characters", suggestName(name));
  if (name.length > NAME_MAX) return result("tooLong");
  if (name.length < NAME_MIN) return result(options.final ? "tooShort" : null);
  if (findTakenName(name, accounts, options.selfId)) return result("taken");
  return result(null);
}
