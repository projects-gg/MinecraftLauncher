// What voice chat's surfaces decide (VOICE-L3; voice.md §3–§5), apart from React so the tests read
// them: who a face is, what a room is called, what a friend's card offers ("Ara", "Odaya davet et"),
// who "Davet et" lists, the words for a refusal or an ended stay, and a shortcut key's label.
import { matchesQuery } from "@pl/ui";
import type { TFunction } from "i18next";
import { untilText } from "@/features/social/crew-notify";
import type { Friend, SocialSnapshot, SocialTeam, TeamMember } from "@/lib/api/social-types";
import type {
  VoiceAnswer,
  VoiceCrewRoom,
  VoiceCrewRooms,
  VoiceKey,
  VoiceKeyCheck,
  VoiceLeftReason,
  VoiceModifier,
  VoicePerson,
  VoicePresence,
  VoiceRole,
  VoiceRoomView,
  VoiceShortcut,
  VoiceState,
} from "@/lib/api/voice-types";

/** Someone as the voice surfaces draw them. */
export interface VoiceFace {
  id: string;
  name: string;
  avatar?: string;
  avatarUrl?: string;
  /** Their profile badges (friends, crewmates, and people a room's card names). */
  marks?: string[];
  /** On the player's friend list. */
  friend: boolean;
}

/** People the friends lists know: friends first, then crewmates. */
export type Lists = Pick<SocialSnapshot, "friends" | "crews">;

/**
 * A face for an id: a friend, a crewmate, someone the voice state names (`profiles`), else "Bir
 * oyuncu" with a lettered head (never an id on screen).
 */
export function faceOf(
  id: string,
  lists: Lists,
  state: Pick<VoiceState, "profiles">,
  someone: string,
): VoiceFace {
  const friend = lists.friends.find((f) => f.id === id);
  if (friend) {
    return {
      id,
      name: friend.name,
      avatar: friend.avatar,
      avatarUrl: friend.avatarUrl,
      marks: friend.marks,
      friend: true,
    };
  }
  for (const crew of lists.crews) {
    const member = crew.members.find((m) => m.id === id);
    if (member) {
      return {
        id,
        name: member.name,
        avatar: member.avatar,
        avatarUrl: member.avatarUrl,
        marks: member.marks,
        friend: false,
      };
    }
  }
  // A friend's friend, named by the card the room sent (VOICE-S2).
  const profile = state.profiles?.find((p) => p.id === id);
  if (profile) {
    return {
      id,
      name: profile.name,
      avatar: profile.avatar,
      avatarUrl: profile.avatarUrl ?? undefined,
      marks: profile.marks?.length ? profile.marks : undefined,
      friend: false,
    };
  }
  return { id, name: someone, friend: false };
}

/** Names in a line: "A, B, C", or "A, B ve 2 kişi daha" past three. */
export function namesLine(names: readonly string[], t: TFunction): string {
  if (names.length <= 3) return names.join(", ");
  return t("voice.names.more", { names: names.slice(0, 2).join(", "), count: names.length - 2 });
}

/**
 * A room's own name as shown. The friends service names a crew's new rooms in Turkish ("Oda 2",
 * "Oda 3"; social `voice-hub.ts` `crewNew`): a name of that form is said in the player's language
 * ("Room 2"); any other name is the leader's own words.
 */
export function shownRoomName(name: string, t: TFunction): string {
  const numbered = /^Oda (\d{1,3})$/.exec(name);
  return numbered ? t("voice.room.numbered", { n: Number(numbered[1]) }) : name;
}

/** A crew room's name as shown: the crew's "Genel" in the player's language, else its own. */
export function crewRoomName(
  room: { name: string | null; general: boolean },
  t: TFunction,
): string {
  return room.general ? t("voice.room.general") : shownRoomName(room.name ?? "", t);
}

/**
 * A room's title: its name; a friend room without one, who is in it ("Kaan_07, Ece_x"), or while
 * the player waits alone, the friend being called; else "Yalnız sen".
 */
export function roomTitle(
  room: Pick<VoiceRoomView, "name" | "people" | "general">,
  nameOf: (id: string) => string,
  t: TFunction,
  calling?: string | null,
): string {
  if (room.general) return t("voice.room.general");
  if (room.name) return shownRoomName(room.name, t);
  if (room.people.length)
    return namesLine(
      room.people.map((p) => nameOf(p.id)),
      t,
    );
  if (calling) return nameOf(calling);
  return t("voice.justYou");
}

