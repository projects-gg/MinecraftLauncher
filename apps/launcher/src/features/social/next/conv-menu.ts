// SOC-NEXT §3.9, pure rules (conv-menu.test.ts): a conversation's own rows in "Mesajlar" (a right
// click, Shift+F10, the menu key), what counts as unread with the player's own "Okunmadı say", and
// the order with up to three conversations pinned to the top. A conversation marked unread here
// counts one message until the player reads it, marks it read, or a newer message comes (then the
// service's count says what is unread, or the player wrote there themself).
import type { ChatState, Conversation } from "@/lib/api/social-types";
import type { Plan } from "../menus";
import { MAX_PINNED } from "./prefs";

export type ConvAction = "markRead" | "markUnread" | "pin" | "unpin" | "mute" | "unmute" | "clear";

export interface ConvInput {
  /** It counts as unread now (`shownUnread`). */
  unread: boolean;
  /** It has messages ("Geçmişi temizle"). */
  hasMessages: boolean;
  /** Its newest message is someone else's: only that can be marked unread (the player's own
   * last word is never "new"). */
  theirsNewest: boolean;
  pinned: boolean;
  /** Three others are pinned: "Başa sabitle" says the rule when pressed (a disabled row could not
   * say why). */
  pinsFull: boolean;
  /** "Sessize al" on this computer. */
  muted: boolean;
}

/** The conversation's rows, before the person's or the crew's rows (a separator between). */
export function conversationMenu(input: ConvInput): Plan<ConvAction> {
  const rows: Plan<ConvAction> = [];
  if (input.unread) rows.push({ action: "markRead" });
  else if (input.theirsNewest) rows.push({ action: "markUnread" });
  rows.push(input.pinned ? { action: "unpin" } : { action: "pin" }, null, {
    action: input.muted ? "unmute" : "mute",
  });
  if (input.hasMessages) rows.push({ action: "clear" });
  return rows;
}

/** Conversations marked unread here: id → the newest message's time when marked (Unix ms). */
export type UnreadMarks = Readonly<Record<string, number>>;

/** How many of a conversation's messages count as unread: the service's count, else one while
 * the player's own mark holds (its message is still the newest). */
export function shownUnread(
  conv: Pick<Conversation, "id" | "unread" | "last">,
  marks: UnreadMarks,
): number {
  if (conv.unread > 0) return conv.unread;
  const mark = marks[conv.id];
  return mark !== undefined && conv.last !== undefined && conv.last.at <= mark ? 1 : 0;
}

/** Unread messages in all conversations, marks included ("Mesajlar"'s count). */
export function unreadShownAll(chat: ChatState | undefined, marks: UnreadMarks): number {
  return chat?.conversations.reduce((sum, conv) => sum + shownUnread(conv, marks), 0) ?? 0;
}

/** Pins of conversations still listed, in their order. */
export function livePins(pinned: readonly string[], conversations: readonly { id: string }[]) {
  return pinned.filter((id) => conversations.some((conv) => conv.id === id)).slice(0, MAX_PINNED);
}

/** The list's order: the pinned ones first in the order they were pinned, then the rest as the
 * service sorted them (the newest first). Nothing pinned: the same list. */
export function withPinsFirst<T extends { id: string }>(
  conversations: readonly T[],
  pinned: readonly string[],
): readonly T[] {
  const pins = livePins(pinned, conversations);
  if (!pins.length) return conversations;
  const top = pins.flatMap((id) => conversations.filter((conv) => conv.id === id));
  return [...top, ...conversations.filter((conv) => !pins.includes(conv.id))];
}

/** Marks that still say something: of a listed conversation whose message is still the newest. */
export function liveMarks(
  marks: UnreadMarks,
  conversations: readonly Pick<Conversation, "id" | "unread" | "last">[],
): UnreadMarks {
  const out: Record<string, number> = {};
  for (const conv of conversations) {
    const mark = marks[conv.id];
    if (mark !== undefined && conv.unread === 0 && conv.last && conv.last.at <= mark) {
      out[conv.id] = mark;
    }
  }
  return out;
}
