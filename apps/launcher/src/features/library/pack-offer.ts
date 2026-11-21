// PACK-OFFER: the official pack where the player picks a Minecraft version by hand. Everything comes
// from the signed content (the family's members, the matching member's loader); no version is
// named here. Kept free of UI imports so it is tested on its own.
import {
  api,
  type FamilyMember,
  type LoaderKind,
  type OfficialFamily,
  type OfficialPack,
} from "@/lib/api";

/** What the official pack offers for a version and loader picked by hand. */
export type PackOffer =
  /** No official instance: install one that follows the member for this version. */
  | { kind: "install"; pack: OfficialPack }
  /** The official instance is on another version: its version switch, aimed at this one. */
  | { kind: "switch"; pack: OfficialPack; instanceId: string; from: string }
  /** The official instance is already on this version. */
  | { kind: "open"; pack: OfficialPack; instanceId: string };

/** The family as the page asks for it: the member for `minecraft` comes with its loader. */
export function officialFamilyQuery(minecraft: string) {
  return {
    queryKey: ["official-family", minecraft],
    queryFn: () => api.officialFamily(minecraft),
    staleTime: 5 * 60_000,
    retry: false,
  } as const;
}

/**
 * The offer for `selection`: only when the family publishes a member for exactly that Minecraft
 * version and loader. `self` is the instance the offer would show on (an empty custom one): the
 * official instance never offers itself.
 */
export function packOffer(
  family: OfficialFamily | undefined,
  selection: { minecraft: string; loader: LoaderKind },
  self?: string,
): PackOffer | null {
  const pack = family?.matched;
  if (!pack || !selection.minecraft) return null;
  if (pack.mcVersion !== selection.minecraft || pack.loader !== selection.loader) return null;
  const instanceId = family.instanceId;
  if (!instanceId) return { kind: "install", pack };
  if (instanceId === self) return null;
  if (family.instanceMinecraft === selection.minecraft) return { kind: "open", pack, instanceId };
  return { kind: "switch", pack, instanceId, from: family.instanceMinecraft ?? "" };
}

/**
 * The family's members newer than `minecraft` (the version an instance of the family is on) that
 * the player was not told about yet, newest first. None when `minecraft` is not a member (the
 * family stopped offering it: the mod center says so on its own) or is the newest.
 */
export function newerMembers(
  family: OfficialFamily | undefined,
  minecraft: string,
  seen: readonly string[],
): FamilyMember[] {
  const members = family?.members ?? [];
  const at = members.findIndex((m) => m.minecraft === minecraft);
  if (at <= 0) return [];
  return members.slice(0, at).filter((m) => !seen.includes(m.packId));
}

/** The key that remembers a hidden offer on an instance (`packOffersSeen`). */
export function offerSeenKey(instanceId: string, packId: string): string {
  return `${instanceId}:${packId}`;
}