/**
 * What someone's own choice turned off, for the sign beside them (VOICE-KEYS; voice.md §5.1):
 * `deaf` (the headphones off, which closes the microphone too: one sign, as Discord shows it),
 * `mute` (the microphone), or null while both are on (no sign at all).
 */
export function voiceMark(p: Pick<VoicePerson, "mute" | "deaf">): "deaf" | "mute" | null {
  if (p.deaf) return "deaf";
  if (p.mute) return "mute";
  return null;
}

/** A small face's sign: a moderator's mute, deafened, or the microphone off. */
export type VoiceFaceMark = "moderator" | "deaf" | "mute";

/**
 * The one sign on a small face (the title bar's pill): a moderator's mute first (it holds whatever
 * they choose), then theirs.
 */
export function faceMark(
  p: Pick<VoicePerson, "mute" | "deaf" | "mutedByModerator">,
): VoiceFaceMark | null {
  return p.mutedByModerator ? "moderator" : voiceMark(p);
}

/** The faces the title bar's pill shows: up to three others, longest present first. */
export function pillPeople(room: Pick<VoiceRoomView, "people">): VoicePerson[] {
  return room.people.slice(0, 3);
}

/** A role's small word in lists ("Lider", "Moderatör"); members carry none. */
export function roleWord(role: VoiceRole, t: TFunction): string | null {
  if (role === "leader") return t("voice.room.leader");
  if (role === "moderator") return t("voice.room.moderator");
  return null;
}

/**
 * What someone's card, menu and chat header offer (voice.md §3.2–§3.4):
 * - `call`: "Ara" (a mutual friend, while the player is in no room or in a crew room the friend
 *   is not in the crew of: calling leaves it, as any other room's "Katıl" does);
 * - `invite`: "Odaya davet et" (a friend into the player's friend room, a crewmate into the
 *   player's room of that crew);
 * - `ringing`: the player's call or invite rings at them ("Aranıyor…", off);
 * - `full`: the room has no place left ("Odaya davet et", off);
 * - null: nothing (voice chat off, they cannot be called, or they are in the room already).
 */
export type CallOffer = "call" | "invite" | "ringing" | "full";

/**
 * Someone's voice chat: the friend list's own (`Friend.voice`, from `presence.voice`), else what
 * the voice state knows (crewmates in a crew room; the preview); undefined: not known.
 */
export function presenceOf(
  id: string,
  state: Pick<VoiceState, "presence">,
  friends: ReadonlyArray<Pick<Friend, "id" | "voice">>,
): VoicePresence | undefined {
  return friends.find((f) => f.id === id)?.voice ?? state.presence?.[id];
}

/**
 * `online`: a crewmate is on Projects now (the crew's member list). The service tells no one's
 * voice chat outside friends, so a crewmate who is no friend may be invited to the player's room of
 * that crew while online (voice.md §3.4: "ekip odasında çevrimiçi ekip arkadaşların"); the ring
 * itself says whether it reached them.
 */
export function callOffer(
  state: Pick<VoiceState, "available" | "enabled" | "room" | "calling" | "presence">,
  id: string,
  friend: boolean,
  crewMembers: (crew: string) => readonly string[],
  friends: ReadonlyArray<Pick<Friend, "id" | "voice">> = [],
  online: (id: string) => boolean = () => false,
): CallOffer | null {
  if (!state.available || !state.enabled) return null;
  const room = state.room;
  const crewmate =
    room?.kind === "crew" && room.crew !== null && crewMembers(room.crew).includes(id);
  if (!presenceOf(id, state, friends) && !(crewmate && !friend && online(id))) return null;
  if (state.calling?.friend === id) return "ringing";
  if (!room) return friend ? "call" : null;
  if (room.people.some((p) => p.id === id)) return null;
  if (room.ringing.some((r) => r.to === id)) return "ringing";
  if (room.kind === "crew" && !crewmate) return friend ? "call" : null;
  if (room.kind === "friend" && !friend) return null;
  return roomFull(room) ? "full" : "invite";
}

