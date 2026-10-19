// The official pack's mods that an instance installs when its game starts: before the first
// download (or the first start of another Minecraft version's set) the mods folder holds none of
// them while the mod center already shows them chosen. The overview and the mods tab list them
// as on their way instead of saying there are no mods (POLISH4).
import { useQuery } from "@tanstack/react-query";
import { api, type InstanceDetail, type ModCenterItem, type ModCenterView } from "@/lib/api";

/**
 * Pack items the next start puts into `mods/`: on (chosen, or needed by a chosen one) and never
 * installed. In the pack's order, the chosen mods before the libraries they need.
 */
export function incomingMods(items: readonly ModCenterItem[]): ModCenterItem[] {
  const incoming = items.filter(
    (item) => item.enabled && !item.installed && item.folder === "mods",
  );
  return [...incoming.filter((item) => !item.library), ...incoming.filter((item) => item.library)];
}

/**
 * The instance follows the pack and its game is not in place for this version yet (never
 * downloaded, or another Minecraft version's first start): only then is the mod center asked.
 * An installed instance has nothing on the way, or the mod center says why (a wiped folder).
 */
export function awaitsPackMods(
  instance: Pick<InstanceDetail, "managed" | "loader" | "state">,
): boolean {
  return instance.managed && instance.loader !== "vanilla" && instance.state !== "ready";
}

/**
 * The side panel's line: both names when there are two at most, else the first one and how
 * many more ("Sodium ve 2 mod daha"), so it fits the narrow panel.
 */
export function incomingNames(items: readonly ModCenterItem[]): { names: string[]; more: number } {
  const shown = items.length <= 2 ? items : items.slice(0, 1);
  return { names: shown.map((item) => item.title), more: items.length - shown.length };
}

const none: ModCenterItem[] = [];
const select = (view: ModCenterView) => incomingMods(view.items);

/**
 * The pack's mods the instance's next start installs: `undefined` while the mod center is
 * asked, empty when none are on the way. The mod center's own query (one answer for the
 * overview, the mods tab and the mod center). Mods the player added before the first start do
 * not hide them.
 */
export function useIncomingMods(
  instance: InstanceDetail,
  enabled = true,
): ModCenterItem[] | undefined {
  const asks = enabled && awaitsPackMods(instance);
  const view = useQuery({
    queryKey: ["mod-center", instance.id],
    queryFn: () => api.modCenter.view(instance.id),
    staleTime: 15_000,
    retry: false,
    enabled: asks,
    select,
  });
  if (!asks) return none;
  if (view.isPending) return undefined;
  return view.data ?? none;
}
