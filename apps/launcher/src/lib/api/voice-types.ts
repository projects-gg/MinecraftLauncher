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
  /** VOICE-ROOMS: people at most, when the crew set fewer than the crew room maximum. */
  max: number | null;
  /** VOICE-ROOMS: the player opened it (they may change and delete it). */
  own: boolean;
}

/** One crew's rooms, "Genel" first, then the crew's own order. */
export interface VoiceCrewRooms {
  crew: string;
  rooms: VoiceCrewRoom[];
  /** "Yeni oda" may show. */
  canAdd: boolean;
  /** VOICE-ROOMS: crews arrange their rooms (Studio's "Oda yönetimi"): the rooms' menu, drags. */
  manage: boolean;
  /** VOICE-ROOMS: people at most in a crew room (a room's own limit is below it). */
  crewMax: number;
}

/**
 * VOICE-ROOMS: a change of a crew's rooms from the crew page. The friends service checks the role
 * (leader, moderators, a room's opener), the switch and the crew's day.
 */
export type VoiceCrewOp =
  /** "Odayı düzenle": its name (3–24 characters) and people at most (null: the crew's maximum). */
  | { op: "edit"; crew: string; room: string; name: string; max: number | null }
  /** The crew's rooms ("Genel" aside) in this order. */
  | { op: "order"; crew: string; rooms: string[] }
  /** "Odayı sil": its people go to "Genel"; `empty`: only while nobody is in it. */
  | { op: "remove"; crew: string; room: string; empty: boolean }
  /** A leader or moderator moves a crewmate to another room of the crew. */
  | { op: "move"; crew: string; person: string; room: string };

/** An audio device; `id` is what the settings keep (none there: the system's default). */
export interface VoiceDevice {
  id: string;
  label: string;
}

export interface VoiceDevices {
  inputs: VoiceDevice[];
  outputs: VoiceDevice[];
}

/**
 * Someone in the room the state names with their card (VOICE-S2's `card`, the friends list's own):
 * a friend's friend the lists do not know.
 */
