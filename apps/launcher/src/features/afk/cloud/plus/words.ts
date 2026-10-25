// Game money and numbers for AFK+ (afkplus.md §1.4: an amount is shown in words too, so a zero too
// many is seen before it costs anything), the players' shorthands ("40k", "40 bin", "1,5m"), and
// the Turkish case endings that follow a number, a time or a date ("18:20'de", "37.500'ün", "30
// Ekim'e"): they follow how the value is read aloud, so they come from the spoken form's last word
// (vowel harmony, and t after a voiceless consonant). The same rules as afk.projects.gg's.
import i18n from "@/lib/i18n";

export type Lang = "tr" | "en";

/** The interface's language now. */
export function currentLang(): Lang {
  return i18n.language === "en" ? "en" : "tr";
}

/** "37.500" / "37,500": whole amounts of the server's money, grouped by the language. */
export function formatAmount(value: number, lang: Lang = currentLang()): string {
  return new Intl.NumberFormat(lang === "tr" ? "tr-TR" : "en-GB", {
    maximumFractionDigits: 0,
  }).format(Math.round(value));
}

const TR_ONES = ["", "bir", "iki", "üç", "dört", "beş", "altı", "yedi", "sekiz", "dokuz"];
const TR_TENS = ["", "on", "yirmi", "otuz", "kırk", "elli", "altmış", "yetmiş", "seksen", "doksan"];

/** 0-999 in Turkish words ("" for 0): "yüz" alone, never "bir yüz". */
function trHundreds(n: number): string[] {
  const words: string[] = [];
  const h = Math.floor(n / 100);
  const rest = n % 100;
  if (h === 1) words.push("yüz");
  else if (h > 1) words.push(TR_ONES[h] ?? "", "yüz");
  const t = Math.floor(rest / 10);
  const o = rest % 10;
  if (t) words.push(TR_TENS[t] ?? "");
  if (o) words.push(TR_ONES[o] ?? "");
  return words;
}

/** Turkish words for a whole number: 40000 → "kırk bin", 37500 → "otuz yedi bin beş yüz". */
export function trWords(value: number): string {
  let n = Math.floor(Math.abs(value));
  if (n === 0) return "sıfır";
  const words: string[] = [];
  const scales: Array<[number, string]> = [
    [1_000_000_000, "milyar"],
    [1_000_000, "milyon"],
    [1_000, "bin"],
  ];
  for (const [size, name] of scales) {
    const count = Math.floor(n / size);
    if (count) {
      // "bin", not "bir bin"; "bir milyon" keeps its "bir".
      if (!(count === 1 && size === 1_000)) words.push(...trHundreds(count));
      words.push(name);
      n %= size;
    }
  }
  words.push(...trHundreds(n));
  return words.filter(Boolean).join(" ");
}

const EN_ONES = [
  "",
  "one",
  "two",
  "three",
  "four",
  "five",
  "six",
  "seven",
  "eight",
  "nine",
  "ten",
  "eleven",
  "twelve",
  "thirteen",
  "fourteen",
  "fifteen",
  "sixteen",
  "seventeen",
  "eighteen",
  "nineteen",
];
const EN_TENS = [
  "",
  "",
  "twenty",
  "thirty",
  "forty",
  "fifty",
  "sixty",
  "seventy",
  "eighty",
  "ninety",
];

/** 1-99 in English words. */
function enTens(n: number): string {
  if (n < 20) return EN_ONES[n] ?? "";
  const t = Math.floor(n / 10);
  const o = n % 10;
  return o ? `${EN_TENS[t]}-${EN_ONES[o]}` : (EN_TENS[t] ?? "");
}

/** 0-999 in British English: "six hundred and eighty". */
function enHundreds(n: number): string {
  const h = Math.floor(n / 100);
  const rest = n % 100;
  if (!h) return rest ? enTens(rest) : "";
  return rest ? `${EN_ONES[h]} hundred and ${enTens(rest)}` : `${EN_ONES[h]} hundred`;
}

/**
 * English words for a whole number, as British English says it (the interface's English is
 * en-GB): 40000 → "forty thousand", 7680 → "seven thousand six hundred and eighty", 40005 →
 * "forty thousand and five".
 */
export function enWords(value: number): string {
  let n = Math.floor(Math.abs(value));
  if (n === 0) return "zero";
  const parts: string[] = [];
  const scales: Array<[number, string]> = [
    [1_000_000_000, "billion"],
    [1_000_000, "million"],
    [1_000, "thousand"],
  ];
  for (const [size, name] of scales) {
    const count = Math.floor(n / size);
    if (count) {
      parts.push(`${enHundreds(count)} ${name}`);
      n %= size;
    }
  }
  if (n) parts.push(parts.length && n < 100 ? `and ${enTens(n)}` : enHundreds(n));
  return parts.join(" ");
}

/** A number in words in the interface's language. */
export function amountWords(value: number, lang: Lang = currentLang()): string {
  return lang === "tr" ? trWords(value) : enWords(value);
}

/**
 * So many of an item: "64 Elmas" (Turkish takes no plural after a number); the game's English
 * names are singular and their plurals cannot be guessed ("Totem of Undying"), so "64 × Diamond".
 */
export function qtyItem(qty: number | string, name: string, lang: Lang = currentLang()): string {
  return lang === "en" ? `${qty} × ${name}` : `${qty} ${name}`;
}

/**
 * An amount typed by the player: digits with the usual separators ("40.000", "40,000", "40 000"),
 * or with the players' shorthands ("40k", "40 bin", "1,5m", "1.5 milyon"). Null when it is not one.
 */
