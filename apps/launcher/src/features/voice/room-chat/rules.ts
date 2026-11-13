// Room chat's pure rules (rules.test.ts): which chat the room panel shows (SQOL §1.3 "Hangi
// sohbet"), how a room's refusal reads in chat's own words, what a row of the room chat is as a
// chat message (message-row.tsx draws it), what its tombstone says, and the previews' timing.
import { directConversation } from "@/features/social/chat/conversations";
import type { ChatMessage, SendFailure } from "@/lib/api/social-types";
import type { VoiceRoomView } from "@/lib/api/voice-types";
import type { RoomChatCode, RoomChatDeletedBy, RoomChatRefusal } from "./types";

/** The chat under the room's people: a friend's direct messages, the room's own chat, or none. */
export type PanelChat =
  | { kind: "dm"; conv: string; friend: string }
  | { kind: "room" }
  | { kind: "none" };

export interface ChatChoiceInput {
  /** `chat`: the room has its chat; `grew`: more than two were ever in it (SQOL §3). */
  room: Pick<VoiceRoomView, "kind" | "people" | "chat" | "grew">;
  /** The player's call that still rings (the friend is not in the room yet). */
  calling: { friend: string } | null;
  me: string | null | undefined;
  /** The player's friends (mutual, as the friends list holds them). */
  friends: readonly string[];
  /** Direct messages are on (`features.chat`). */
  chatOn: boolean;
  /** The room chat showed in this stay already (a third person came): it stays. */
  latched: boolean;
}

/**
 * SQOL §1.3: a friend room that never held more than two, with a friend (mutual) and chat on, shows
 * the conversation with that friend; otherwise the room's chat while the room has one; else none.
 * A third person turns it to the room's chat for good (`grew`, and this computer's latch).
 */
export function chatChoice(input: ChatChoiceInput): PanelChat {
  const { room, calling, me, chatOn, latched } = input;
  const grew = room.grew === true || room.people.length > 1;
  if (room.kind === "friend" && !grew && !latched && chatOn && me) {
    const other =
      room.people.length === 1
        ? room.people[0]?.id
        : room.people.length === 0
          ? calling?.friend
          : null;
    if (other && input.friends.includes(other)) {
      return { kind: "dm", conv: directConversation(me, other), friend: other };
    }
  }
  return room.chat ? { kind: "room" } : { kind: "none" };
}

/**
 * A room's refusal of a message (SQOL-R's `RoomChatError` and `RoomChatReason`) as chat's own
 * failure words (`chat.failure.*`, `chat2.failure.*`): the filters' phone, link (its rule's own
 * words) and words as chat says them; Studio's stickers switch as chat's "Bu özellik şu an
 * kapalı."; a mute, a limit, haste and a closed chat as chat's; only a room that is gone or did
 * not answer says "Gönderilemedi.".
 */
export function failureOf(refusal: RoomChatRefusal): { failure: SendFailure; rule?: string } {
  switch (refusal.code) {
    case "rejected":
      switch (refusal.reason) {
        case "phone":
          return { failure: "phone" };
        case "link":
          return refusal.rule ? { failure: "link", rule: refusal.rule } : { failure: "link" };
        case "words":
          return { failure: "words" };
        default:
          // Too long (the launcher's own `length`; the composer stops it first) or no reason.
          return { failure: "invalid" };
      }
    case "stickersOff":
      return { failure: "featureOff" };
    case "rateLimited":
      return { failure: "tooFast" };
    case "muted":
      return { failure: "muted" };
    case "limited":
      return { failure: "limitedLinks" };
    case "notAllowed":
      return { failure: "notAllowed" };
    case "off":
      return { failure: "off" };
    case "gone":
    case "failed":
      return { failure: "connection" };
  }
}

/**
 * A deletion the room refused, as the words its toast says: `notAllowed` (someone else's message
 * without the right: "Bu mesajı silemezsin."), `off` (chat's "Sohbet şu an kapalı."), else chat's
 * "Gönderilemedi."; null when the message is not there any more (`gone`): it stays taken away.
 */
