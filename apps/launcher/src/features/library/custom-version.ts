// CUSTOM-VER: an instance made from another installer's version (OptiFine's, Forge's, a
// modpack's own) is named by that version wherever the game and loader are shown.
import type { CustomVersion, InstanceSummary, LoaderKind } from "@/lib/api";

/** What a custom version needs to be named: the list's entry or an instance's fields. */
export interface CustomVersionLike {
  id: string;
  minecraft: string;
  loader: LoaderKind;
  loaderVersion?: string;
}

/**
 * How players know another installer's version: OptiFine's as "OptiFine 1.21.11" (edition "HD U
 * J9"), one named exactly as the launcher names its own loader profiles as "Forge 1.21.11" (the
 * loader's version); anything else by its own name (`friendly` false).
 */
export function customName(
  version: CustomVersionLike,
  loaderName: (loader: LoaderKind) => string,
): { label: string; detail?: string; friendly: boolean } {
  const { id, minecraft, loader, loaderVersion } = version;
  const optifine = id.startsWith(`${minecraft}-OptiFine_`)
    ? id.slice(minecraft.length + "-OptiFine_".length)
    : "";
  if (optifine) {
    return {
      label: `OptiFine ${minecraft}`,
      detail: optifine.replaceAll("_", " "),
      friendly: true,
    };
  }
  if (loader !== "vanilla" && loaderVersion) {
    const own: Record<Exclude<LoaderKind, "vanilla">, string[]> = {
      fabric: [`fabric-loader-${loaderVersion}-${minecraft}`],
      quilt: [`quilt-loader-${loaderVersion}-${minecraft}`],
      forge: [`${minecraft}-forge-${loaderVersion}`],
      neoForge: [`neoforge-${loaderVersion}`, `${minecraft}-neoforge-${loaderVersion}`],
    };
    if (own[loader].includes(id)) {
      return { label: `${loaderName(loader)} ${minecraft}`, detail: loaderVersion, friendly: true };
    }
  }
  return { label: id, friendly: false };
}

/**
 * The line naming what an instance runs: "Fabric 1.21.4", "Minecraft 1.21.4", or another
 * installer's version ("OptiFine 1.21.11"; one of its own by its name).
 */
export function gameLine(
  instance: Pick<InstanceSummary, "loader" | "mcVersion" | "customVersion" | "loaderVersion">,
  loaderName: (loader: LoaderKind) => string,
): string {
  if (instance.customVersion) {
    return customName(
      {
        id: instance.customVersion,
        minecraft: instance.mcVersion,
        loader: instance.loader,
        loaderVersion: instance.loaderVersion,
      },
      loaderName,
    ).label;
  }
  return instance.loader === "vanilla"
    ? `Minecraft ${instance.mcVersion}`
    : `${loaderName(instance.loader)} ${instance.mcVersion}`;
}

/** What a custom version runs on beside Minecraft: its mod loader, OptiFine, or nothing. */
export function customBase(version: CustomVersion): LoaderKind | "optiFine" | null {
  if (version.loader !== "vanilla") return version.loader;
  return version.kind === "optiFine" ? "optiFine" : null;
}

/** The version a "Özel" pick stands for: the player's pick while it is listed, else the newest. */
export function pickedCustom(versions: CustomVersion[], picked: string): CustomVersion | undefined {
  return versions.find((v) => v.id === picked) ?? versions[0];
}
