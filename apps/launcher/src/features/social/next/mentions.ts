// SOC-NEXT §3.1, pure rules (mentions.test.ts): what the composer suggests while the player
// writes, and who a crew chat message mentions. "@" offers the crew's members (the letters written
// so far narrow them, those online first, six at most) and puts "@Name " into the text; ":" and
// two letters offer emoji (never after a digit: "10:30" stays as written). A message goes as plain
// text: whoever reads "@Kaan_07" is mentioned when it names them, matched without case or Turkish
// marks and at a word's edge, the longest member name first (names may have spaces). A message
// that mentions more than five members tells nobody (the highlight stays).
import { fold } from "@pl/ui/search";

/** What the player is writing at the caret: a member's name after "@", or an emoji's after ":". */
export interface SuggestToken {
  kind: "member" | "emoji";
  /** What was written after the sign (as written). */
  query: string;
  /** Where the sign is (the suggestion replaces from here to the caret). */
  start: number;
  end: number;
}

/** Emoji suggestions need this many letters after ":". */
export const EMOJI_FROM = 2;
/** At most this many rows. */
export const SUGGEST_ROWS = 6;
/** A message that mentions more members than this tells none of them. */
export const MENTION_NOTIFY_MAX = 5;
/** A name written after "@" is not looked for beyond this many characters. */
const MEMBER_QUERY_MAX = 32;

/** A letter, digit or "_" (a name goes on): anything else is a word's edge. */
const WORDY = /[\p{L}\p{N}_]/u;

const before = (text: string, at: number) => (at > 0 ? text.slice(0, at).slice(-1) : "");

/**
 * The token at the caret, or null. "@" opens at the text's start or after a space (not in an
 * address: "a@b"), its name may have spaces while a member's name goes on that way; ":" opens at
 * the start or after a space or an opening bracket, never after a digit, with two letters at least.
 */
