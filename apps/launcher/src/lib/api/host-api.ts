// `hostApi` (HOST1, "Dünyanı arkadaşlarına aç"): the app's commands, or the browser preview's mock.
// Kept apart from `api` (index.ts) so the feature's seam stays in its own files until HOST1-L1 wires
// the backend.
import { isTauri } from "../platform";
import { mockHost } from "./host-mock";
import { tauriHost } from "./host-tauri";
import type { HostApi } from "./host-types";

export const hostApi: HostApi = isTauri() ? tauriHost : mockHost;

export * from "./host-types";