export function parseAmount(text: string): number | null {
  const raw = text.trim().toLocaleLowerCase("tr").replace(/\s+/g, " ");
  if (!raw) return null;
  const short = /^(\d+(?:[.,]\d{1,3})?)\s?(k|bin|m|mn|milyon|million)$/.exec(raw);
  if (short?.[1] && short[2]) {
    const base = Number(short[1].replace(",", "."));
    const factor = short[2] === "k" || short[2] === "bin" ? 1_000 : 1_000_000;
    return Number.isFinite(base) ? Math.round(base * factor) : null;
  }
  // Separators only between groups of three ("40.000", "1,250,000"), or none at all.
  if (/^\d{1,3}([., ]\d{3})+$/.test(raw)) return Number(raw.replace(/[., ]/g, ""));
  if (/^\d+$/.test(raw)) return Number(raw);
  return null;
}

// --- Turkish case endings ------------------------------------------------------------------------

export type TrCase = "loc" | "dat" | "abl" | "acc" | "gen";

const BACK = new Set(["a", "ı", "o", "u", "â", "û"]);
const VOWELS = new Set(["a", "e", "ı", "i", "o", "ö", "u", "ü", "â", "î", "û"]);
const VOICELESS = new Set(["ç", "f", "h", "k", "p", "s", "ş", "t"]);

/** The ending a word takes in a case, by how it is read: "Ekim" + dat → "e", "Mart" + loc → "ta". */
export function trEnding(word: string, kind: TrCase): string {
  const w = word.toLocaleLowerCase("tr").replace(/[^a-zçğıöşüâîû]/g, "");
  if (!w) return "";
  const letters = [...w];
  const lastVowel = [...letters].reverse().find((c) => VOWELS.has(c)) ?? "e";
  const last = letters[letters.length - 1] ?? "";
  const back = BACK.has(lastVowel);
  const endsVowel = VOWELS.has(last);
  const hard = VOICELESS.has(last);
  const four =
    lastVowel === "a" || lastVowel === "ı" || lastVowel === "â"
      ? "ı"
      : lastVowel === "o" || lastVowel === "u" || lastVowel === "û"
        ? "u"
        : lastVowel === "ö" || lastVowel === "ü"
          ? "ü"
          : "i";
  const two = back ? "a" : "e";
  switch (kind) {
    case "loc":
      return `${hard ? "t" : "d"}${two}`;
    case "abl":
      return `${hard ? "t" : "d"}${two}n`;
    case "dat":
      return endsVowel ? `y${two}` : two;
    case "acc":
      return endsVowel ? `y${four}` : four;
    case "gen":
      return endsVowel ? `n${four}n` : `${four}n`;
  }
}

/** How the end of a number is read: the last word of its Turkish words ("37.500" → "yüz"). */
function spokenTail(value: number): string {
  const words = trWords(value).split(" ");
  return words[words.length - 1] ?? "";
}

/** The Turkish names of the letters, for names that end in letters read one by one. */
const LETTER_NAMES: Readonly<Record<string, string>> = {
  A: "a",
  B: "be",
  C: "ce",
  Ç: "çe",
  D: "de",
  E: "e",
  F: "fe",
  G: "ge",
  Ğ: "ge",
  H: "he",
  I: "ı",
  İ: "i",
  J: "je",
  K: "ke",
  L: "le",
  M: "me",
  N: "ne",
  O: "o",
  Ö: "ö",
  P: "pe",
  Q: "kü",
  R: "re",
  S: "se",
  Ş: "şe",
  T: "te",
  U: "u",
  Ü: "ü",
  V: "ve",
  W: "ve",
  X: "iks",
  Y: "ye",
  Z: "ze",
};

/**
 * The last letter's name when a name ends in up to three capitals: an abbreviation ("Deniz_TR",
 * "AFK") or a capital after small letters ("MertK") is read letter by letter ("re", "ke"); a
 * longer run of capitals is a word written in capitals ("AHMET").
 */
function spelledTail(token: string): string | null {
  const run = /([A-ZÇĞİÖŞÜ]+)$/.exec(token)?.[1];
  if (!run || run.length > 3) return null;
  return LETTER_NAMES[run.slice(-1)] ?? null;
}

/**
 * A number, time or date as written, with its Turkish ending after an apostrophe:
 * "37.500" + dat → "37.500'e", "18:20" + loc → "18:20'de", "04:00" + loc → "04:00'te",
 * "30 Ekim" + dat → "30 Ekim'e", "2026" + dat → "2026'ya".
 */
export function trWith(text: string, kind: TrCase): string {
  const trimmed = String(text).trim();
  const time = /(\d{1,2})[:.](\d{2})$/.exec(trimmed);
  let tail: string;
  if (time?.[1] && time[2]) {
    const minutes = Number(time[2]);
    tail = spokenTail(minutes || Number(time[1]));
  } else {
    const lastToken = trimmed.split(/\s+/).pop() ?? "";
    const digits = lastToken.replace(/[.,\s]/g, "");
    // A name that ends in digits is read by them too: "Kaan_07" + dat → "Kaan_07'ye" (yedi); one
    // that ends in a short run of capitals by its last letter: "Deniz_TR" + dat → "Deniz_TR'ye".
    const trailing = /(\d+)$/.exec(digits)?.[1];
    tail = trailing ? spokenTail(Number(trailing)) : (spelledTail(lastToken) ?? lastToken);
  }
  return `${trimmed}'${trEnding(tail, kind)}`;
}

/** The value with its Turkish ending in Turkish; as it is in English. */
export function withCase(text: string, kind: TrCase, lang: Lang = currentLang()): string {
  return lang === "tr" ? trWith(text, kind) : text;
}

/** An amount, with its Turkish ending in Turkish ("40.000'in" / "40,000"). */
export function amountWithCase(value: number, kind: TrCase, lang: Lang = currentLang()): string {
  return withCase(formatAmount(value, lang), kind, lang);
}
