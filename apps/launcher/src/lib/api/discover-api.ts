// `discoverApi` (CREW-DISCOVER, "Ekip bul"): the app's commands, or the browser preview's mock. Kept
// apart from `api` (index.ts) so the feature's seam stays in its own files, as voice chat's does.
import { isTauri } from "../platform";
import { mockDiscover } from "./discover-mock";
import { tauriDiscover } from "./discover-tauri";
import type { DiscoverApi } from "./discover-types";

export const discoverApi: DiscoverApi = isTauri() ? tauriDiscover : mockDiscover;

export * from "./discover-types";
