// Reactions (CHAT2 §7; owner, 22 Oct 2026: up to five per person on one message), pure rules
// (reactions.test.ts). A message carries at most ten kinds; adding one it has already ("Ben de")
// always works. Messages take reactions while they are in the service's mailbox: sent, not
// deleted, younger than a week.
import type { ChatMessage, ChatReaction } from "@/lib/api/social-types";

/** Different emoji one player may put on one message. */
export const REACTIONS_PER_PERSON = 5;
/** Different emoji one message may carry. */
export const REACTION_KINDS = 10;
/** Reactions only on messages younger than this (the mailbox's week). */
export const REACTION_WINDOW_MS = 7 * 24 * 60 * 60_000;
/** The quick row's six when the player has no recent emoji (§7). */
export const QUICK_DEFAULTS = ["👍", "❤️", "😂", "😭", "🔥", "😮"] as const;

const VS16 = /\u{FE0F}/gu;

/** ❤ and ❤️ are the same reaction (the service adds the missing VS16); a skin tone is another. */
export function sameEmoji(a: string, b: string): boolean {
  return a.replace(VS16, "") === b.replace(VS16, "");
}

/** The quick row: the player's recent emoji first, then the defaults; six, each once. */
export function quickReactions(recent: readonly string[], count = 6): string[] {
  const out: string[] = [];
  for (const emoji of [...recent, ...QUICK_DEFAULTS]) {
    if (out.some((e) => sameEmoji(e, emoji))) continue;
    // The middle finger is never a reaction (the service refuses it).
    if (emoji.startsWith("\u{1F595}")) continue;
    out.push(emoji);
    if (out.length === count) break;
  }
  return out;
}

/** Whether the message takes reactions now. */
export function reactable(
  message: Pick<ChatMessage, "state" | "deleted" | "id" | "at">,
  nowMs: number,
): boolean {
  return (
    message.state === "sent" &&
    !message.deleted &&
    message.id !== undefined &&
    nowMs - message.at < REACTION_WINDOW_MS
  );
}

export type ReactVerdict = "add" | "remove" | "mine" | "kinds" | "refused";

/**
 * What a press on `emoji` does: takes the player's back when they gave it, adds it when there is
 * room, else says why not (`mine`: five of theirs already; `kinds`: ten kinds and this one new;
 * `refused`: the middle finger, in any tone, is never a reaction).
 */
export function reactVerdict(
  reactions: readonly ChatReaction[] | undefined,
  emoji: string,
): ReactVerdict {
  if (emoji.startsWith("\u{1F595}")) return "refused";
  const list = reactions ?? [];
  const kind = list.find((r) => sameEmoji(r.e, emoji));
  if (kind?.mine) return "remove";
  if (list.filter((r) => r.mine).length >= REACTIONS_PER_PERSON) return "mine";
  if (!kind && list.length >= REACTION_KINDS) return "kinds";
  return "add";
}

/**
 * The reactions with the player's change, shown at once until the service answers: their id
 * added at the end of the kind's people (a new kind at the end), or taken out (a kind nobody is
 * left in goes).
 */
export function withMyReaction(
  reactions: readonly ChatReaction[] | undefined,
  emoji: string,
  me: string,
  on: boolean,
): ChatReaction[] {
  const list = (reactions ?? []).map((r) => ({ ...r, by: [...r.by] }));
  const at = list.findIndex((r) => sameEmoji(r.e, emoji));
  if (on) {
    if (at < 0) list.push({ e: emoji, by: [me], mine: true });
    else {
      const kind = list[at] as ChatReaction;
      if (!kind.by.includes(me)) kind.by.push(me);
      kind.mine = true;
    }
    return list;
  }
  if (at < 0) return list;
  const kind = list[at] as ChatReaction;
  kind.by = kind.by.filter((id) => id !== me);
  kind.mine = false;
  return kind.by.length ? list : list.filter((_, i) => i !== at);
}

/** People in a reaction's list: the player first ("Sen"), then in the order they reacted. */
export function reactors(reaction: ChatReaction, me: string | undefined): string[] {
  const mine = me !== undefined && reaction.by.includes(me);
  return mine ? [me, ...reaction.by.filter((id) => id !== me)] : [...reaction.by];
}