/** No place left: the people, the player and the invites still ringing fill it. */
export function roomFull(room: Pick<VoiceRoomView, "people" | "ringing" | "max">): boolean {
  return room.people.length + 1 + room.ringing.length >= room.max;
}

/** Someone "Davet et" lists. */
export interface Invitable extends VoiceFace {
  /** The player's invite rings at them now ("Aranıyor…"). */
  ringing: boolean;
}

/**
 * Who "Davet et" lists: in a friend room the player's friends who may be called, in a crew room the
 * crew's members who may, or who are online when they are no friends (their voice chat is not
 * told; voice.md §3.4); never someone in the room. Filtered by name as typed (Turkish letters
 * folded), in name order.
 */
export function invitables(
  state: Pick<VoiceState, "room" | "presence" | "profiles">,
  lists: Lists,
  query: string,
): Invitable[] {
  const room = state.room;
  if (!room) return [];
  const inRoom = new Set([room.me.id, ...room.people.map((p) => p.id)]);
  const ringing = new Set(room.ringing.map((r) => r.to));
  const may = (id: string) => !inRoom.has(id) && Boolean(presenceOf(id, state, lists.friends));
  let ids: string[];
  if (room.kind === "crew") {
    const crew = lists.crews.find((c) => c.id === room.crew);
    ids = (crew?.members ?? [])
      .filter(
        (m) =>
          !m.me &&
          !m.blocked &&
          (may(m.id) || (!m.friend && !inRoom.has(m.id) && m.state !== "offline")),
      )
      .map((m) => m.id);
  } else {
    ids = lists.friends.filter((f) => may(f.id)).map((f) => f.id);
  }
  return ids
    .map((id) => ({ ...faceOf(id, lists, state, ""), ringing: ringing.has(id) }))
    .filter((face) => !query.trim() || matchesQuery(face.name, query))
    .sort((a, b) => a.name.localeCompare(b.name, "tr"));
}

/** A rate limit this long is the day's quota ("Bugünlük arama hakkın doldu."). */
export const DAILY_LIMIT_SECONDS = 600;

const NO_BREAK = String.fromCharCode(0xa0);

/** BAN-TYPES: a voice chat ban's end (Unix seconds) with its date on one line ("5 Kasım 14:00'e
 * kadar": a toast's line break once fell between the day and the month). */
function banEnd(t: TFunction, until: number, nowMs: number): string {
  return untilText(t, until * 1000, nowMs).replace(/(\d) (\p{L})/gu, `$1${NO_BREAK}$2`);
}

/**
 * The words for a refused action, or null when it went through (or there was nothing to say:
 * `nothing`). `name` is who the action was about ("… şu an aranamıyor."). BAN-TYPES: a voice chat
 * ban with its end says until when ("5 Kasım 14:00'e kadar", as of `nowMs`).
 */
export function refusalText(
  answer: VoiceAnswer,
  t: TFunction,
  name?: string,
  nowMs = Date.now(),
): string | null {
  switch (answer.refused) {
    case null:
    case "nothing":
      return null;
    case "unreachable":
      return t("voice.refused.unreachable", { name: name ?? t("voice.someone") });
    case "rateLimited":
      return (answer.retryAfter ?? 0) >= DAILY_LIMIT_SECONDS
        ? t("voice.refused.daily")
        : t("voice.refused.rateLimited");
    case "accountLimited":
      return t("voice.refused.daily");
    case "staffMuted":
      return answer.until
        ? t("voice.refused.staffMutedUntil", { until: banEnd(t, answer.until, nowMs) })
        : t("voice.refused.staffMuted");
    default:
      return t(`voice.refused.${answer.refused}`);
  }
}

/**
 * The words for a stay that ended on its own (the toast). BAN-TYPES: `restricted` with `until`
 * (Unix seconds) says until when, as of `nowMs`.
 */
export function leftText(
  reason: VoiceLeftReason,
  t: TFunction,
  until?: number | null,
  nowMs = Date.now(),
): string {
  // "Cevap yok" is the pill's own (with "Tekrar ara"), as is "Bağlanamadı" (with "Tekrar dene").
  if (reason === "noAnswer") return t("voice.call.noAnswer");
  if (reason === "restricted" && until)
    return t("voice.left.restrictedUntil", { until: banEnd(t, until, nowMs) });
  return t(`voice.left.${reason}`);
}

