// Emoji search (CHAT2 §3.1): in the interface's language; a Turkish search also finds English
// ("fire", "lol"). Folded as every search in the app (`fold`): "I", "İ" and "ı" match "i" and
// marks go, so "gulmek" finds "gülmek". Each word of the query must start a word of the emoji; a
// query of three letters or more also matches inside a name or a keyword. Ranked: the name
// itself, a name that starts with the query, the name's words, its keywords, then the same in
// English, then inside a word; Unicode's order within a rank. A pasted emoji finds itself.
import { fold } from "@pl/ui/search";
import type { EmojiData, EmojiEntry } from "./data";
import { bareEmoji } from "./tone";

const split = (text: string) => text.split(/[^\p{L}\p{N}+#*]+/u).filter(Boolean);
const PICTOGRAPHIC = /\p{Extended_Pictographic}|\p{Regional_Indicator}/u;

/**
 * The emoji chats use most (Unicode's emoji frequency list, then a game server's own), most
 * used first. The first twenty come first among the emoji a query finds the same way (by name,
 * by keyword, in English): "kalp" puts the red heart before the anatomical one, "göz" the eyes.
 */
const POPULAR = [
  ..."😂❤️🤣👍😭🙏😘🥰😍😊🎉😁💕🥺😅🔥☺️🤦♥️🤷🙄😆🤗😉🎂🤔👏🙂😳🥳😎👌💜😔💪✨💖👀😋😏😢👉💗😩💯🌹💞🎈💙😃😡💐😜🙈✌️😄🤩😀💓🤪😌💔✅😇🎁💘🤞😑🙌😒🌸🥴☀️💚💛🤭😐✔️💥😞🙃😬🤤💋🖤🤍😻🌟⭐😴🍀😤😪😱🌺😥🙋💀👇🎮🏆💎⛏️⚔️🛡️🏹🗡️🧱😈🤡👑🐉",
].reduce<string[]>((list, ch) => {
  // The spread splits a VS16 from its emoji: it joins the one before.
  if (ch === "\u{fe0f}" && list.length) list[list.length - 1] += ch;
  else list.push(ch);
  return list;
}, []);
const BOOSTED = 20;
const POPULARITY = new Map(
  POPULAR.slice(0, BOOSTED).map((emoji, index) => [bareEmoji(emoji), index]),
);

interface FoldedName {
  /** The whole name, folded. */
  whole: string;
  /** Its words between spaces (" a b c "). */
  words: string;
}

interface Folded {
  name: FoldedName;
  /** The English name of Turkish data. */
  english: FoldedName;
}

/** The folded names, made on the first search in a language and kept with its data. */
const cache = new WeakMap<EmojiData, Folded[]>();

function foldName(name: string): FoldedName {
  const whole = fold(name);
  return { whole, words: ` ${split(whole).join(" ")} ` };
}

function foldedNames(data: EmojiData): Folded[] {
  let names = cache.get(data);
  if (!names) {
    names = data.entries.map((entry) => ({
      name: foldName(entry.name),
      english: foldName(entry.fallbackName),
    }));
    cache.set(data, names);
  }
  return names;
}

/** Whether a word of `words` (" a b c ") starts with `token`. */
const startsAWord = (words: string, token: string) => words.includes(` ${token}`);

/** Whether `words` (" a b c ") holds `token` as a whole word. */
const hasWord = (words: string, token: string) => words.includes(` ${token} `);

/**
 * How an entry matches, best first, with its band: within the bands of whole words (0), keywords
 * (2) and English (3) the most used emoji come first; within the name's beginnings (1) the order
 * is the rank's ("göz": the eye, then the eyes, then tears of joy); inside a word (4) last.
 */
function rank(
  folded: Folded,
  entry: EmojiEntry,
  query: string,
  tokens: string[],
): [tier: number, band: number] | null {
  const { name, english } = folded;
  if (name.whole === query) return [0, 0];
  if (tokens.every((token) => hasWord(name.words, token))) return [1, 0];
  if (name.whole.startsWith(query)) return [2, 1];
  const inName = (token: string) => startsAWord(name.words, token);
  if (tokens.every(inName)) return [3, 1];
  const inWords = (token: string) => inName(token) || startsAWord(entry.words, token);
  if (tokens.every(inWords)) return [4, 2];
  if (english.whole) {
    if (english.whole === query) return [5, 3];
    if (tokens.every((token) => hasWord(english.words, token))) return [6, 3];
    if (english.whole.startsWith(query)) return [7, 3];
    const inEnglish = (token: string) => startsAWord(english.words, token);
    if (tokens.every(inEnglish)) return [8, 3];
    const anywhere = (token: string) =>
      inWords(token) || inEnglish(token) || startsAWord(entry.fallback, token);
    if (tokens.every(anywhere)) return [9, 3];
  }
  if (query.length >= 3 && (name.whole.includes(query) || entry.words.includes(query))) {
    return [10, 4];
  }
  return null;
}

/** The first grapheme of a text. */
function firstGrapheme(text: string): string {
  const segment = new Intl.Segmenter("en", { granularity: "grapheme" }).segment(text);
  return segment[Symbol.iterator]().next().value?.segment ?? text;
}

/** The entries a query finds, best first; nothing for an empty query. */
export function searchEmoji(data: EmojiData, text: string): EmojiEntry[] {
  const trimmed = text.trim();
  if (!trimmed) return [];
  if (PICTOGRAPHIC.test(trimmed)) {
    const wanted = bareEmoji(firstGrapheme(trimmed));
    return data.entries.filter((entry) => bareEmoji(entry.emoji) === wanted);
  }
  const tokens = split(fold(trimmed));
  if (!tokens.length) return [];
  const query = tokens.join(" ");
  const names = foldedNames(data);
  const found: Array<{ entry: EmojiEntry; band: number; popular: number; tier: number }> = [];
  for (const entry of data.entries) {
    const folded = names[entry.index];
    if (!folded) continue;
    const match = rank(folded, entry, query, tokens);
    if (!match) continue;
    const [tier, band] = match;
    const popular = band === 1 ? BOOSTED : (POPULARITY.get(bareEmoji(entry.emoji)) ?? BOOSTED);
    found.push({ entry, band, popular, tier });
  }
  found.sort(
    (a, b) =>
      a.band - b.band || a.popular - b.popular || a.tier - b.tier || a.entry.index - b.entry.index,
  );
  return found.map(({ entry }) => entry);
}
