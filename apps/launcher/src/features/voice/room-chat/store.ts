// What the room chat keeps on the page (SQOL §1.3): the player's room's messages (never written
// anywhere: they go with the room), the player's own on their way, how many came while the chat was
// out of sight, the previews over the stage, and the field's words. The changes are pure functions
// (store.test.ts); the store only holds their result.
import { create } from "zustand";
import type { ChatPerson } from "@/features/social/chat/sender-name";
import { failureOf, PEEK_MS, type Peek, type ShownMsg, withPeek } from "./rules";
import type { RoomChatDeletedBy, RoomChatMsg, RoomChatSend } from "./types";

/** The page keeps at most this many of a room's messages (the room sends its last 50). */
export const KEPT_MAX = 200;

export interface RoomChatData {
  /** The room these messages are of (null: no room). */
  room: string | null;
  msgs: ShownMsg[];
  /** The room's last messages came (or there is nothing to wait for). */
  loaded: boolean;
  /** The last number a message took (`ShownMsg.seq`). */
  seq: number;
  /** Others' messages that came while the chat was out of sight. */
  unread: number;
  /** "Yeni Mesajlar" goes over this message while the chat shows (the first unread one). */
  divider: string | null;
  /** The chat shows now (the room panel open on it). */
  shown: boolean;
  /** The previews over the stage (RoomChatPeek), the newest last. */
  peeks: Peek[];
  /** A third person came in this stay: the panel keeps the room's chat (SQOL §1.3). */
  latched: boolean;
  /** Senders as they were when they wrote (someone who left keeps their name and face). */
  people: Record<string, ChatPerson>;
  /** The field's words. */
  draft: string;
}

export const EMPTY_ROOM_CHAT: RoomChatData = {
  room: null,
  msgs: [],
  loaded: false,
  seq: 0,
  unread: 0,
  divider: null,
  shown: false,
  peeks: [],
  latched: false,
  people: {},
  draft: "",
};

/** The data for `room`: the same while it stays, a fresh start for another (or none). */
export function forRoom(data: RoomChatData, room: string | null): RoomChatData {
  return data.room === room ? data : { ...EMPTY_ROOM_CHAT, room, shown: data.shown };
}

const sameBody = (m: ShownMsg, body: { text: string; sticker?: string }) =>
  m.text === body.text && (m.sticker ?? null) === (body.sticker ?? null);

function kept(msgs: ShownMsg[]): ShownMsg[] {
  return msgs.length > KEPT_MAX ? msgs.slice(msgs.length - KEPT_MAX) : msgs;
}

export interface Arrival {
  me: string | null | undefined;
  /** Others' messages show as previews now (the stage is up, the setting on, the window in front). */
  peek: boolean;
  nowMs: number;
  /** The sender as the lists know them now. */
  person?: ChatPerson;
}

/**
 * A message the room sent: the player's own settles the one on its way (matched by its words when
 * its answer has not come yet), anyone else's is added once; out of sight it counts, and with
 * `peek` it shows over the stage.
 */
export function receive(data: RoomChatData, msg: RoomChatMsg, arrival: Arrival): RoomChatData {
  const known = data.msgs.find((m) => m.id === msg.id);
  if (known) {
    // The room's copy of the player's own message carries its words as the room keeps them.
    if (!known.mine || sameBody(known, msg)) return data;
    return {
      ...data,
      msgs: data.msgs.map((m) =>
        m === known ? { ...m, text: msg.text, sticker: msg.sticker } : m,
      ),
    };
  }
  const mine = Boolean(arrival.me) && msg.from === arrival.me;
  const people = arrival.person ? { ...data.people, [msg.from]: arrival.person } : data.people;
  if (mine) {
    const waiting = data.msgs.find((m) => m.mine && m.id === undefined && sameBody(m, msg));
    if (waiting) {
      return {
        ...data,
        people,
        msgs: data.msgs.map((m) =>
          m === waiting ? { ...m, id: msg.id, at: msg.at, state: "sent", failure: undefined } : m,
        ),
      };
    }
  }
  const seq = data.seq + 1;
  const shown: ShownMsg = {
    key: `r:${msg.id}`,
    id: msg.id,
    seq,
    from: msg.from,
    mine,
    at: msg.at,
    text: msg.text,
    ...(msg.sticker ? { sticker: msg.sticker } : {}),
    state: "sent",
  };
  const counts = !mine && !data.shown;
  return {
    ...data,
    people,
    seq,
    msgs: kept([...data.msgs, shown]),
    unread: counts ? data.unread + 1 : data.unread,
    peeks:
      counts && arrival.peek
        ? withPeek(data.peeks, {
            key: shown.key,
            from: msg.from,
            text: msg.text,
            ...(msg.sticker ? { sticker: msg.sticker } : {}),
            until: arrival.nowMs + PEEK_MS,
          })
        : data.peeks,
  };
}

/**
 * The room's whole log anew (joining, coming back, someone blocked; SQOL-R's `log`): it replaces
 * what the page shows, an empty one too. What the page had of it stays as it was (its number, a
 * tombstone); what the room no longer holds goes, with its preview; the player's own on their way
 * or refused stay at the end. History counts nothing unread.
 */
