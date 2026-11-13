// Room chat (SQOL §1.3, §2.2, §3): what the launcher hands the page, as this module reads it. The
// launcher's events and answers are read field by field (`msgOf`, `eventOf`, `sendOf`), so a
// launcher that names a field as the room's wire does (`u`, `st`, `ms`) reads the same.

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
 * Why the room did not take a message (SQOL-R's `RoomChatError`; the room's §2.2 codes read the
 * same: `chat_off`, `chat_rejected`, `rate_limited`, `chat_muted`). `gone`: the message or the room
 * is not there any more.
 */
export type RoomChatCode =
  | "off"
  | "rejected"
  | "rateLimited"
  | "muted"
  | "limited"
  | "gone"
  | "failed";

export interface RoomChatRefusal {
  code: RoomChatCode;
  /** With `rejected`: which text rule refused it. */
  reason?: "phone" | "link" | "word" | "length" | "lines";
  /** With a refused link: the link rule (chat-links.json), for its own words. */
  rule?: string;
}

export type RoomChatSend = { sent: { id: string; at: number } } | { refused: RoomChatRefusal };

/** What the page asks of room chat. Listeners return their stop function. */
export interface RoomChatApi {
  /** The room's last messages as this computer holds them (the people the player blocks out). */
  log(room: string): Promise<RoomChatMsg[]>;
  send(room: string, body: { text?: string; sticker?: string }): Promise<RoomChatSend>;
  remove(room: string, id: string): Promise<void>;
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
  const from = text(raw.from ?? raw.u ?? raw.user);
  const at = typeof raw.at === "number" ? raw.at : Number(raw.at);
  const sticker = text(raw.sticker ?? raw.st) || undefined;
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
      const msg = msgOf(raw.msg ?? raw.m ?? raw.message);
      return msg ? { room, kind: "msg", msg } : null;
    }
    case "del": {
      const id = text(raw.id);
      const by = BY.find((b) => b === raw.by) ?? "self";
      return id ? { room, kind: "del", id, by } : null;
    }
    case "log": {
      const list = raw.msgs ?? raw.ms ?? raw.messages;
      const msgs = Array.isArray(list)
        ? list.map(msgOf).filter((m): m is RoomChatMsg => m !== null)
        : [];
      return { room, kind: "log", msgs };
    }
    default:
      return null;
  }
}

/** A refusal's code as SQOL-R names it, from its name or the room's (§2.2). */
const CODES: Record<string, RoomChatCode> = {
  off: "off",
  chat_off: "off",
  chat_feature_off: "off",
  rejected: "rejected",
  chat_rejected: "rejected",
  rateLimited: "rateLimited",
  rate_limited: "rateLimited",
  muted: "muted",
  chat_muted: "muted",
  limited: "limited",
  gone: "gone",
  not_found: "gone",
  failed: "failed",
};
const REASONS = ["phone", "link", "word", "length", "lines"] as const;

/** A refusal's code, or `failed` for one this page does not know. */
export function codeOf(value: unknown): RoomChatCode {
  return (typeof value === "string" && Object.hasOwn(CODES, value) && CODES[value]) || "failed";
}

/**
 * What `voice_chat_send` answered: the room took it (its id and time), or why not. SQOL-R answers
 * `{kind: "sent", id, at}` or `{kind: "refused", error, reason, rule, …}`; a flat `{code, …}` or a
 * nested `{refused: {code, …}}` reads the same.
 */
export function sendOf(raw: unknown, nowMs = Date.now()): RoomChatSend {
  if (!isRaw(raw)) return { refused: { code: "failed" } };
  const refusal = raw.kind === "sent" ? null : (raw.refused ?? raw.error ?? raw.code);
  if (raw.kind === "refused" || (refusal !== undefined && refusal !== null)) {
    const detail = isRaw(refusal) ? refusal : raw;
    const code = codeOf(isRaw(refusal) ? refusal.code : refusal);
    const reason = REASONS.find((r) => r === detail.reason);
    const rule = text(detail.rule);
    return {
      refused: { code, ...(reason ? { reason } : {}), ...(rule ? { rule } : {}) },
    };
  }
  const id = text(raw.id);
  if (!id) return { refused: { code: "failed" } };
  const at = typeof raw.at === "number" && Number.isFinite(raw.at) ? raw.at : nowMs;
  return { sent: { id, at } };
}
