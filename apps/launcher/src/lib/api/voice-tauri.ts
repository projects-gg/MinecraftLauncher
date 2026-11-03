// The app's side of `voiceApi`: VOICE-L1's commands (`commands.voice…`) and events (`voice-state`,
// `voice-speaking`, `voice-notice`, `voice-level`; voice.md "L1 API (VOICE-L3 için)"). The backend
// builds its views field for field like voice-types.ts, so each answer is the command's data as it
// is; a key's label is optional on the wire (an older settings file), so the page fills it in. A
// failed command rejects with a `VoiceCommandError` (`message` in the player's language; `code`
// and `details` for support); refusals are answers, never errors.
import { listen } from "@tauri-apps/api/event";
import {
  type VoiceBackup as BindingBackup,
  type VoiceKey as BindingKey,
  type VoiceSettings as BindingSettings,
  type CmdError,
  commands,
} from "@/bindings";
import {
  DEFAULT_VOICE_SETTINGS,
  VOICE_OK,
  type VoiceApi,
  type VoiceBackup,
  type VoiceKey,
  type VoiceLevel,
  type VoiceLevels,
  type VoiceNotice,
  type VoiceSettings,
  type VoiceSpeaking,
  type VoiceState,
} from "./voice-types";

/** A failed command: friendly `message`, plus `code`/`details` for "Ayrıntıları kopyala". */
export class VoiceCommandError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(error: CmdError) {
    super(error.message);
    this.code = error.code;
    this.details = error.details;
  }
}

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new VoiceCommandError(result.error);
  return result.data;
}

/** A command that answers nothing (`null` on the wire). */
async function done(promise: Promise<Res<null>>): Promise<void> {
  await ok(promise);
}

/** One event's payloads; returns its stop. */
function on<T>(event: string, listener: (payload: T) => void): () => void {
  const unlisten = listen<T>(event, (e) => listener(e.payload));
  return () => {
    void unlisten.then((stop) => stop());
  };
}

const asKey = (key: BindingKey | null): VoiceKey | null =>
  key === null ? null : key.kind === "keyboard" ? { ...key, label: key.label ?? "" } : key;

/** The settings with every field set (each is optional on the wire: Rust fills its defaults). */
function asSettings(s: BindingSettings): VoiceSettings {
  const d = DEFAULT_VOICE_SETTINGS;
  return {
    enabled: s.enabled ?? d.enabled,
    inputDevice: s.inputDevice ?? null,
    outputDevice: s.outputDevice ?? null,
    mode: s.mode ?? d.mode,
    pushToTalkKey: asKey(s.pushToTalkKey ?? null),
    muteKey: asKey(s.muteKey ?? null),
    deafenKey: asKey(s.deafenKey ?? null),
    noiseSuppression: s.noiseSuppression ?? d.noiseSuppression,
    echoCancellation: s.echoCancellation ?? d.echoCancellation,
    autoGain: s.autoGain ?? d.autoGain,
    sensitivity: s.sensitivity ?? null,
    micVolume: s.micVolume ?? d.micVolume,
    sounds: s.sounds ?? d.sounds,
    askBeforeCall: s.askBeforeCall ?? d.askBeforeCall,
    outputVolume: s.outputVolume ?? d.outputVolume,
  };
}

export const tauriVoice: VoiceApi = {
  state: () => ok(commands.voiceState()),
  onState: (listener) => on<VoiceState>("voice-state", listener),
  onSpeaking: (listener) => on<VoiceSpeaking>("voice-speaking", listener),
  onNotice: (listener) => on<VoiceNotice>("voice-notice", listener),
  onLevel: (listener) => on<VoiceLevel>("voice-level", listener),
  onLevels: (listener) => on<VoiceLevels>("voice-levels", listener),
  setLevels: (shown) => done(commands.voiceLevels(shown)),

  call: (friend) => ok(commands.voiceCall(friend)),
  answer: (room) => ok(commands.voiceAccept(room)),
  decline: async (room) => {
    await done(commands.voiceDecline(room));
    return VOICE_OK;
  },
  dismissMissed: (room) => done(commands.voiceDismissMissed(room)),
  joinCrew: (crew, room) => ok(commands.voiceJoinCrew(crew, room)),
  newCrewRoom: (crew) => ok(commands.voiceNewCrewRoom(crew)),
  leave: () => done(commands.voiceLeave()),
  retry: () => ok(commands.voiceRetry()),
  setMute: (mute) => done(commands.voiceSetMute(mute)),
  setDeaf: (deaf) => done(commands.voiceSetDeaf(deaf)),
  invite: (friend) => ok(commands.voiceInvite(friend)),
  cancelInvite: (friend) => ok(commands.voiceCancelInvite(friend)),
  setVolume: (person, volume) => done(commands.voiceSetVolume(person, volume)),
  setMutedForMe: (person, muted) => done(commands.voiceSetMutedForMe(person, muted)),
  moderate: (action) => ok(commands.voiceModerate(action)),

  settings: async () => asSettings(await ok(commands.voiceSettings())),
  saveSettings: async (settings) => asSettings(await ok(commands.voiceSaveSettings(settings))),
  checkKey: (key, shortcut) => ok(commands.voiceCheckKey({ key, shortcut })),
  discordOffer: async () => {
    const offer = await ok(commands.voiceDiscordOffer());
    return offer
      ? {
          pushToTalk: asKey(offer.pushToTalk ?? null),
          mute: asKey(offer.mute ?? null),
          deafen: asKey(offer.deafen ?? null),
          pushToTalkMode: offer.pushToTalkMode,
          skipped: offer.skipped,
        }
      : null;
  },
  discordAnswer: async (copy) => asSettings(await ok(commands.voiceDiscordAnswer(copy))),
  testMic: (on) => done(commands.voiceSelfTest(on)),
  settingsOpen: (open) => done(commands.voiceSetup(open)),
  openMicSettings: () => done(commands.voiceOpenMicSettings()),
  backup: (): Promise<VoiceBackup> => ok(commands.voiceBackup()),
  // The launcher reads it (serde with its defaults) and bounds it; a part it cannot read fails
  // alone, the rest of the backup still applies.
  restore: (backup) => done(commands.voiceRestore(backup as BindingBackup)),
};
