// Voice chat ("Sesli sohbet", VOICE; docs/tasks/voice.md §3–§7): what the launcher's page sees of it
// and what it asks. VOICE-L1 builds these in Rust (`pl_voice::view`, `pl_voice::settings`); this is
// their mirror, field for field, written by VOICE-L3 before the commands existed, so the preview's
// mock (voice-mock.ts) and the app (voice-tauri.ts) answer the same shapes.
//
// People are ids: the page names them from the friend list and the crews, and from `profiles` for
// anyone else (a friend's friend in a friend room). Refusals are answers, never errors.

/** A friend room (a call that grows with invites) or a crew room. */
export type VoiceRoomKind = "friend" | "crew";

/** Where the player's stay is. */
export type VoicePhase =
  /** Getting in (the room's answer, the audio connection). */
  | "joining"
  /** In the room, audio flowing. */
  | "live"
  /** The connection dropped for more than 2 seconds: "Yeniden bağlanıyor…". */
  | "reconnecting";

/** A role in a room; crew rooms follow the crew's roles. */
export type VoiceRole = "leader" | "moderator" | "member";

/** A connection's quality; a mark shows only when it is not `good`. */
export type VoiceQuality = "good" | "weak" | "bad";

/** The microphone as the voice engine saw it last. */
export type VoiceMic = "unknown" | "ok" | "denied" | "missing";

/** The player in the room. */
export interface VoiceMe {
  id: string;
  role: VoiceRole;
  /** "Mikrofonu kapat". */
  mute: boolean;
  /** "Sağırlaştır" (keeps the microphone closed too). */
  deaf: boolean;
  /** A moderator (or Projects' staff) muted the player for everyone. */
  mutedByModerator: boolean;
  /** The microphone sends now. */
  talking: boolean;
  quality: VoiceQuality;
}

/** A leader's or moderator's actions on one person (the menu shows only these). */
export interface VoicePowers {
  /** "Herkes için sustur" and its undo. */
  mute: boolean;
  /** "Odadan çıkar". */
  kick: boolean;
  /** "Taşı" (crew rooms with another live room). */
  moveTo: boolean;
  /** "Moderatör yap" / "Moderatörlükten al" (friend rooms, the leader). */
  moderator: boolean;
}

/** Someone else in the room. */
export interface VoicePerson {
  id: string;
  role: VoiceRole;
  mute: boolean;
  deaf: boolean;
  /** Muted for everyone by a leader or moderator: nobody hears them. */
  mutedByModerator: boolean;
  quality: VoiceQuality;
  /** Speaks now, as of this state (`onSpeaking` brings the changes). */
  speaking: boolean;
  /** Their volume for the player: percent, 0–200, 100 as they arrive; kept on this computer. */
  volume: number;
  /** "Sustur (yalnız sen)"; kept on this computer. */
  mutedForMe: boolean;
  /** When they came in (Unix seconds). */
  joinedAt: number;
  can: VoicePowers;
}

/** The player's invite that still rings. */
export interface VoiceOutRing {
  to: string;
  /** Unix seconds. */
  until: number;
}

/** The room the player is in, or is joining. */
export interface VoiceRoomView {
  id: string;
  kind: VoiceRoomKind;
  /** Crew rooms: the crew. */
  crew: string | null;
  /** Crew rooms always; friend rooms only once the leader named it (else the people's names). */
  name: string | null;
  /** The crew's "Genel": never renamed, never closed. */
  general: boolean;
  /** People at most. */
  max: number;
  phase: VoicePhase;
  me: VoiceMe;
  /** Everyone else, longest present first. */
  people: VoicePerson[];
  /** The player's own invites still ringing ("İptal" beside each). */
  ringing: VoiceOutRing[];
  canRename: boolean;
  canClose: boolean;
}

/** The call the player started, ringing at the friend while the player waits in the room. */
export interface VoiceCalling {
  friend: string;
  /** Unix seconds: the ring stops here. */
  until: number;
}

/** A call or invite ringing for the player. Do-not-disturb rings never come here. */
export interface VoiceRing {
  room: string;
  from: string;
  /** Others in the room, at most 4. */
  with: string[];
  /** Unix seconds. */
  until: number;
}

/** A missed call: "Katıl" while `joinable`, else "Geri ara". */
export interface VoiceMissed {
  room: string;
  from: string;
  /** Unix seconds. */
  at: number;
  joinable: boolean;
}

export interface VoiceCrewRoom {
  id: string;
  name: string;
  general: boolean;
  /** Who is in it now (blocked people left out). */
  people: string[];
}

/** One crew's live rooms. */
export interface VoiceCrewRooms {
  crew: string;
  rooms: VoiceCrewRoom[];
  /** "Yeni oda" may show. */
  canAdd: boolean;
}

