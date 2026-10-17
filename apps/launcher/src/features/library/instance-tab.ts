export type InstanceTab =
  | "overview"
  | "modcenter"
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
  "modcenter",
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
 * A vanilla instance has no mod center.
 */
export function instanceTab(asked: unknown, vanilla: boolean): InstanceTab {
  const tab = tabs.find((t) => t === asked);
  if (!tab || (tab === "modcenter" && vanilla)) return "overview";
  return tab;
}