export function receiveLog(
  data: RoomChatData,
  msgs: readonly RoomChatMsg[],
  me: string | null | undefined,
): RoomChatData {
  const sorted = [...msgs].sort((a, b) => a.at - b.at);
  const known = new Map(data.msgs.filter((m) => m.id !== undefined).map((m) => [m.id, m]));
  let seq = data.seq;
  const fromLog: ShownMsg[] = sorted.map((msg) => {
    const had = known.get(msg.id);
    if (had) return had;
    seq += 1;
    return {
      key: `r:${msg.id}`,
      id: msg.id,
      seq,
      from: msg.from,
      mine: Boolean(me) && msg.from === me,
      at: msg.at,
      text: msg.text,
      ...(msg.sticker ? { sticker: msg.sticker } : {}),
      state: "sent",
    };
  });
  const waiting = data.msgs.filter((m) => m.id === undefined);
  const next = kept([...fromLog, ...waiting]);
  const keys = new Set(next.map((m) => m.key));
  return {
    ...data,
    seq,
    loaded: true,
    msgs: next,
    // What went cannot wait unread, carry the line or show as a preview.
    unread: Math.min(data.unread, next.filter((m) => !m.mine && !m.deleted).length),
    divider: data.divider !== null && keys.has(data.divider) ? data.divider : null,
    peeks: data.peeks.filter((p) => keys.has(p.key)),
  };
}

/** A message taken away: its tombstone (the player's own deletion says so). */
export function markDeleted(
  data: RoomChatData,
  id: string,
  by: RoomChatDeletedBy | "me",
): RoomChatData {
  let changed = false;
  const msgs = data.msgs.map((m) => {
    if (m.id !== id) return m;
    changed = true;
    // The player's own press already said "Bu mesajı sildin": the room's word does not change it.
    const deleted = m.deleted?.by === "me" ? m.deleted : { by };
    return { ...m, deleted };
  });
  return changed ? { ...data, msgs, peeks: data.peeks.filter((p) => p.key !== `r:${id}`) } : data;
}

/** The player's deletion did not go: the message comes back. */
export function undelete(data: RoomChatData, id: string): RoomChatData {
  return {
    ...data,
    msgs: data.msgs.map((m) =>
      m.id === id && m.deleted?.by === "me" ? { ...m, deleted: undefined } : m,
    ),
  };
}

/** The player's message on its way (`nonce`: its key until the room answers). */
export function startSend(
  data: RoomChatData,
  nonce: string,
  body: { text: string; sticker?: string },
  me: string,
  nowMs: number,
): RoomChatData {
  const seq = data.seq + 1;
  const msg: ShownMsg = {
    key: `n:${nonce}`,
    seq,
    from: me,
    mine: true,
    at: nowMs,
    text: body.text,
    ...(body.sticker ? { sticker: body.sticker } : {}),
    state: "sending",
  };
  return { ...data, seq, msgs: kept([...data.msgs, msg]) };
}

/** The room's answer to a message on its way. */
export function settleSend(data: RoomChatData, nonce: string, result: RoomChatSend): RoomChatData {
  const key = `n:${nonce}`;
  const own = data.msgs.find((m) => m.key === key);
  if (!own) return data;
  if ("sent" in result) {
    // The room's own copy may have come first and settled it already.
    const copy = data.msgs.find((m) => m.id === result.sent.id && m.key !== key);
    const msgs = data.msgs
      .filter((m) => m !== copy)
      .map((m) =>
        m.key === key
          ? { ...m, id: result.sent.id, at: own.id ? m.at : result.sent.at, state: "sent" as const }
          : m,
      );
    return { ...data, msgs };
  }
  // The room's copy came first: the room took it, whatever the answer says (a late timeout).
  if (own.id !== undefined) return data;
  const { failure, rule } = failureOf(result.refused);
  return {
    ...data,
    msgs: data.msgs.map((m) =>
      m.key === key
        ? { ...m, state: "failed" as const, failure, ...(rule ? { failureRule: rule } : {}) }
        : m,
    ),
  };
}

/** A refused message tried again: on its way once more. */
export function retrySend(data: RoomChatData, key: string): RoomChatData {
  return {
    ...data,
    msgs: data.msgs.map((m) =>
      m.key === key
        ? { ...m, state: "sending" as const, failure: undefined, failureRule: undefined }
        : m,
    ),
  };
}

/** A refused message the player let go ("Sil"), or took back to the field ("Düzenle"). */
export function discard(data: RoomChatData, key: string): RoomChatData {
  return { ...data, msgs: data.msgs.filter((m) => m.key !== key) };
}

/**
 * The chat came on screen or went: on screen what waited is read ("Yeni Mesajlar" over the first
 * of it for this look) and the previews go; out of sight the line goes with it.
 */
export function setShown(data: RoomChatData, shown: boolean): RoomChatData {
  if (data.shown === shown) return data;
  if (!shown) return { ...data, shown, divider: null };
  let divider: string | null = null;
  if (data.unread > 0) {
    const theirs = data.msgs.filter((m) => !m.mine && !m.deleted);
    divider = theirs[Math.max(0, theirs.length - data.unread)]?.key ?? null;
  }
  return { ...data, shown, unread: 0, divider, peeks: [] };
}

interface RoomChatStore extends RoomChatData {
  /** Applies a change (one of the functions above). */
  apply: (change: (data: RoomChatData) => RoomChatData) => void;
  setDraft: (draft: string) => void;
}

export const useRoomChat = create<RoomChatStore>()((set) => ({
  ...EMPTY_ROOM_CHAT,
  apply: (change) =>
    set((s) => {
      const next = change(s);
      return next === s ? s : next;
    }),
  setDraft: (draft) => set({ draft }),
}));

/** The data alone (tests, and changes made outside React). */
export function roomChatNow(): RoomChatData {
  return useRoomChat.getState();
}
