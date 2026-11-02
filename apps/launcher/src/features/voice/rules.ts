// What voice chat's surfaces decide (VOICE-L3; voice.md §3–§5), apart from React so the tests read
// them: who a face is, what a room is called, what a friend's card offers ("Ara", "Odaya davet et"),
// who "Davet et" lists, the words for a refusal or an ended stay, and a shortcut key's label.
import { matchesQuery } from "@pl/ui";
import type { TFunction } from "i18next";
import type { Friend, SocialSnapshot } from "@/lib/api/social-types";
import type {
  VoiceAnswer,
  VoiceKey,
  VoiceKeyCheck,
  VoiceLeftReason,
  VoicePerson,
  VoicePresence,
  VoiceRole,
  VoiceRoomView,
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

/** A crew room's name as shown: the crew's "Genel" in the player's language, else its own. */
export function crewRoomName(
  room: { name: string | null; general: boolean },
  t: TFunction,
): string {
  return room.general ? t("voice.room.general") : (room.name ?? "");
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
  if (room.name) return room.name;
  if (room.people.length)
    return namesLine(
      room.people.map((p) => nameOf(p.id)),
      t,
    );
  if (calling) return nameOf(calling);
  return t("voice.justYou");
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

/**
 * The words for a refused action, or null when it went through (or there was nothing to say:
 * `nothing`). `name` is who the action was about ("… şu an aranamıyor.").
 */
export function refusalText(answer: VoiceAnswer, t: TFunction, name?: string): string | null {
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
    default:
      return t(`voice.refused.${answer.refused}`);
  }
}

/** The words for a stay that ended on its own (the toast). */
export function leftText(reason: VoiceLeftReason, t: TFunction): string {
  // "Cevap yok" is the pill's own (with "Tekrar ara"), as is "Bağlanamadı" (with "Tekrar dene").
  if (reason === "noAnswer") return t("voice.call.noAnswer");
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

/** A key's label as its cap shows it ("Caps Lock", "V", "F8", "Fare 4"). */
export function keyLabel(key: VoiceKey, t: TFunction): string {
  if (key.kind === "mouse") return t(MOUSE_KEYS[key.button]);
  const named = NAMED_KEYS[key.code];
  if (named) return t(named);
  return key.label || codeLabel(key.code);
}

/** A readable name from a `KeyboardEvent.code` alone ("KeyV" → "V", "Numpad1" → "Num 1"). */
export function codeLabel(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^Numpad\d$/.test(code)) return `Num ${code.slice(6)}`;
  return code.replace(/(Left|Right)$/, "");
}

/**
 * The key a press picked, or null for one that cannot be a shortcut (Escape cancels the pick; the
 * Windows key opens the Start menu). The label is the layout's own letter ("Ş" on a Turkish Q),
 * upper-cased in the interface's language.
 */
export function keyFromEvent(
  event: Pick<KeyboardEvent, "code" | "key">,
  lang: string,
): VoiceKey | null {
  const { code, key } = event;
  if (!code || code === "Escape" || code.startsWith("Meta") || code === "Unidentified") return null;
  const label = key.length === 1 && key !== " " ? key.toLocaleUpperCase(lang) : codeLabel(code);
  return { kind: "keyboard", code, label };
}

/** The mouse button a press picked (`MouseEvent.button`): the wheel or a side button. */
export function keyFromMouse(button: number): VoiceKey | null {
  if (button === 1) return { kind: "mouse", button: "middle" };
  if (button === 3) return { kind: "mouse", button: "back" };
  if (button === 4) return { kind: "mouse", button: "forward" };
  return null;
}

/** The key picker's one line about a key, or null when there is nothing to say. */
export function clashText(check: VoiceKeyCheck, t: TFunction): string | null {
  if (!check.usable) return t("voice.key.clash.unusable");
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