export interface VoiceProfile {
  id: string;
  name: string;
  avatar: string;
  /** Discord's picture when `avatar` is `discord`. */
  avatarUrl?: string | null;
  /** Their profile badges (BADGES). */
  marks?: string[];
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
  /** Names and pictures of people in the room the lists do not know (their cards). */
  profiles?: VoiceProfile[];
  /**
   * Crewmates in a live room of the player's crews (`in`); a friend's own `Friend.voice` comes
   * first. Absent: nothing known (a crewmate who is no friend may still be invited to a crew room
   * while online, voice.md §3.4).
   */
  presence?: Record<string, VoicePresence>;
  /** The person volume slider's top: 200, or 100 while people play through `<audio>`. */
  volumeMax?: number;
  /** Grows when the settings or a person's choices change: the settings backup goes up again. */
  backupRev?: number;
  // VOICE-CALL ---------------------------------------------------------------------------------
  /** "Aramadan önce sor": "Sesli arama başlat" asks first (absent: an older launcher, it asks). */
  askBeforeCall?: boolean;
  /**
   * When someone else first shared the room with the player in this stay (Unix seconds, this
   * computer's clock): the call timer's start. Null while the player has only been alone in it.
   */
  togetherSince?: number | null;
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

/**
 * The live meters (`voice-levels`, VOICE-CALL): the player's microphone and how loud each person
 * speaks, on the settings bar's 0–1 scale; only while the page asks (`setLevels`) and its window is
 * on screen, at most sixteen a second, only on change.
 */
export interface VoiceLevels {
  /** The player's microphone (0 while it is off). */
  me: number;
  /** What it picks up goes out now (else heard but not sent). */
  open: boolean;
  /** People who make sound now; the quiet are left out. */
  people: Array<{ id: string; level: number }>;
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

/** Held with a mute or deafen key ("Ctrl + Shift + M"); left and right alike. */
export type VoiceModifier = "ctrl" | "shift" | "alt";

/**
 * A shortcut's key: a keyboard key by its place on the keyboard (`KeyboardEvent.code`: the same key
 * whatever the layout) with the label the page showed when it was picked, or a mouse button; with
 * the modifiers held with it (mute and deafen only; none or absent: the key alone, which fires
 * whatever else is held).
 */
export type VoiceKey =
  | { kind: "keyboard"; code: string; label: string; mods?: VoiceModifier[] }
  | { kind: "mouse"; button: VoiceMouseButton; mods?: VoiceModifier[] };

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
  /** "Sesli sohbet sesleri" (it was "Katılma ve ayrılma sesleri"; the same value). */
  sounds: boolean;
  /** "Aramadan önce sor" (VOICE-CALL). */
  askBeforeCall: boolean;
  /** "Ses seviyesi": everyone's voice at once, percent 0–200 (VOICE-CALL). */
  outputVolume: number;
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
  askBeforeCall: true,
  outputVolume: 100,
};

/** Which shortcut a key is for. */
export type VoiceShortcut = "pushToTalk" | "mute" | "deafen";

/**
 * What a key would clash with: the key picker's single line. `typing`: a mute or deafen key that
 * writes (a letter, a digit, Space…) without Ctrl or Alt fires while the player types too.
 */
export type VoiceKeyClash =
  | "gameVoiceChat"
  | "overlay"
  | "game"
  | "languageSwitch"
  | "taken"
  | "typing";

export interface VoiceKeyCheck {
  /** The launcher can watch it. */
  usable: boolean;
  clash: VoiceKeyClash | null;
}

/**
 * Discord's push-to-talk, mute and deafen keys on this computer, as ours (VOICE-KEYS), offered once
 * as the player first opens voice chat: "Discord kısayolların kopyalansın mı?".
 */
export interface VoiceDiscordOffer {
  pushToTalk: VoiceKey | null;
  mute: VoiceKey | null;
  deafen: VoiceKey | null;
  /** Discord's microphone works by push-to-talk: copying turns "Bas-konuş" on too. */
  pushToTalkMode: boolean;
  /** Shortcuts Discord has that cannot be used here: the card's one line names them. */
  skipped: VoiceShortcut[];
}

/**
 * What the settings backup carries of voice chat (`voice` in its document): the settings without
 * this computer's devices, and the latest people's volumes and "Benim için sustur". The page only
 * passes it on; the launcher reads, checks and bounds a downloaded one.
 */
export interface VoiceBackup {
  settings?: unknown;
  people?: unknown;
}

// --- the seam ------------------------------------------------------------------------------------

/** Everything the page asks of voice chat. Listeners return their stop function. */
export interface VoiceApi {
  state(): Promise<VoiceState>;
  onState(listener: (state: VoiceState) => void): () => void;
  onSpeaking(listener: (speaking: VoiceSpeaking) => void): () => void;
  onNotice(listener: (notice: VoiceNotice) => void): () => void;
  onLevel(listener: (level: VoiceLevel) => void): () => void;
  /** The live meters' levels (VOICE-CALL), while `setLevels(true)`. */
  onLevels(listener: (levels: VoiceLevels) => void): () => void;
  /** The page shows the live meters now (its window in front with a room), or not. */
  setLevels(on: boolean): Promise<void>;

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
  /** VOICE-ROOMS: a change of the crew's rooms from the crew page. */
  crewChange(op: VoiceCrewOp): Promise<VoiceAnswer>;
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
  /** Discord's keys to offer, once per computer; null: answered before, or nothing to copy. */
  discordOffer(): Promise<VoiceDiscordOffer | null>;
  /** "Kopyala" (`copy`) or "Hayır": never asked again. The settings as kept. */
  discordAnswer(copy: boolean): Promise<VoiceSettings>;
  /** "Mikrofonu dene". */
  testMic(on: boolean): Promise<void>;
  /** The settings page opened or closed (devices and the level bar run only meanwhile). */
  settingsOpen(open: boolean): Promise<void>;
  /** Windows' microphone privacy page ("Ayarları aç"). */
  openMicSettings(): Promise<void>;
  /** The settings backup's voice part (`voice`). */
  backup(): Promise<VoiceBackup>;
  /** A downloaded backup's voice part (this computer's devices stay). */
  restore(backup: VoiceBackup): Promise<void>;
}