/** An audio device; `id` is what the settings keep (none there: the system's default). */
export interface VoiceDevice {
  id: string;
  label: string;
}

export interface VoiceDevices {
  inputs: VoiceDevice[];
  outputs: VoiceDevice[];
}

/** Someone the state names who is neither a friend nor a crewmate (voice.md "L3 → L1 istek"). */
export interface VoiceProfile {
  id: string;
  name: string;
  avatar: string;
  avatarUrl?: string;
}

/** A friend's or crewmate's voice chat: may be called (`ready`) or is in a room (`in`). */
export type VoicePresence = "ready" | "in";

/** Voice chat now, whole (`voice-state` after every change, `voice_state` at the start). */
export interface VoiceState {
  /** Voice chat may be offered at all (signed config, Studio, the friends service, signed in).
   * False: nothing of it anywhere, not even the settings page. */
  available: boolean;
  /** The player's own switch (Ayarlar → Sesli sohbet). Off: no "Ara", no ring. */
  enabled: boolean;
  room: VoiceRoomView | null;
  calling: VoiceCalling | null;
  /** Oldest first. */
  rings: VoiceRing[];
  /** Newest first, at most 10. */
  missed: VoiceMissed[];
  /** A crew without live rooms has no entry (its "Genel" shows with "Katıl"). */
  crews: VoiceCrewRooms[];
  /** Rooms a crew may have, "Genel" included. */
  crewRoomsMax: number;
  mic: VoiceMic;
  /** Known while the voice engine runs (in a room, or while the settings page is open). */
  devices: VoiceDevices;
  /** "Mikrofonu dene" runs. */
  testing: boolean;
  /** Names and pictures of people the lists do not know (requested of L1). */
  profiles?: VoiceProfile[];
  /** Friends' and crewmates' voice chat (requested of L1); absent: cannot be called. */
  presence?: Record<string, VoicePresence>;
}

/** Nothing at all: before the service and the signed config said anything. */
export const HIDDEN_VOICE_STATE: VoiceState = {
  available: false,
  enabled: true,
  room: null,
  calling: null,
  rings: [],
  missed: [],
  crews: [],
  crewRoomsMax: 3,
  mic: "unknown",
  devices: { inputs: [], outputs: [] },
  testing: false,
};

/** Who speaks now (`voice-speaking`; at most ten a second, only while the window is on screen). */
export interface VoiceSpeaking {
  ids: string[];
  /** The player's microphone sends now. */
  me: boolean;
}

/** The microphone's level for the settings page (`voice-level`, while it is open). */
export interface VoiceLevel {
  /** 0 to 1. */
  level: number;
  /** Above the sensitivity: this would be sent. */
  open: boolean;
}

/** Why the player's stay ended. */
export type VoiceLeftReason =
  | "closed"
  | "alone"
  | "noAnswer"
  | "elsewhere"
  | "kicked"
  | "crew"
  | "idle"
  | "unavailable"
  | "failed";

/** A moment worth a toast (`voice-notice`). */
export type VoiceNotice =
  | { kind: "left"; reason: VoiceLeftReason; friend: string | null }
  | { kind: "moved"; room: string; name: string }
  | { kind: "mutedByModerator" }
  | { kind: "missed"; from: string; room: string }
  | { kind: "micChanged" }
  | { kind: "mic"; mic: VoiceMic };

/** Why an action was refused. */
export type VoiceRefusal =
  | "off"
  | "unreachable"
  | "rateLimited"
  | "accountLimited"
  | "staffMuted"
  | "roomFull"
  | "roomsFull"
  | "roomGone"
  | "kicked"
  | "notMember"
  | "notAllowed"
  | "notFound"
  | "invalidName"
  | "nameRejected"
  | "failed"
  | "nothing";

/** What an action got: no `refused` means it went through. */
export interface VoiceAnswer {
  refused: VoiceRefusal | null;
  /** Seconds to wait (`rateLimited`, `accountLimited`). */
  retryAfter: number | null;
  /** Unix seconds (`accountLimited`, `staffMuted`). */
  until: number | null;
}

export const VOICE_OK: VoiceAnswer = { refused: null, retryAfter: null, until: null };

/** A leader's or moderator's action. */
export type VoiceModAction =
  | { act: "mute"; person: string }
  | { act: "unmute"; person: string }
  | { act: "kick"; person: string }
  | { act: "unkick"; person: string }
  | { act: "move"; person: string; room: string }
  | { act: "mod"; person: string }
  | { act: "unmod"; person: string }
  | { act: "rename"; name: string }
  | { act: "close" };

