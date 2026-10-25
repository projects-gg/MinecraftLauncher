export type InstanceTab =
  | "overview"
  | "mods"
  | "resourcePacks"
  | "shaders"
  | "worlds"
  | "screenshots"
  | "servers"
  | "logs"
  | "settings";

const tabs: readonly InstanceTab[] = [
  "overview",
  "mods",
  "resourcePacks",
  "shaders",
  "worlds",
  "screenshots",
  "servers",
  "logs",
  "settings",
];

/**
 * The tab a link asks for (`?tab=mods`: projects:// links, the command palette, "Göster" after
 * a drop) when this instance has it, else the overview: a page never opens with no tab chosen.
 * The mod center became the "Modlar" tab's sections (MODHUB2): its old id `modcenter` and the
 * Modrinth browser for mods (`?browse=mod`, now its "Keşfet") open "Modlar".
 */
export function instanceTab(asked: unknown, browse?: unknown): InstanceTab {
  if (browse === "mod" || asked === "modcenter") return "mods";
  return tabs.find((t) => t === asked) ?? "overview";
}

/** The sections of the "Modlar" tab (the old mod center's, MODHUB2). */
export type ModsSection = "recommended" | "own" | "sets" | "discover";

const sections: readonly ModsSection[] = ["recommended", "own", "sets", "discover"];

/**
 * The section a link asks for: `focus` names one (the crash dialog's "Setlerde güncelle" the
 * sets, a dropped mod the player's own mods, the launch notices the recommended ones) and
 * `browse=mod` is "Keşfet". Otherwise none: the tab opens where the player left it.
 */
export function askedModsSection(focus: unknown, browse?: unknown): ModsSection | undefined {
  if (browse === "mod") return "discover";
  return sections.find((s) => s === focus);
}

/**
 * The sections of an instance's "Modlar" tab, in order. Every mod has one home: the official
 * pack's mods are its recommended ones (switched there), a set's files belong to its set, every
 * other file is the player's own; finding more comes last.
 */
export function modsSections(managed: boolean): ModsSection[] {
  return managed ? ["recommended", "own", "sets", "discover"] : ["own", "sets", "discover"];
}

/** The first of `wanted` the instance's tab has (an asked, then a remembered one), else its first. */
export function pickModsSection(
  managed: boolean,
  ...wanted: Array<ModsSection | undefined>
): ModsSection {
  const own = modsSections(managed);
  return (
    wanted.find((section): section is ModsSection => !!section && own.includes(section)) ??
    (own[0] as ModsSection)
  );
}
