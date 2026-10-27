// One card per game account (AFK-CLEAR): a desktop AFK account with the same name on the same
// server as one of the player's 7/24 Bulut accounts (its twin, `CloudPageData.twins`) shows on that
// cloud card instead of a card of its own. Pure, so the page, the cards and the tests read the same.
import { isAfkIdle } from "@/features/accounts/afk-match";
import type { AfkAccount } from "@/lib/api";
import type { CloudAccount, CloudPageData } from "./api";
import { isCloudActive } from "./status";

/** The ids of the cloud accounts the page lists (the service answers, the player is signed in). */
function listedIds(data: CloudPageData | undefined): Set<string> {
  if (!data?.overview.available || data.overview.session !== "signedIn") return new Set();
  return new Set(data.overview.accounts.map((account) => account.id));
}

/** Desktop accounts whose card is their cloud twin's: the desktop list leaves them out. */
export function foldedIds(data: CloudPageData | undefined): Set<string> {
  const listed = listedIds(data);
  if (!listed.size || !data) return new Set();
  return new Set(
    data.twins.filter((twin) => listed.has(twin.cloudId)).map((twin) => twin.desktopId),
  );
}

/** The copy on this computer of a listed cloud account, if there is one. */
export function hereCopy(
  data: CloudPageData | undefined,
  cloudId: string,
  desktop: readonly AfkAccount[],
): AfkAccount | null {
  if (!listedIds(data).has(cloudId) || !data) return null;
  const twin = data.twins.find((t) => t.cloudId === cloudId);
  return twin ? (desktop.find((d) => d.id === twin.desktopId) ?? null) : null;
}

/** Where an account in both places runs, as its card tells it. */
export type TwinPlace = "here" | "cloud";

/**
 * This computer while its copy here is busy (connecting, in the game, reconnecting, fetching its
 * client), or stopped with a failure while the cloud one is not asked to run and holds nothing of
 * its own (it was tried here last: the card says why it stopped); else the cloud, which also
 * speaks while the account runs nowhere (its subscription is the card's).
 */
export function twinPlace(cloud: CloudAccount, here: AfkAccount): TwinPlace {
  if (!isAfkIdle(here.state)) return "here";
  if (isCloudActive(cloud)) return "cloud";
  const failedHere = here.state.kind === "error" || here.state.kind === "clientFailed";
  return failedHere && !cloud.hold && !cloud.expired ? "here" : "cloud";
}
