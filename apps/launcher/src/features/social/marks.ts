// Profile badges without React (BADGES-UI; SOC-VERIFY's marks grown into a catalogue): which of
// someone's badges this launcher draws and how a row or a sentence names them
// (person-mark.tsx draws them; marks.test.ts tests them). Display rules (docs/tasks/badges.md,
// "BADGES-UI için"): beside a name only the first badge (the catalogue's order is importance), its
// tooltip and a screen reader name them all; the whole list sits under the name on a card.
import { type MarkItem, markLookOf } from "@pl/ui";
import type { MarkDef } from "@/lib/api/social-types";

export type Translate = (key: string, options?: Record<string, unknown>) => string;

/**
 * The service's catalogue until Studio's owner first saves one, as the launcher's Rust side carries
 * it too (`pl_social::MarkCatalog::builtin`): the badges that show, in order. A snapshot from before
 * badges has none; the first one always brings the catalogue.
 */
export const BUILTIN_MARK_DEFS: readonly MarkDef[] = [
  mark("admin", "Projects yöneticisi", "Projects admin", "red", "check"),
  mark("moderator", "Projects moderatörü", "Projects moderator", "purple", "check"),
  mark("staff", "Projects personeli", "Projects staff", "green", "check"),
  mark("verified", "Doğrulanmış hesap", "Verified account", "blue", "check"),
  mark("premium", "Premium Club üyesi", "Premium Club member", "orange", "star"),
];

function mark(key: string, tr: string, en: string, color: string, glyph: string): MarkDef {
  return { key, name: { tr, en }, color, glyph, shape: "seal" };
}

/**
 * The badges `keys` name that the catalogue knows and this version draws, in the keys' order (the
 * catalogue's: the most important first), named in the interface's language. A key a newer
 * catalogue made, or a look this version cannot draw, is left out.
 */
export function markItems(
  keys: readonly string[] | undefined,
  defs: readonly MarkDef[],
  english: boolean,
): MarkItem[] {
  if (!keys || keys.length === 0) return [];
  const items: MarkItem[] = [];
  for (const key of keys) {
    if (items.some((item) => item.key === key)) continue;
    const def = defs.find((one) => one.key === key);
    const look = def ? markLookOf(def) : null;
    if (!def || !look) continue;
    items.push({ key, label: english ? def.name.en : def.name.tr, ...look });
  }
  return items;
}

/** How a row names someone for screen readers: "Ayşe, Projects yöneticisi, Premium Club üyesi"
 * (the name alone without badges). */
export function markedName(name: string, marks: readonly MarkItem[], t: Translate): string {
  return marks.length > 0
    ? t("verify.named", { name, mark: marks.map((item) => item.label).join(", ") })
    : name;
}

/** "Rozetlerim": one of the player's badges and whether others see it. */
export interface HeldMark {
  item: MarkItem;
  shown: boolean;
}

/**
 * The badges the player holds, shown and kept off, in the catalogue's order (only those this
 * launcher can draw: the switch of one it cannot show would make no sense).
 */
export function heldMarks(
  shown: readonly string[] | undefined,
  hidden: readonly string[] | undefined,
  defs: readonly MarkDef[],
  english: boolean,
): HeldMark[] {
  const on = new Set(shown ?? []);
  const off = new Set((hidden ?? []).filter((key) => !on.has(key)));
  const order = defs.map((def) => def.key).filter((key) => on.has(key) || off.has(key));
  return markItems(order, defs, english).map((item) => ({ item, shown: on.has(item.key) }));
}

/** The keys the player keeps off after switching `key` on or off, in the catalogue's order. */
export function hiddenAfter(
  held: readonly HeldMark[],
  key: string,
  show: boolean,
): string[] {
  return held
    .filter((one) => (one.item.key === key ? !show : !one.shown))
    .map((one) => one.item.key);
}

/** A private-use character that holds the name's place while a sentence is translated. */
const NAME_SLOT = String.fromCharCode(0xe000);

/** A sentence split around its name's one place, or null when the name is not in it once. */
export function sentenceParts(text: (name: string) => string): [string, string] | null {
  const parts = text(NAME_SLOT).split(NAME_SLOT);
  return parts.length === 2 ? [parts[0] ?? "", parts[1] ?? ""] : null;
}
