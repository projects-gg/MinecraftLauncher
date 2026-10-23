// `projectsSkinApi` (SKIN1): the app's commands, or the browser preview's mock. Kept apart from
// `api` (index.ts) so the feature's seam stays in its own files until SKIN1-R wires the backend.
import { isTauri } from "../platform";
import { mockProjectsSkin } from "./projects-skin-mock";
import { tauriProjectsSkin } from "./projects-skin-tauri";
import type { ProjectsSkinApi } from "./projects-skin-types";

export const projectsSkinApi: ProjectsSkinApi = isTauri() ? tauriProjectsSkin : mockProjectsSkin;

export * from "./projects-skin-types";
