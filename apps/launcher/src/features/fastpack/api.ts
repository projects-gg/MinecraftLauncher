import { queryOptions } from "@tanstack/react-query";
import { commands, type FastPacksView } from "@/bindings";
import { isTauri } from "@/lib/platform";
import { mockFastPack } from "./mock";

/** "Hızlı kaynak paketi yükleme" in the launcher (`src-tauri/src/fastpack.rs`, `crates/pl-content`). */
export interface FastPackApi {
  /** The page's choice: called at its start and whenever it changes. */
  configure(enabled: boolean): Promise<void>;
  /** Whether the switch shows, and what it does for the install Play opens (`null`: none). */
  state(instanceId: string | null): Promise<FastPacksView>;
}

const tauriFastPack: FastPackApi = {
  configure: (enabled) => commands.fastPacksConfigure(enabled),
  state: async (instanceId) => {
    const result = await commands.fastPacksState(instanceId);
    if (result.status === "ok") return result.data;
    // The install could not be read: the switch still shows, without a line.
    const flagOnly = await commands.fastPacksState(null);
    return flagOnly.status === "ok"
      ? flagOnly.data
      : { shown: false, official: true, available: true };
  },
};

export const fastPackApi: FastPackApi = isTauri() ? tauriFastPack : mockFastPack;

/**
 * The row's view for the install Play opens. The command bar loads it ahead and the row reads the
 * same cache entry, so Genel opens with the row already in place (TASTE-SWEEP FASTPACK r1).
 */
export function fastPackStateQuery(instanceId: string | null) {
  return queryOptions({
    queryKey: ["fast-packs", instanceId],
    queryFn: () => fastPackApi.state(instanceId),
    staleTime: 60_000,
  });
}
