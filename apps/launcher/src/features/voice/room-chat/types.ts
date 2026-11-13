// Room chat (SQOL §1.3, §2.2, §3): what the launcher hands the page, as this module reads it. The
// launcher's events and answers are SQOL-R's (`RoomChatMsg`, `VoiceChatEvent`, `RoomChatSend`,
// `RoomChatError`, `RoomChatReason` in the generated bindings), read field by field (`msgOf`,
// `eventOf`, `sendOf`) so a malformed one is dropped instead of breaking the chat. The refusals'
// names are the bindings' own: a name R changes fails the typecheck here.
import type { RoomChatError, RoomChatReason } from "@/bindings";

/** One message of the room's chat. */
export interface RoomChatMsg {
  id: string;
  /** The sender's user id. */
  from: string;
  /** Unix ms. */
  at: number;
  /** The words (empty for a sticker). */
  text: string;
  /** A "Küp" sticker ("pl1/gg") in place of words. */
  sticker?: string;
}

/** Who took a message away: its sender, the room's leader or a moderator, Projects' moderators. */
export type RoomChatDeletedBy = "self" | "mod" | "staff";

export type RoomChatEvent =
  | { room: string; kind: "msg"; msg: RoomChatMsg }
  | { room: string; kind: "del"; id: string; by: RoomChatDeletedBy }
  | { room: string; kind: "log"; msgs: RoomChatMsg[] };

/**
 * Why the room did not take a message, a deletion or a report (SQOL-R's `RoomChatError`): `off`
 * (no chat now), `stickersOff` (Studio switched the stickers off), `rejected` (the filters: see
 * `reason`), `rateLimited`, `muted` (Projects' moderators), `limited` (a new account's link),
 * `notAllowed` (deleting someone else's message without the right), `gone` (not in that room any
 * more, or no such message), `failed` (the room did not answer).
 */
export type RoomChatCode = RoomChatError;

export interface RoomChatRefusal {
  code: RoomChatCode;
  /** With `rejected`: what the filters found (`phone`, `link`, `words`), or the launcher's `length`. */
  reason?: RoomChatReason;
  /** With a refused link: the link rule (chat-links.json), for its own words. */
  rule?: string;
}

export type RoomChatSend = { sent: { id: string; at: number } } | { refused: RoomChatRefusal };

/** A deletion or a report the room refused: its callers word `code`. */
export class RoomChatRefused extends Error {
  readonly code: RoomChatCode;

  constructor(code: RoomChatCode) {
    super(`room chat: ${code}`);
    this.code = code;
  }
}

/** What the page asks of room chat. Listeners return their stop function. */
export interface RoomChatApi {
  /** The room's last messages as this computer holds them (the people the player blocks out). */
  log(room: string): Promise<RoomChatMsg[]>;
  send(room: string, body: { text?: string; sticker?: string }): Promise<RoomChatSend>;
  /** Rejects with `RoomChatRefused` when the room refuses. */
  remove(room: string, id: string): Promise<void>;
  /** Rejects with `RoomChatRefused` when the room refuses. */
  report(room: string, id: string, reason: string, note?: string): Promise<void>;
  /** A text field has the focus: the single-key voice shortcuts rest meanwhile (R `voice_typing`). */
  typing(on: boolean): Promise<void>;
  onEvent(listener: (event: RoomChatEvent) => void): () => void;
}

type Raw = Record<string, unknown>;

const isRaw = (value: unknown): value is Raw => typeof value === "object" && value !== null;
const text = (value: unknown): string | undefined =>
  typeof value === "string" ? value : typeof value === "number" ? String(value) : undefined;

/** One message, or null when it is not one (no id, sender or time; neither words nor sticker). */
export function msgOf(raw: unknown): RoomChatMsg | null {
  if (!isRaw(raw)) return null;
  const id = text(raw.id);
  const from = text(raw.from);
  const at = typeof raw.at === "number" ? raw.at : Number(raw.at);
  const sticker = text(raw.sticker) || undefined;
  const words = text(raw.text) ?? "";
  if (!id || !from || !Number.isFinite(at) || (!words && !sticker)) return null;
  return sticker ? { id, from, at, text: "", sticker } : { id, from, at, text: words };
}

const BY: readonly RoomChatDeletedBy[] = ["self", "mod", "staff"];

/** One `voice-chat` event, or null when it is not one this page knows. */
export function eventOf(raw: unknown): RoomChatEvent | null {
  if (!isRaw(raw)) return null;
  const room = text(raw.room);
  if (!room) return null;
  switch (raw.kind) {
    case "msg": {
      const msg = msgOf(raw.msg);
      return msg ? { room, kind: "msg", msg } : null;
    }
    case "del": {
      const id = text(raw.id);
      const by = BY.find((b) => b === raw.by) ?? "self";
      return id ? { room, kind: "del", id, by } : null;
    }
    case "log": {
      const msgs = Array.isArray(raw.msgs)
        ? raw.msgs.map(msgOf).filter((m): m is RoomChatMsg => m !== null)
        : [];
      return { room, kind: "log", msgs };
    }
    default:
      return null;
  }
}

/** Every refusal SQOL-R names (a name it adds or renames fails the typecheck here). */
const CODES: Record<RoomChatCode, true> = {
  off: true,
  stickersOff: true,
  rejected: true,
  rateLimited: true,
  muted: true,
  limited: true,
  notAllowed: true,
  gone: true,
  failed: true,
};
const REASONS: Record<RoomChatReason, true> = {
  phone: true,
  link: true,
  words: true,
  length: true,
};

/** A refusal's code, or `failed` for one this page does not know. */
export function codeOf(value: unknown): RoomChatCode {
  return typeof value === "string" && Object.hasOwn(CODES, value)
    ? (value as RoomChatCode)
    : "failed";
}

/** A rejection's reason, or none for one this page does not know. */
export function reasonOf(value: unknown): RoomChatReason | undefined {
  return typeof value === "string" && Object.hasOwn(REASONS, value)
    ? (value as RoomChatReason)
    : undefined;
}

/**
 * What `voice_chat_send` answered (SQOL-R's `RoomChatSend`): `{kind: "sent", id, at}`, or
 * `{kind: "refused", error, reason, rule, retryAfter, until}`; anything else did not go.
 */
export function sendOf(raw: unknown, nowMs = Date.now()): RoomChatSend {
  if (!isRaw(raw)) return { refused: { code: "failed" } };
  if (raw.kind === "refused") {
    const reason = reasonOf(raw.reason);
    const rule = text(raw.rule);
    return {
      refused: {
        code: codeOf(raw.error),
        ...(reason ? { reason } : {}),
        ...(rule ? { rule } : {}),
      },
    };
  }
  const id = raw.kind === "sent" ? text(raw.id) : undefined;
  if (!id) return { refused: { code: "failed" } };
  const at = typeof raw.at === "number" && Number.isFinite(raw.at) ? raw.at : nowMs;
  return { sent: { id, at } };
}
