// Ready-made sets (MODSET1): what a card shows and how a set fits an instance. React-free; the
// words come back as locale keys (`modSets.*`) with their values, so tests can check both.
import type {
  InstanceIcon,
  LoaderKind,
  SetCard,
  SetItem,
  SetItemKind,
  SetPurpose,
  SetVariant,
  SyncSummary,
} from "@/lib/api";

/** A locale key with its interpolation values. */
export interface Words {
  key: string;
  values?: Record<string, string | number>;
}

/** The set's name in the interface language (English when the team wrote one). */
export function setName(set: { name: string; nameEn?: string }, english: boolean): string {
  return (english && set.nameEn?.trim()) || set.name;
}

/** The set's summary in the interface language (falls back to the Turkish one). */
export function setSummary(
  set: { summary?: string; summaryEn?: string },
  english: boolean,
): string | undefined {
  return (english && set.summaryEn?.trim()) || set.summary || undefined;
}

/** How a set fits an instance on `mcVersion` with `loader`. */
export type SetFit =
  | { kind: "fits"; variant: SetVariant }
  /** No variant for the instance's Minecraft version; the versions it has, newest first. */
  | { kind: "otherVersion"; versions: string[] }
  /** A variant for the version needs another loader. */
  | { kind: "otherLoader"; loader: LoaderKind };

/**
 * The variant an instance uses: the one for its loader, else the loader-less one (packs only,
 * any loader). Mods never cross loaders. Mirrors `pl_content::ModSet::variant_for`.
 */
export function fitFor(set: SetCard, mcVersion: string, loader: LoaderKind): SetFit {
  const onVersion = set.variants.filter((v) => v.minecraft === mcVersion.trim());
  const exact = loader !== "vanilla" ? onVersion.find((v) => v.loader === loader) : undefined;
  const packsOnly = onVersion.find((v) => !v.loader);
  const variant = exact ?? packsOnly;
  if (variant) return { kind: "fits", variant };
  const other = onVersion.find((v) => v.loader);
  if (other?.loader) return { kind: "otherLoader", loader: other.loader };
  return { kind: "otherVersion", versions: set.variants.map((v) => v.minecraft) };
}

/**
 * The variant a new instance from the set starts with: the recommended Minecraft version's when
 * the set has one (the version "Sıfırdan" and the official pack lead with, whose game files the
 * player most likely has already), else the newest.
 */
export function defaultVariant(set: SetCard, recommended?: string): SetVariant | undefined {
  return set.variants.find((v) => recommended && v.minecraft === recommended) ?? set.variants[0];
}

/** Item icons a card shows: the first `max` choices (not libraries) with an icon, and how many
 * other choices there are. */
export function iconStrip(variant: SetVariant, max = 6): { shown: SetItem[]; more: number } {
  const choices = variant.items.filter((item) => !item.library);
  const withIcon = choices.filter((item) => item.iconUrl);
  const shown = withIcon.slice(0, max);
  return { shown, more: choices.length - shown.length };
}

/** "12 mod, 3 kaynak paketi": what adding the set installs, by kind (zeros left out). */
export function countWords(counts: {
  mods: number;
  resourcePacks: number;
  shaderPacks: number;
}): Words[] {
  const words: Words[] = [];
  if (counts.mods > 0) words.push({ key: "modSets.card.mods", values: { count: counts.mods } });
  if (counts.resourcePacks > 0) {
    words.push({ key: "modSets.card.resourcePacks", values: { count: counts.resourcePacks } });
  }
  if (counts.shaderPacks > 0) {
    words.push({ key: "modSets.card.shaderPacks", values: { count: counts.shaderPacks } });
  }
  return words;
}

/** Why a set does not fit an instance, for its card. */
export function fitWords(fit: SetFit, loaderName: (loader: LoaderKind) => string): Words | null {
  switch (fit.kind) {
    case "fits":
      return null;
    case "otherLoader":
      return { key: "modSets.card.forLoader", values: { loader: loaderName(fit.loader) } };
    case "otherVersion":
      return { key: "modSets.card.forVersions", values: { versions: fit.versions.join(", ") } };
  }
}

/** The items a player chooses and the ones that only come along (libraries). */
export function splitItems(items: SetItem[]): { choices: SetItem[]; libraries: SetItem[] } {
  return {
    choices: items.filter((item) => !item.library),
    libraries: items.filter((item) => item.library),
  };
}

/** Items installed under `choices` (missing: the default): the chosen ones and, transitively,
 * what they require. Mirrors the mod center's rule. */