/** Labels of the keys whose `KeyboardEvent.key` says nothing useful. */
const NAMED_KEYS: Record<string, string> = {
  Space: "voice.key.space",
  CapsLock: "voice.key.capsLock",
  ShiftLeft: "voice.key.shiftLeft",
  ShiftRight: "voice.key.shiftRight",
  ControlLeft: "voice.key.controlLeft",
  ControlRight: "voice.key.controlRight",
  AltLeft: "voice.key.altLeft",
  AltRight: "voice.key.altRight",
};

const MOUSE_KEYS = {
  middle: "voice.key.mouseMiddle",
  back: "voice.key.mouseBack",
  forward: "voice.key.mouseForward",
} as const;

/** Modifiers in the order they are written ("Ctrl + Shift + Alt + M"). */
export const MODIFIERS: readonly VoiceModifier[] = ["ctrl", "shift", "alt"];

/** The modifier a key is ("ShiftRight" → "shift"), if it is one. */
export function modifierOf(code: string): VoiceModifier | null {
  if (code === "ControlLeft" || code === "ControlRight") return "ctrl";
  if (code === "ShiftLeft" || code === "ShiftRight") return "shift";
  if (code === "AltLeft" || code === "AltRight") return "alt";
  return null;
}

/** A modifier's name on its cap: the same word in both languages. */
export function modifierLabel(mod: VoiceModifier, t: TFunction): string {
  return t(`voice.key.mod.${mod}`);
}

/** A key's caps, modifiers first ("Ctrl", "Shift", "M"; "Fare 4"). */
export function keyParts(key: VoiceKey, t: TFunction): string[] {
  const mods = MODIFIERS.filter((m) => key.mods?.includes(m)).map((m) => modifierLabel(m, t));
  if (key.kind === "mouse") return [...mods, t(MOUSE_KEYS[key.button])];
  const named = NAMED_KEYS[key.code];
  const label = named ? t(named) : key.label || codeLabel(key.code);
  // The number pad's keys say so ("Num *", "Num 1"): the main keys carry the same signs.
  return [...mods, key.code.startsWith("Numpad") && label.length === 1 ? `Num ${label}` : label];
}

/** A key's label in words ("Caps Lock", "V", "Ctrl + Shift + M", "Fare 4"). */
export function keyLabel(key: VoiceKey, t: TFunction): string {
  return keyParts(key, t).join(" + ");
}

/** What a mute or deafen picker records with a key: the modifiers held (Ctrl/Shift/Alt). */
export function heldModifiers(
  event: Pick<KeyboardEvent, "ctrlKey" | "shiftKey" | "altKey"> & {
    getModifierState?: (key: string) => boolean;
  },
): VoiceModifier[] {
  // AltGr (Right Alt, with the Left Ctrl Windows adds) types a character ("@" on a Turkish Q):
  // neither counts.
  const altGr = event.getModifierState?.("AltGraph") ?? false;
  const held: VoiceModifier[] = [];
  if (event.ctrlKey && !altGr) held.push("ctrl");
  if (event.shiftKey) held.push("shift");
  if (event.altKey && !altGr) held.push("alt");
  return held;
}

/** Whether a shortcut takes Ctrl, Shift or Alt with its key (push-to-talk is one key). */
export function takesModifiers(shortcut: VoiceShortcut): boolean {
  return shortcut !== "pushToTalk";
}

/** A readable name from a `KeyboardEvent.code` alone ("KeyV" → "V", "Numpad1" → "Num 1"). */
export function codeLabel(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^Numpad\d$/.test(code)) return `Num ${code.slice(6)}`;
  return code.replace(/(Left|Right)$/, "");
}

/** The keyboard layout's own character per key place (`navigator.keyboard.getLayoutMap()`). */
export type LayoutMap = ReadonlyMap<string, string>;

/** This keyboard's layout now, where the browser tells it (WebView2 does); null elsewhere. */
export async function keyboardLayout(): Promise<LayoutMap | null> {
  const keyboard = (
    navigator as Navigator & { keyboard?: { getLayoutMap?: () => Promise<LayoutMap> } }
  ).keyboard;
  try {
    return (await keyboard?.getLayoutMap?.()) ?? null;
  } catch {
    return null;
  }
}

