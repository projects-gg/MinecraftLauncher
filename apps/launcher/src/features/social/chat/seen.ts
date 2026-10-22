// "Görüldü" (CHAT2 §5, direct messages only), pure rules (seen.test.ts): under the player's last
// message the friend has seen, while the friend has not written since (an answer says it was seen
// by itself; Instagram and iMessage do the same). No time, no ticks on every bubble. And when this
// computer counts a friend's message as read: on screen for a second, the window in front.
import type { ChatMessage } from "@/lib/api/social-types";

/** A friend's newest message counts as read after this long on screen. */
export const READ_AFTER_MS = 1_000;

/**
 * The key of the message that shows "Görüldü", or null: the player's last sent message whose id
 * the friend's mark (`peerRead`, the newest id they have seen) reaches, with no message of the
 * friend's after it.
 */
export function seenKey(
  messages: readonly ChatMessage[],
  peerRead: number | undefined,
): string | null {
  if (peerRead === undefined) return null;
  let found: string | null = null;
  for (const message of messages) {
    // A message of the friend's (also one deleted since) answers what came before it.
    if (!message.mine) {
      found = null;
      continue;
    }
    if (message.state !== "sent" || message.id === undefined || message.deleted) continue;
    if (message.id <= peerRead) found = message.key;
  }
  return found;
}

/**
 * The newest message of someone else's that the player has not marked read yet, or null (the
 * conversation's own unread count says whether there is any): the one to watch for a second.
 */
export function newestTheirs(messages: readonly ChatMessage[]): ChatMessage | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    const message = messages[i] as ChatMessage;
    if (message.mine) return null;
    if (!message.deleted && message.id !== undefined) return message;
  }
  return null;
}