export function enabledItems(items: SetItem[], choices: Record<string, boolean>): Set<string> {
  const byId = new Map(items.map((item) => [item.id, item]));
  const on = new Set<string>();
  const visit = (id: string) => {
    const item = byId.get(id);
    if (!item || on.has(id)) return;
    on.add(id);
    for (const required of item.requires) visit(required);
  };
  for (const item of items) {
    if (!item.library && (choices[item.id] ?? item.defaultEnabled)) visit(item.id);
  }
  return on;
}

/** The choices that need it (on, by title), for an item that is on although not chosen. */
export function neededBy(items: SetItem[], enabled: Set<string>, id: string): string[] {
  return items
    .filter((item) => enabled.has(item.id) && item.requires.includes(id))
    .map((item) => item.title);
}

/** Bytes of the items that are on. */
export function bytesOf(items: SetItem[], enabled: Set<string>): number {
  return items
    .filter((item) => enabled.has(item.id))
    .reduce((sum, item) => sum + item.sizeBytes, 0);
}

/** Choices that differ from the defaults: what a new instance from the set is created with. */
export function changedChoices(
  items: SetItem[],
  choices: Record<string, boolean>,
): Record<string, boolean> {
  const changed: Record<string, boolean> = {};
  for (const item of items) {
    const choice = choices[item.id];
    if (!item.library && choice !== undefined && choice !== item.defaultEnabled) {
      changed[item.id] = choice;
    }
  }
  return changed;
}

const kindKeys: Record<SetItemKind, "mods" | "resourcePacks" | "shaderPacks"> = {
  mod: "mods",
  resourcePack: "resourcePacks",
  shaderPack: "shaderPacks",
};

/** The locale key of an item kind's chip. */
export function kindKey(kind: SetItemKind): string {
  return `modSets.kind.${kind}`;
}

/** "3 mod ve 1 kaynak paketi eklenecek": a dry run's summary, by action and kind. */
export function summaryWords(summary: SyncSummary): Array<{ key: string; items: Words[] }> {
  const actions = [
    ["added", summary.added],
    ["updated", summary.updated],
    ["removed", summary.removed],
  ] as const;
  return actions
    .map(([action, counts]) => ({
      key: `modSets.summary.${action}`,
      items: (["mods", "resourcePacks", "shaderPacks"] as const)
        .filter((kind) => counts[kind] > 0)
        .map((kind) => ({ key: `modSets.summary.${kind}`, values: { count: counts[kind] } })),
    }))
    .filter((part) => part.items.length > 0);
}

/**
 * A dry run in one sentence ("2 mod ve 1 kaynak paketi eklenecek. 3 MB indirilecek."); `t` is
 * the translator, `list` joins words the language's way, `size` formats bytes.
 */
export function describeSummary(
  summary: SyncSummary,
  t: (key: string, values?: Record<string, unknown>) => string,
  list: (parts: string[]) => string,
  size: (bytes: number) => string,
): string {
  const parts = summaryWords(summary).map((part) =>
    t(part.key, { items: list(part.items.map((item) => t(item.key, item.values))) }),
  );
  if (parts.length === 0) return t("modSets.summary.noFiles");
  const download =
    summary.downloadBytes > 0
      ? ` ${t("modSets.summary.download", { size: size(summary.downloadBytes) })}`
      : "";
  return `${parts.join(", ")}.${download}`;
}

/** The kind's summary key, for counts built in the UI. */
export function summaryKindKey(kind: SetItemKind): string {
  return `modSets.summary.${kindKeys[kind]}`;
}

/**
 * The set the crash dialog suggests updating (FIX2): installed, changed by the team since, and
 * one of its mods is what the crash strongly points at (by name, or the file "Bu modu kapat"
 * would turn off). A new PvP instance crashed on TipTapShow until the set's update brought the
 * library it needed.
 */
export function setToUpdate<
  S extends {
    installed: boolean;
    available: boolean;
    updateAvailable: boolean;
    items: ReadonlyArray<{ title: string; fileName: string }>;
  },
>(
  crash: {
    suspects: ReadonlyArray<{ name: string; strong: boolean }>;
    turnOff: { file: string } | null;
  },
  sets: readonly S[],
): S | null {
  const norm = (text: string) => text.trim().toLowerCase();
  const names = new Set(crash.suspects.filter((s) => s.strong).map((s) => norm(s.name)));
  const file = crash.turnOff ? norm(crash.turnOff.file) : null;
  return (
    sets.find(
      (set) =>
        set.installed &&
        set.available &&
        set.updateAvailable &&
        set.items.some((item) => names.has(norm(item.title)) || norm(item.fileName) === file),
    ) ?? null
  );
}

/** The instance icon a new instance from a set gets. */
export function iconFor(purpose: SetPurpose): InstanceIcon {
  switch (purpose) {
    case "pvp":
      return "swords";
    case "performance":
      return "pickaxe";
    case "visual":
      return "potion";
    case "other":
      return "compass";
  }
}
