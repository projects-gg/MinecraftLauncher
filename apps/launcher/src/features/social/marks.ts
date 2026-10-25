// Profile marks without React (SOC-VERIFY): the mark's name, how a row names someone with one, and
// a sentence split around its name's place (person-mark.tsx draws them; marks.test.ts tests them).
import type { ProfileMark } from "@/lib/api/social-types";

export type Translate = (key: string, options?: Record<string, unknown>) => string;

/** The mark's name: "Doğrulanmış hesap" / "Projects yöneticisi". */
export function markLabel(mark: ProfileMark, t: Translate): string {
  return t(mark === "admin" ? "verify.admin" : "verify.verified");
}

/** How a row names someone for screen readers: "Ayşe, Doğrulanmış hesap" (the name alone without). */
export function markedName(name: string, mark: ProfileMark | undefined, t: Translate): string {
  return mark ? t("verify.named", { name, mark: markLabel(mark, t) }) : name;
}

/** A private-use character that holds the name's place while a sentence is translated. */
const NAME_SLOT = String.fromCharCode(0xe000);

/** A sentence split around its name's one place, or null when the name is not in it once. */
export function sentenceParts(text: (name: string) => string): [string, string] | null {
  const parts = text(NAME_SLOT).split(NAME_SLOT);
  return parts.length === 2 ? [parts[0] ?? "", parts[1] ?? ""] : null;
}
