// The picker's emoji (CHAT2, docs: chat2.md §3.1): one TSV per language, made by
// scripts/emoji-data.mjs from Unicode Emoji 17.0 and CLDR 48.2.0. The list keeps Unicode's order
// and its nine groups; each row has the emoji without a skin tone, its tones, its name and the
// folded words a search finds it by. Loaded on the picker's first use, never at start.

/** The groups in the picker's order (their names are in the chat2 locale module). */
export const EMOJI_GROUPS = [
  "smileys",
  "people",
  "nature",
  "food",
  "travel",
  "activities",
  "objects",
  "symbols",
  "flags",
] as const;
export type EmojiGroup = (typeof EMOJI_GROUPS)[number];

export type EmojiLanguage = "tr" | "en";

export interface EmojiEntry {
  /** The emoji as the picker inserts it without a skin tone (fully qualified). */
  emoji: string;
  group: EmojiGroup;
  /**
   * Its skin tones: none; "rule" (a tone after each code point in `toneBases`, a VS16 right after
   * one dropped); or the five written out (people holding hands keep the hands without one).
   */
  tones: "rule" | readonly string[] | null;
  /** CLDR's name, in lower case as CLDR writes it ("sevinç gözyaşları"). */
  name: string;
  /** Folded search words, each between spaces (" kahkaha komik "). */
  words: string;
  /** Turkish data: the English name, for a Turkish search in English ("fire" finds 🔥 first). */
  fallbackName: string;
  /** Turkish data: English words the Turkish ones do not find ("skull"); like `words`. */
  fallback: string;
  /** Place in the list. */
  index: number;
}

export interface EmojiData {
  lang: EmojiLanguage;
  entries: readonly EmojiEntry[];
  /** The code points a skin tone follows (for `tones: "rule"`). */
  toneBases: ReadonlySet<number>;
}

const MAGIC = "PL-EMOJI";

/** Reads one data file; a file of another format or a broken row throws. */
export function parseEmojiData(text: string): EmojiData {
  const lines = text.split(/\r?\n/).filter((line) => line.length > 0);
  const header = (lines[0] ?? "").split("\t");
  if (header[0] !== MAGIC || header[1] !== "1") throw new Error("Not an emoji data file");
  const lang = header[2] === "en" ? "en" : "tr";
  const toneBases = new Set(
    (header[6] ?? "")
      .split(" ")
      .filter(Boolean)
      .map((hex) => Number.parseInt(hex, 16)),
  );
  const entries = lines.slice(1).map((line, index): EmojiEntry => {
    const [emoji = "", group = "", tones = "", name = "", words = "", english = "", fallback = ""] =
      line.split("\t");
    const groupId = EMOJI_GROUPS[Number(group)];
    if (!emoji || !groupId || !name) throw new Error(`Broken emoji row ${index + 1}`);
    return {
      emoji,
      group: groupId,
      tones: tones === "1" ? "rule" : tones ? tones.split(" ") : null,
      name,
      words: words ? ` ${words} ` : "",
      fallbackName: english,
      fallback: fallback ? ` ${fallback} ` : "",
      index,
    };
  });
  return { lang, entries, toneBases };
}

const loading = new Map<EmojiLanguage, Promise<EmojiData>>();

/** The data in one language, loaded once (a failed load is tried again next time). */
export function loadEmojiData(lang: EmojiLanguage): Promise<EmojiData> {
  let pending = loading.get(lang);
  if (!pending) {
    pending = (
      lang === "en" ? import("./data/emoji.en.tsv?raw") : import("./data/emoji.tr.tsv?raw")
    ).then((module) => parseEmojiData(module.default));
    pending.catch(() => loading.delete(lang));
    loading.set(lang, pending);
  }
  return pending;
}

/** The emoji's name for the screen and the footer, its first letter in upper case. */
export function displayName(name: string, lang: EmojiLanguage): string {
  const first = name.codePointAt(0);
  if (first === undefined) return name;
  const head = String.fromCodePoint(first);
  return head.toLocaleUpperCase(lang === "tr" ? "tr" : "en") + name.slice(head.length);
}
