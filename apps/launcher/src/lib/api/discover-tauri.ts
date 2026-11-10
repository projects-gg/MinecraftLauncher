// The app's side of `discoverApi` (CREW-DISCOVER): commands/social_discover.rs. pl-social keeps
// the service's wire as it is; here tags this launcher does not know drop out. A failed command
// rejects with a `SocialCommandError` (`message` in the player's language).
import {
  type CmdError,
  commands,
  type DiscoverCrew as WireCrew,
  type DiscoverStatus as WireStatus,
  type DiscoverText as WireText,
} from "@/bindings";
import type { DiscoverApi, DiscoverCrew, DiscoverStatus, DiscoverText } from "./discover-types";
import { knownTags } from "./discover-types";
import { SocialCommandError } from "./social-tauri";

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new SocialCommandError(result.error);
  return result.data;
}

// Fields the backend may leave out (its defaults when reading) read as those defaults here.
function crewOf(wire: WireCrew): DiscoverCrew {
  return {
    ...wire,
    motto: wire.motto ?? "",
    tags: knownTags(wire.tags ?? []),
    join: wire.join ?? "open",
    maxMembers: wire.maxMembers ?? undefined,
    active: wire.active ?? false,
  };
}

function textOf(wire: WireText | null | undefined): DiscoverText | null {
  return wire ? { text: wire.text, tags: knownTags(wire.tags ?? []) } : null;
}

function statusOf(wire: WireStatus): DiscoverStatus {
  return {
    state: wire.state,
    listed: textOf(wire.listed),
    draft: textOf(wire.draft),
    changeWaits: wire.changeWaits ?? false,
    waveAt: wire.waveAt,
    retryAt: wire.retryAt ?? null,
    checks: wire.checks,
    editsLeft: wire.editsLeft,
    editsReset: wire.editsReset,
  };
}

export const tauriDiscover: DiscoverApi = {
  async list(etag) {
    const page = await ok(commands.socialDiscoverList(etag));
    if (!page) return null;
    return {
      listing: { ...page.listing, at: page.listing.at ?? 0, crews: page.listing.crews.map(crewOf) },
      etag: page.etag ?? null,
    };
  },
  join: (teamId) => ok(commands.socialDiscoverJoin(teamId)),
  status: async (teamId) => statusOf(await ok(commands.socialDiscoverStatus(teamId))),
  set: async (teamId, text, tags) =>
    statusOf(await ok(commands.socialDiscoverSet(teamId, text, [...tags]))),
  async off(teamId) {
    const status = await ok(commands.socialDiscoverOff(teamId));
    return status ? statusOf(status) : null;
  },
};
