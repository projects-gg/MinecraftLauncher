// SOC-NEXT §3.2, pure rules (chat-search.test.ts): search in one conversation over the history
// this computer holds (5,000 messages at most). Folded as every search in the app (`fold`: case,
// "İ"/"ı"/"I" and Turkish marks do not matter), two characters at least; tombstones and stickers
// are not searched. Results are messages, the newest first ("1/12" is the newest); every place
// in a message that matches is marked.
import { fold } from "@pl/ui/search";
import type { ChatMessage } from "@/lib/api/social-types";

/** A search starts from this many characters. */
export const SEARCH_MIN = 2;
/** Typing waits this long before the search runs. */
export const SEARCH_WAIT_MS = 150;

/** A text folded, with where each folded character came from in the original (UTF-16). */
export interface FoldedText {
  folded: string;
  /** `from[i]`: where the original character of `folded[i]` starts; `from[folded.length]` the
   * text's length. */
  from: number[];
  /** `to[i]`: where that original character ends. */
  to: number[];
}

export function foldWithMap(text: string): FoldedText {
  let folded = "";
  const from: number[] = [];
  const to: number[] = [];
  let index = 0;
  for (const ch of text) {
    const piece = fold(ch);
    for (let k = 0; k < piece.length; k++) {
      from.push(index);
      to.push(index + ch.length);
    }
    folded += piece;
    index += ch.length;
  }
  from.push(text.length);
  return { folded, from, to };
}

/** Where `query` (already folded) occurs in `text`, as ranges of the original text. */
export function matchRanges(text: string, query: string): Array<[number, number]> {
  if (!query) return [];
  const map = foldWithMap(text);
  const out: Array<[number, number]> = [];
  let at = map.folded.indexOf(query);
  while (at >= 0) {
    const start = map.from[at] ?? 0;
    const end = map.to[at + query.length - 1] ?? text.length;
    // Two matches inside one original character read as one.
    const last = out.at(-1);
    if (last && start < last[1]) last[1] = Math.max(last[1], end);
    else out.push([start, end]);
    at = map.folded.indexOf(query, at + Math.max(1, query.length));
  }
  return out;
}

/** The query as it is searched, or null while it is too short. */
export function searchQuery(text: string): string | null {
  const folded = fold(text.trim());
  return [...folded].length >= SEARCH_MIN ? folded : null;
}

/** What a message offers the search: its words (none for a tombstone or a sticker). */
export function searchable(message: Pick<ChatMessage, "text" | "deleted" | "sticker">): boolean {
  return !message.deleted && !message.sticker && message.text.length > 0;
}

/** The messages that match, the newest first (their keys). `text` reads a message's shown words
 * (an edit on its way shows its new text). */
export function searchMessages(
  messages: readonly ChatMessage[],
  query: string,
  text: (message: ChatMessage) => string = (message) => message.text,
): string[] {
  const out: string[] = [];
  for (let index = messages.length - 1; index >= 0; index--) {
    const message = messages[index];
    if (!message || !searchable(message)) continue;
    if (fold(text(message)).includes(query)) out.push(message.key);
  }
  return out;
}

/** The history and the messages shown, each once (the shown one wins: it came later), oldest
 * first. */
export function mergeHistory(
  history: readonly ChatMessage[],
  shown: readonly ChatMessage[],
): ChatMessage[] {
  const keys = new Set(shown.map((message) => message.key));
  const merged = [...history.filter((message) => !keys.has(message.key)), ...shown];
  return merged.sort(
    (a, b) => (a.id ?? Number.MAX_SAFE_INTEGER) - (b.id ?? Number.MAX_SAFE_INTEGER) || a.at - b.at,
  );
}

/** The next result to show: older (`1`, ↑ and Enter) or newer (`-1`, ↓ and Shift+Enter), going
 * round at the ends. */
export function stepResult(index: number, total: number, direction: 1 | -1): number {
  if (total <= 0) return -1;
  if (index < 0) return 0;
  return (index + direction + total) % total;
}
