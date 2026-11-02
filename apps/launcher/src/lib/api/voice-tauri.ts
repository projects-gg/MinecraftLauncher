// The app's side of `voiceApi` (VOICE-L1's commands and events: `voice_state`, `voice-state`,
// `voice-speaking`, `voice-notice`, `voice-level`, …). Until L1's commands are in the bindings
// (voice.md "L1 API hazır"), voice chat stays hidden in the app: the page shows nothing of it, asks
// nothing, and every action answers "off".
import {
  DEFAULT_VOICE_SETTINGS,
  HIDDEN_VOICE_STATE,
  type VoiceAnswer,
  type VoiceApi,
} from "./voice-types";

const off: VoiceAnswer = { refused: "off", retryAfter: null, until: null };
const quiet = () => () => {};
const refuse = async (): Promise<VoiceAnswer> => off;
const nothing = async (): Promise<void> => {};

export const tauriVoice: VoiceApi = {
  state: async () => HIDDEN_VOICE_STATE,
  onState: quiet,
  onSpeaking: quiet,
  onNotice: quiet,
  onLevel: quiet,
  call: refuse,
  answer: refuse,
  decline: refuse,
  dismissMissed: nothing,
  joinCrew: refuse,
  newCrewRoom: refuse,
  leave: nothing,
  retry: refuse,
  setMute: nothing,
  setDeaf: nothing,
  invite: refuse,
  cancelInvite: refuse,
  setVolume: nothing,
  setMutedForMe: nothing,
  moderate: refuse,
  settings: async () => DEFAULT_VOICE_SETTINGS,
  saveSettings: async (settings) => settings,
  checkKey: async () => ({ usable: true, clash: null }),
  testMic: nothing,
  settingsOpen: nothing,
  openMicSettings: nothing,
};