export function suggestToken(
  text: string,
  caret: number,
  memberNames: readonly string[] = [],
): SuggestToken | null {
  const head = text.slice(0, caret);
  // An emoji's name: letters, digits, "_", "+" or "-" since the last ":".
  const emoji = /(^|[\s([{])(:)([\p{L}\p{N}_+-]{2,32})$/u.exec(head);
  if (emoji) {
    const start = caret - (emoji[3]?.length ?? 0) - 1;
    const query = emoji[3] ?? "";
    // "10:30", "12:00": a digit before the sign is a time; letters must lead.
    if (!/\d/u.test(before(text, start)) && /^\p{L}/u.test(query) && query.length >= EMOJI_FROM) {
      return { kind: "emoji", query, start, end: caret };
    }
  }
  const at = head.lastIndexOf("@");
  if (at < 0) return null;
  const sign = before(text, at);
  if (sign && !/[\s([{]/u.test(sign)) return null;
  const query = head.slice(at + 1);
  if (query.length > MEMBER_QUERY_MAX || /[\n\r@]/u.test(query)) return null;
  // A space goes on only while some member's name goes on the same way.
  if (/\s/u.test(query)) {
    const folded = fold(query);
    if (!memberNames.some((name) => fold(name).startsWith(folded))) return null;
  }
  return { kind: "member", query, start: at, end: caret };
}

/** Someone the "@" box may offer. */
export interface MemberChoice {
  id: string;
  /** As the player knows them (a friend's nickname too). */
  name: string;
  /** The name the crew knows (what goes into the text). */
  realName?: string;
  online: boolean;
}

const collator = new Intl.Collator("tr", { sensitivity: "base" });

/** Whether a name answers what was written: its start, or the start of one of its words. */
function answers(name: string, query: string): boolean {
  const folded = fold(name);
  return (
    folded.startsWith(query) || folded.split(/[\s_.-]+/u).some((word) => word.startsWith(query))
  );
}

/** The members to offer, those online first, then by name; six at most, the player left out. */
export function memberChoices<T extends MemberChoice>(
  members: readonly T[],
  query: string,
  me: string | undefined,
  limit = SUGGEST_ROWS,
): T[] {
  const folded = fold(query.trim());
  return members
    .filter((member) => member.id !== me)
    .filter(
      (member) =>
        !folded ||
        answers(member.name, folded) ||
        (member.realName !== undefined && answers(member.realName, folded)),
    )
    .sort(
      (a, b) =>
        Number(b.online) - Number(a.online) ||
        collator.compare(a.realName ?? a.name, b.realName ?? b.name),
    )
    .slice(0, limit);
}

/** The text with a suggestion in the token's place, a space after it, the caret after that. */
export function putSuggestion(
  text: string,
  token: Pick<SuggestToken, "start" | "end">,
  value: string,
): { value: string; caret: number } {
  const next = `${text.slice(0, token.start)}${value} ${text.slice(token.end).replace(/^ /u, "")}`;
  return { value: next, caret: token.start + value.length + 1 };
}

/** A member a message may mention. */
export interface Mentionable {
  id: string;
  /** The name the crew knows. */
  name: string;
}

/**
 * Who a message mentions: every "@" at a word's edge followed by a member's name (the longest
 * that fits) that ends at a word's edge. Each member once, in the order first mentioned.
 */
export function mentionsIn(text: string, members: readonly Mentionable[]): string[] {
  if (!text.includes("@") || !members.length) return [];
  const names = members
    .map((member) => ({ id: member.id, folded: fold(member.name) }))
    .filter((member) => member.folded.length > 0)
    .sort((a, b) => b.folded.length - a.folded.length);
  const found: string[] = [];
  for (const range of mentionRanges(text, names)) {
    if (!found.includes(range.id)) found.push(range.id);
  }
  return found;
}

/** Where a message's mentions are (UTF-16 offsets of "@Name"), for their look in the bubble. */
export function mentionSpans(
  text: string,
  members: readonly Mentionable[],
): Array<{ start: number; end: number; id: string }> {
  if (!text.includes("@") || !members.length) return [];
  const names = members
    .map((member) => ({ id: member.id, folded: fold(member.name) }))
    .filter((member) => member.folded.length > 0)
    .sort((a, b) => b.folded.length - a.folded.length);
  return mentionRanges(text, names);
}

function mentionRanges(
  text: string,
  names: ReadonlyArray<{ id: string; folded: string }>,
): Array<{ start: number; end: number; id: string }> {
  const out: Array<{ start: number; end: number; id: string }> = [];
  let from = 0;
  for (;;) {
    const at = text.indexOf("@", from);
    if (at < 0) break;
    from = at + 1;
    const sign = before(text, at);
    if (sign && WORDY.test(sign)) continue;
    // Folding keeps a text's length for the letters names use; compare piece by piece.
    for (const name of names) {
      const end = matchEnd(text, at + 1, name.folded);
      if (end === null) continue;
      const next = text.slice(end, end + 2);
      if (next && WORDY.test([...next][0] ?? "")) continue;
      out.push({ start: at, end, id: name.id });
      from = end;
      break;
    }
  }
  return out;
}

/** Where `folded` ends when the text from `start` folds to it, or null. */
function matchEnd(text: string, start: number, folded: string): number | null {
  let built = "";
  let index = start;
  for (const ch of text.slice(start)) {
    if (built.length >= folded.length) break;
    built += fold(ch);
    index += ch.length;
    if (!folded.startsWith(built)) return null;
  }
  return built === folded ? index : null;
}

/** How a message is heard by the player: it mentions them (and few enough others), it mentions
 * them among too many, or not at all. */
export function mentionOf(
  text: string,
  me: string | undefined,
  members: readonly Mentionable[],
): "me" | "crowd" | "none" {
  if (!me) return "none";
  const ids = mentionsIn(text, members);
  if (!ids.includes(me)) return "none";
  return ids.length > MENTION_NOTIFY_MAX ? "crowd" : "me";
}