// --- settings ------------------------------------------------------------------------------------

/** When the microphone sends: "Konuşunca" or "Bas-konuş". */
export type VoiceTalkMode = "voice" | "pushToTalk";

/** A mouse button a shortcut may use (never the left or right one). */
export type VoiceMouseButton = "middle" | "back" | "forward";

/**
 * A shortcut's key: a keyboard key by its place on the keyboard (`KeyboardEvent.code`: the same key
 * whatever the layout) with the label the page showed when it was picked, or a mouse button.
 */
export type VoiceKey =
  | { kind: "keyboard"; code: string; label: string }
  | { kind: "mouse"; button: VoiceMouseButton };

/** Ayarlar → Sesli sohbet. */
export interface VoiceSettings {
  enabled: boolean;
  /** None: the system's default. This computer only. */
  inputDevice: string | null;
  outputDevice: string | null;
  mode: VoiceTalkMode;
  pushToTalkKey: VoiceKey | null;
  muteKey: VoiceKey | null;
  deafenKey: VoiceKey | null;
  noiseSuppression: boolean;
  echoCancellation: boolean;
  autoGain: boolean;
  /** None: "Otomatik"; else the threshold, 0 (everything goes) to 100. */
  sensitivity: number | null;
  /** Percent, 0–200, while automatic volume is off. */
  micVolume: number;
  sounds: boolean;
}

export const DEFAULT_VOICE_SETTINGS: VoiceSettings = {
  enabled: true,
  inputDevice: null,
  outputDevice: null,
  mode: "voice",
  pushToTalkKey: null,
  muteKey: null,
  deafenKey: null,
  noiseSuppression: true,
  echoCancellation: true,
  autoGain: true,
  sensitivity: null,
  micVolume: 100,
  sounds: true,
};

/** Which shortcut a key is for. */
export type VoiceShortcut = "pushToTalk" | "mute" | "deafen";

/** What a key would clash with: the key picker's single line. */
export type VoiceKeyClash = "gameVoiceChat" | "overlay" | "game" | "languageSwitch" | "taken";

export interface VoiceKeyCheck {
  /** The launcher can watch it. */
  usable: boolean;
  clash: VoiceKeyClash | null;
}

// --- the seam ------------------------------------------------------------------------------------

/** Everything the page asks of voice chat. Listeners return their stop function. */
export interface VoiceApi {
  state(): Promise<VoiceState>;
  onState(listener: (state: VoiceState) => void): () => void;
  onSpeaking(listener: (speaking: VoiceSpeaking) => void): () => void;
  onNotice(listener: (notice: VoiceNotice) => void): () => void;
  onLevel(listener: (level: VoiceLevel) => void): () => void;

  /** "Ara": a new friend room, ringing the friend. */
  call(friend: string): Promise<VoiceAnswer>;
  /** "Katıl" on a ring or a missed call. */
  answer(room: string): Promise<VoiceAnswer>;
  /** "Reddet". */
  decline(room: string): Promise<VoiceAnswer>;
  dismissMissed(room: string): Promise<void>;
  /** A crew's room ("Genel" when `room` is null). */
  joinCrew(crew: string, room: string | null): Promise<VoiceAnswer>;
  /** "Yeni oda": opens one and joins it. */
  newCrewRoom(crew: string): Promise<VoiceAnswer>;
  /** "Ayrıl" (also "İptal" while a call rings: the room ends with the player). */
  leave(): Promise<void>;
  /** "Tekrar dene" after "Bağlanamadı.". */
  retry(): Promise<VoiceAnswer>;
  setMute(mute: boolean): Promise<void>;
  setDeaf(deaf: boolean): Promise<void>;
  /** "Davet et" / "Odaya davet et": rings them into the player's room. */
  invite(friend: string): Promise<VoiceAnswer>;
  /** "İptal" beside an invite that rings. */
  cancelInvite(friend: string): Promise<VoiceAnswer>;
  setVolume(person: string, volume: number): Promise<void>;
  setMutedForMe(person: string, muted: boolean): Promise<void>;
  moderate(action: VoiceModAction): Promise<VoiceAnswer>;

  settings(): Promise<VoiceSettings>;
  saveSettings(settings: VoiceSettings): Promise<VoiceSettings>;
  checkKey(key: VoiceKey, shortcut: VoiceShortcut): Promise<VoiceKeyCheck>;
  /** "Mikrofonu dene". */
  testMic(on: boolean): Promise<void>;
  /** The settings page opened or closed (devices and the level bar run only meanwhile). */
  settingsOpen(open: boolean): Promise<void>;
  /** Windows' microphone privacy page ("Ayarları aç"). */
  openMicSettings(): Promise<void>;
}