/**
 * The key a press picked, or null for one that cannot be a shortcut (Escape cancels the pick; the
 * Windows key opens the Start menu). The label is the layout's own letter ("Ş" on a Turkish Q),
 * upper-cased in the interface's language: from the layout itself when known (Shift or AltGr
 * change what the press typed: "!" is still the 1 key), else from the press. `mods` are kept with
 * it (a mute or deafen combination).
 */
export function keyFromEvent(
  event: Pick<KeyboardEvent, "code" | "key">,
  lang: string,
  mods: readonly VoiceModifier[] = [],
  layout: LayoutMap | null = null,
): VoiceKey | null {
  const { code, key } = event;
  if (!code || code === "Escape" || code.startsWith("Meta") || code === "Unidentified") return null;
  const typed = layout?.get(code) ?? (mods.length ? null : key);
  const label =
    typed && typed.length === 1 && typed !== " "
      ? typed.toLocaleUpperCase(lang)
      : /^Key[A-Z]$/.test(code) && key.length === 1 && /\p{L}/u.test(key)
        ? key.toLocaleUpperCase(lang)
        : codeLabel(code);
  return mods.length
    ? { kind: "keyboard", code, label, mods: [...mods] }
    : { kind: "keyboard", code, label };
}

/** The mouse button a press picked (`MouseEvent.button`): the wheel or a side button. */
export function keyFromMouse(button: number, mods: readonly VoiceModifier[] = []): VoiceKey | null {
  const withMods = (key: VoiceKey): VoiceKey => (mods.length ? { ...key, mods: [...mods] } : key);
  if (button === 1) return withMods({ kind: "mouse", button: "middle" });
  if (button === 3) return withMods({ kind: "mouse", button: "back" });
  if (button === 4) return withMods({ kind: "mouse", button: "forward" });
  return null;
}

/**
 * The key picker's one line about a key, or null when there is nothing to say; a key that writes
 * says what it would do while the player types (mute or deafen them).
 */
export function clashText(
  check: VoiceKeyCheck,
  t: TFunction,
  shortcut?: VoiceShortcut,
): string | null {
  if (!check.usable) return t("voice.key.clash.unusable");
  if (check.clash === "typing" && shortcut === "mute") return t("voice.key.clash.typingMute");
  if (check.clash === "typing" && shortcut === "deafen") return t("voice.key.clash.typingDeafen");
  return check.clash ? t(`voice.key.clash.${check.clash}`) : null;
}

/** A volume picked by the pointer lands on 100 % when it comes close (the notch). */
export function snapVolume(value: number): number {
  const v = Math.max(0, Math.min(200, Math.round(value)));
  return Math.abs(v - 100) <= 4 ? 100 : v;
}

/**
 * The tone the page plays now: the ring while a call rings for the player, else the ringback while
 * the player's call rings at a friend; none with the window away (Windows' notification rings
 * then, VOICE-L1) or voice chat off.
 */
export function toneNow(
  state: Pick<VoiceState, "available" | "enabled" | "rings" | "calling">,
  away: boolean,
): "ring" | "ringback" | null {
  if (away || !state.available || !state.enabled) return null;
  if (state.rings.length) return "ring";
  if (state.calling) return "ringback";
  return null;
}

// --- VOICE-ROOMS: the crew page's rooms ---------------------------------------------------------

/** A crew's "Genel" room id (the friends service's `g` + crew id). */
export const generalRoomId = (crew: string) => `g${crew}`;

/**
 * A crew's rooms as the crew page lists them: "Genel" first, always (an older service left an
 * empty "Genel" out while another room was live, and the row disappeared), then the rest in the
 * service's order, without any the player is deleting (`hidden`, waiting for its "Geri al").
 */
export function crewRoomList(
  crew: string,
  entry: Pick<VoiceCrewRooms, "rooms"> | undefined,
  hidden: readonly string[] = [],
): VoiceCrewRoom[] {
  const id = generalRoomId(crew);
  const rooms = entry?.rooms ?? [];
  const general = rooms.find((room) => room.id === id) ?? {
    id,
    name: "Genel",
    general: true,
    people: [],
    max: null,
    own: false,
  };
  return [general, ...rooms.filter((room) => room.id !== id && !hidden.includes(room.id))];
}