export function deleteFailureOf(code: RoomChatCode): "notAllowed" | "off" | "connection" | null {
  switch (code) {
    case "gone":
      return null;
    case "notAllowed":
      return "notAllowed";
    case "off":
      return "off";
    default:
      return "connection";
  }
}

/** A message as the room chat shows it. */
export interface ShownMsg {
  /** Stays the same from writing to sent: `n:<nonce>` for the player's own, else `r:<id>`. */
  key: string;
  /** The room's id, once the room has it. */
  id?: string;
  /** A number of its own on this page (a chat row's `id`: its menu and report read one). */
  seq: number;
  from: string;
  mine: boolean;
  /** Unix ms. */
  at: number;
  text: string;
  sticker?: string;
  state: "sent" | "sending" | "failed";
  failure?: SendFailure;
  failureRule?: string;
  /** Taken away: by whom (`me`: the player, from here). */
  deleted?: { by: RoomChatDeletedBy | "me" };
}

/**
 * The row's chat message (message-row.tsx): no edits, reactions or replies; it may be taken away
 * while the room lasts (its sender, the room's leader and moderators).
 */
export function asChatMessage(message: ShownMsg, mayDelete: boolean): ChatMessage {
  return {
    key: message.key,
    id: message.id !== undefined ? message.seq : undefined,
    from: message.from,
    mine: message.mine,
    text: message.text,
    at: message.at,
    edited: false,
    state: message.state,
    ...(message.failure ? { failure: message.failure } : {}),
    ...(message.failureRule ? { failureRule: message.failureRule } : {}),
    ...(message.sticker ? { sticker: message.sticker } : {}),
    ...(mayDelete && message.id !== undefined && !message.deleted
      ? { deleteUntil: Number.MAX_SAFE_INTEGER }
      : {}),
    ...(message.deleted ? { deleted: { by: message.deleted.by } } : {}),
  };
}

/** Which tombstone words a message taken away takes. */
export type RoomTombstone = "mine" | "sender" | "mod" | "staff";

export function tombstoneOf(
  deleted: NonNullable<ShownMsg["deleted"]>,
  mine: boolean,
): RoomTombstone {
  if (deleted.by === "me") return "mine";
  if (deleted.by === "staff") return "staff";
  if (deleted.by === "mod") return "mod";
  return mine ? "mine" : "sender";
}

// --- previews (RoomChatPeek) -------------------------------------------------------------------

/** At most this many previews at once; a fourth pushes the oldest out. */
export const PEEK_MAX = 3;
/** A preview fades this long after it came (a pointer resting on them holds them). */
export const PEEK_MS = 6_000;

export interface Peek {
  key: string;
  from: string;
  text: string;
  sticker?: string;
  /** Unix ms: it goes then. */
  until: number;
}

/** The previews with one more (the newest last). */
export function withPeek(list: readonly Peek[], peek: Peek, max = PEEK_MAX): Peek[] {
  return [...list.filter((p) => p.key !== peek.key), peek].slice(-max);
}

/** The previews still showing at `nowMs`. */
export function livePeeks(list: readonly Peek[], nowMs: number): Peek[] {
  return list.filter((p) => p.until > nowMs);
}

/** When the next preview goes, or null when none waits. */
export function nextPeekEnd(list: readonly Peek[], nowMs: number): number | null {
  let next: number | null = null;
  for (const p of list) if (p.until > nowMs && (next === null || p.until < next)) next = p.until;
  return next;
}

/** The previews held while the pointer rested on them: their time moves on by the rest. */
export function heldPeeks(list: readonly Peek[], restedMs: number): Peek[] {
  return restedMs > 0 ? list.map((p) => ({ ...p, until: p.until + restedMs })) : [...list];
}

/** A line for a screen reader or a preview: words cut to one line's worth. */
export function shortLine(text: string, max = 140): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max - 1).trimEnd()}…` : flat;
}
