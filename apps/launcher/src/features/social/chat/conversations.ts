// Conversations (CHAT1), pure rules (conversations.test.ts): their ids, who each is with, what is
// unread, a conversation muted on this computer, the order the list shows, and what a deleted
// message's tombstone says (FIX2).
import type {
  ChatMessage,
  ChatState,
  Conversation,
  SocialProfile,
  SocialSnapshot,
  SocialTeam,
} from "@/lib/api/social-types";

/** A crew's chat. */
export function crewConversation(team: string): string {
  return `c:${team}`;
}

/** Two friends' conversation, `d:<smaller>:<larger>`: a shorter id is the smaller one, ids of the
 * same length compare as text (the service's order). */
export function directConversation(a: string, b: string): string {
  const order = a.length - b.length || (a < b ? -1 : a > b ? 1 : 0);
  return order <= 0 ? `d:${a}:${b}` : `d:${b}:${a}`;
}

/** The crew of a crew's chat. */
export function crewOf(conv: string): string | null {
  return conv.startsWith("c:") ? conv.slice(2) : null;
}

/** The other person of a direct conversation. */
export function peerOf(conv: string, me: string): string | null {
  if (!conv.startsWith("d:")) return null;
  const [a, b] = conv.slice(2).split(":");
  if (a === me) return b ?? null;
  if (b === me) return a ?? null;
  return null;
}

/** Conversations muted on this computer ("Sessize al"): until when (ms), or null until unmuted. */
export type ChatMutes = Readonly<Record<string, number | null>>;

export function chatMutedNow(conv: string, mutes: ChatMutes, nowMs: number): boolean {
  if (!Object.hasOwn(mutes, conv)) return false;
  const until = mutes[conv];
  return until === null || (until ?? 0) > nowMs;
}

/** Unread messages in all conversations. */
export function unreadAll(chat: ChatState | undefined): number {
  return chat?.conversations.reduce((sum, conv) => sum + conv.unread, 0) ?? 0;
}

/**
 * Unread direct messages that wait for the player (the friends entry says so): crew chat only
 * counts in the list, and a conversation muted here says nothing.
 */
export function unreadDirect(chat: ChatState | undefined, mutes: ChatMutes, nowMs: number): number {
  if (!chat) return 0;
  return chat.conversations
    .filter((conv) => conv.peer && !chatMutedNow(conv.id, mutes, nowMs))
    .reduce((sum, conv) => sum + conv.unread, 0);
}

/** Who a conversation is with, as the lists know them now. */
export interface ConversationFace {
  /** The friend's or the crew's name, else the last known one. */
  name: string | null;
  /** The friend (or the last known picture). */
  person?: SocialProfile & { state?: "offline" | "launcher" | "playing"; dnd?: boolean };
  crew?: SocialTeam;
}

export function conversationFace(
  conv: Pick<Conversation, "id" | "peer" | "crew" | "title">,
  view: Pick<SocialSnapshot, "friends" | "crews">,
): ConversationFace {
  if (conv.crew) {
    const crew = view.crews.find((c) => c.id === conv.crew);
    return { name: crew?.name ?? conv.title ?? null, crew };
  }
  const friend = conv.peer ? view.friends.find((f) => f.id === conv.peer) : undefined;
  return { name: friend?.name ?? conv.title ?? null, person: friend };
}

/** Messages in their order: the service's ids, then the player's that are on their way. */
export function sortMessages(messages: ChatMessage[]): ChatMessage[] {
  return [...messages].sort(
    (a, b) => (a.id ?? Number.MAX_SAFE_INTEGER) - (b.id ?? Number.MAX_SAFE_INTEGER) || a.at - b.at,
  );
}

/** Messages a few minutes apart from one sender read as one group (one name, one picture). */
export const GROUP_GAP_MS = 4 * 60_000;

/** Whether `next` continues `previous`'s group. */
export function sameGroup(previous: ChatMessage | undefined, next: ChatMessage): boolean {
  return (
    previous !== undefined &&
    previous.from === next.from &&
    next.at - previous.at < GROUP_GAP_MS &&
    new Date(previous.at).toDateString() === new Date(next.at).toDateString()
  );
}

/** A day line goes before the first message of each day. */
export function newDay(previous: ChatMessage | undefined, next: ChatMessage): boolean {
  return (
    previous === undefined ||
    new Date(previous.at).toDateString() !== new Date(next.at).toDateString()
  );
}

// --- tombstones (FIX2) ---------------------------------------------------------------------------

/**
 * Who a tombstone says deleted the message: the player ("Bu mesajı sildin"), its sender or nobody
 * known ("Bu mesaj silindi", also when that account is gone), a crew's leader or one of its
 * moderators (crew chat), or Projects' moderators.
 */
export type TombstoneKind = "mine" | "sender" | "leader" | "moderator" | "staff";

export function tombstoneKind(
  by: string,
  from: string,
  me: string | undefined,
  crew?: Pick<SocialTeam, "members">,
): TombstoneKind {
  if (by === "staff") return "staff";
  if (me && by === me) return "mine";
  if (!by || by === from || !crew) return "sender";
  // Someone else took it away in crew chat: its leader, else one of its moderators (also one who
  // has left the crew since).
  return crew.members.find((m) => m.id === by)?.role === "founder" ? "leader" : "moderator";
}

/** The message a tombstone stands for, or the player's own deletion that waits for its answer. */
export function deletionOf(
  message: Pick<ChatMessage, "deleted" | "key">,
  deleting: Readonly<Record<string, true>>,
  me: string | undefined,
): { by: string } | undefined {
  return message.deleted ?? (deleting[message.key] ? { by: me ?? "" } : undefined);
}

/** Messages that still say something: tombstones are neither unread nor edited again. */
export function readable(message: Pick<ChatMessage, "deleted">): boolean {
  return !message.deleted;
}