/** The player's place in a crew, as the crew's room rules read it. */
export type CrewStanding = Pick<SocialTeam, "role" | "moderator">;

const leads = (standing: CrewStanding) => standing.role === "founder";
const moderates = (standing: CrewStanding) => leads(standing) || standing.moderator;

/** What the player may do to one of the crew's rooms from the crew page (voice.md §4). */
export interface RoomPowers {
  /** "Odayı düzenle": name and people at most. */
  edit: boolean;
  /** "Yukarı taşı" / "Aşağı taşı", and dragging the room. */
  order: boolean;
  /** "Odayı sil". */
  remove: boolean;
}

/** Nothing but the room's own action ("Genel", a member's room of someone else, the switch off). */
export const NO_ROOM_POWERS: RoomPowers = { edit: false, order: false, remove: false };

/**
 * The crew's leader and moderators change, order and delete any room but "Genel"; whoever opened
 * a room changes and deletes it; nothing while the service does not let crews arrange rooms.
 */
export function roomPowers(
  standing: CrewStanding,
  room: Pick<VoiceCrewRoom, "general" | "own">,
  manage: boolean,
): RoomPowers {
  if (!manage || room.general) return NO_ROOM_POWERS;
  const may = moderates(standing);
  return { edit: may || room.own, order: may, remove: may || room.own };
}

/**
 * Whether the player may move `member` between the crew's rooms (a drag of their face): the
 * room's "Taşı" rule, leaders anyone, moderators members only; never themselves.
 */
export function mayMovePerson(
  standing: CrewStanding,
  member: Pick<TeamMember, "role" | "moderator" | "me"> | undefined,
  manage: boolean,
): boolean {
  if (!manage || !member || member.me) return false;
  if (leads(standing)) return true;
  return standing.moderator && member.role !== "founder" && !member.moderator;
}

/** The rooms' order (no "Genel") with `id` one step up (-1) or down (1); null at an end. */
export function moveRoom(order: readonly string[], id: string, step: -1 | 1): string[] | null {
  const at = order.indexOf(id);
  const to = at + step;
  if (at < 0 || to < 0 || to >= order.length) return null;
  const next = [...order];
  next.splice(at, 1);
  next.splice(to, 0, id);
  return next;
}

/** A room's people at most as the edit dialog offers it: none (the crew's maximum), or 2 to it. */
export function limitChoices(crewMax: number): Array<number | null> {
  const top = Math.max(2, Math.min(25, crewMax));
  return [null, ...Array.from({ length: top - 1 }, (_, i) => i + 2)];
}

/** Whether a room is full for one more (its own limit within the crew's maximum). */
export function crewRoomFull(
  room: Pick<VoiceCrewRoom, "people" | "max">,
  crewMax: number,
  joining = 1,
): boolean {
  return room.people.length + joining > Math.min(crewMax, room.max ?? crewMax);
}

/** A face in a room's row and the gap after it, and "+N" with its gap (px). */
const FACE_STEP = 28;
const MORE_PX = 30;

/**
 * How many of a room's `count` faces its row shows in `width` px; the rest go into "+N". Before
 * the row is measured (`null`): four.
 */
export function facesThatFit(count: number, width: number | null): number {
  if (width === null) return Math.min(count, 4);
  if (count * FACE_STEP - 4 <= width) return count;
  return Math.max(1, Math.min(count, Math.floor((width - MORE_PX + 4) / FACE_STEP)));
}

/** A room name as typed is one the service takes (3–24 characters; the filter is the service's). */
export function roomNameFits(name: string): boolean {
  const size = [...name.trim()].length;
  return size >= 3 && size <= 24;
}

/** Who came into and who left the room between two states (for the screen reader). */
export function peopleChanges(
  before: Pick<VoiceRoomView, "id" | "people"> | null,
  after: Pick<VoiceRoomView, "id" | "people"> | null,
): { joined: string[]; left: string[] } {
  if (!before || !after || before.id !== after.id) return { joined: [], left: [] };
  const was = new Set(before.people.map((p) => p.id));
  const is = new Set(after.people.map((p) => p.id));
  return {
    joined: [...is].filter((id) => !was.has(id)),
    left: [...was].filter((id) => !is.has(id)),
  };
}
