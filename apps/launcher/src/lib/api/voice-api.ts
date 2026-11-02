// `voiceApi` (VOICE, "Sesli sohbet"): the app's commands, or the browser preview's mock. Kept apart
// from `api` (index.ts) so the feature's seam stays in its own files, as world sharing's does.
import { isTauri } from "../platform";
import { mockVoice } from "./voice-mock";
import { tauriVoice } from "./voice-tauri";
import type { VoiceApi } from "./voice-types";

export const voiceApi: VoiceApi = isTauri() ? tauriVoice : mockVoice;

export * from "./voice-types";
