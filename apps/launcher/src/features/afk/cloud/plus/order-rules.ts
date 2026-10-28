// AFK+ typed orders: the free rule check of the player's words, run as an instant hint while they
// type (projects-studio services/cloud-afk/README.md "AFK+ trade orders" → Typed orders; the
// Worker runs the same rules exactly and the agent again: their word is final). A port of the
// contract's reference (test-vectors/afkplus-intake.mjs `checkText`, with afkplus-money.mjs's
// amount reading and afkplus-text.mjs's contact rules); vectors/afkplus-intake-v1.json `rules` run
// in order-rules.test.ts. The friends service's word list is the one part left out: it never ships
// with the launcher (social/chat/text.ts), so a text only it refuses gets its answer from the
// service ("Kontrol et").
import {
  checkForm,
  cleanChatText,
  hasText,
  looksLikeLink,
  looksLikePhone,
} from "@/features/social/chat/text";

/** The typed text's length bounds, in code points after cleaning. */
export const TEXT_MIN = 3;
export const TEXT_MAX = 200;
/** A generic profile's `money.maxDeal`; a Studio profile has its own. */
export const GENERIC_MAX_DEAL = 100_000_000;

/** The reasons a check gives (`reject:<reason>`); `loosen` carries the hard rule it tried. */
export type RuleReason =
  | "length"
  | "link"
  | "command"
  | "personal"
  | "unsafe"
  | "loosen"
  | "too_big"
  | "orders_full"
  | "rate"
  | "daily";

export type LoosenRule =
  | "prepay"
  | "pay_cmd"
  | "disclose"
  | "hide_ai"
  | "rmt"
  | "off_platform"
  | "third_party";

export interface RuleContext {
  /** The account's profile's `money.maxDeal` (default: the generic profile's). */
  maxDeal?: number;
  active?: number;
  orderMax?: number;
  /** Checks in the last minute (the Worker's rate binding: 3). */
  minute?: number;
  today?: number;
  perDay?: number;
  /**
   * The server's money name (the profile's `money.label`, else the player's `trade.currency`): an
   * amount written with it is no real money, nor, when it is a real-money word ("TL"), the rmt
   * phrases made of it ("tl ile").
   */
  currency?: string | null;
}

export interface RuleCheck {
  /** `ok`, `question:limit` (stored only with a limit), `reject:<reason>` or `reject:loosen:<rule>`. */
  result: string;
  /** `approval_pref`: the authority decides, not the text. */
  notes: string[];
}

// --- the money reading (afkplus-money.mjs) ------------------------------------------------------

interface Unit {
  word: string;
  value: number;
  suffix: string;
}

/** Multipliers, longest word first; Turkish case endings written without an apostrophe. */
const UNITS: Unit[] = [
  { word: "milyon", value: 1_000_000, suffix: "(?:dan|luk|da|la|un|a|u)?" },
  { word: "million", value: 1_000_000, suffix: "" },
  { word: "thousand", value: 1_000, suffix: "" },
  { word: "bin", value: 1_000, suffix: "(?:den|lik|de|le|in|e|i)?" },
  { word: "kk", value: 1_000_000, suffix: "" },
  { word: "mn", value: 1_000_000, suffix: "" },
  { word: "k", value: 1_000, suffix: "" },
  { word: "m", value: 1_000_000, suffix: "" },
];
const UNIT_PATTERNS = UNITS.map((unit) => ({
  unit,
  pattern: new RegExp(`^${unit.word}${unit.suffix}`, "u"),
}));

/** Real-money words and signs: an amount written with one of them is real money talk (RMT). */
const CURRENCY_WORDS = ["tl", "lira", "try", "usd", "dolar", "dollar", "euro", "eur"];
const CURRENCY_SIGNS = ["₺", "$", "€"];

const FOLD: Record<string, string> = {
  ç: "c",
  ğ: "g",
  ı: "i",
  ö: "o",
  ş: "s",
  ü: "u",
  â: "a",
  î: "i",
  û: "u",
};

/**
 * The form money is read in: NFKC, lower case the Turkish way ("I" → "ı", "İ" → "i"), Turkish
 * letters folded to ASCII, typographic apostrophes as "'", white space runs as one space, trimmed.
 */
export function moneyForm(text: string): string {
  const lower = text.normalize("NFKC").replaceAll("İ", "i").replaceAll("I", "ı").toLowerCase();
  let out = "";
  for (const char of lower) out += FOLD[char] ?? char;
  return out
    .replace(/[‘’ʼ´`]/gu, "'")
    .replace(/\s+/gu, " ")
    .trim();
}

const isDigit = (char: string | undefined) => char !== undefined && char >= "0" && char <= "9";
const isWordChar = (char: string | undefined) => char !== undefined && /[\p{L}\p{N}_]/u.test(char);

function caseEnding(form: string, at: number): number {
  const match = /^'\p{L}+/u.exec(form.slice(at));
  return match === null ? 0 : match[0].length;
}

function boundaryAt(form: string, at: number): boolean {
  return at >= form.length || !isWordChar(form[at]);
}

function unitAt(form: string, at: number): { unit: Unit; end: number } | null {
  let start = at;
  if (form[start] === " ") start += 1;
  for (const { unit, pattern } of UNIT_PATTERNS) {
    const match = pattern.exec(form.slice(start));
    if (match === null) continue;
    let end = start + match[0].length;
    end += caseEnding(form, end);
    if (boundaryAt(form, end)) return { unit, end };
  }
  return null;
}

function currencyAt(form: string, at: number): number {
  let start = at;
  if (form[start] === " ") start += 1;
  for (const sign of CURRENCY_SIGNS) {
    if (form.startsWith(sign, start)) return start + sign.length;
  }
  for (const word of CURRENCY_WORDS) {
    if (form.startsWith(word, start)) {
      let end = start + word.length;
      if (word.length >= 4) {
        while (end < form.length && /\p{L}/u.test(form[end] ?? "")) end += 1;
      }
      end += caseEnding(form, end);
      if (boundaryAt(form, end)) return end;
    }
  }
  return -1;
}

/**
 * The server's own money name right at `at` (after at most one space), in the money form: its end,
 * else -1. Like a currency word: an ending after an apostrophe, and on names of 4 letters or more
 * letters glued on ("ripplea").
 */
function ownAt(form: string, at: number, own: string | null): number {
  if (own === null || own === "") return -1;
  let start = at;
  if (form[start] === " ") start += 1;
  if (!form.startsWith(own, start)) return -1;
  let end = start + own.length;
  if (own.length >= 4 && /\p{L}$/u.test(own)) {
    while (end < form.length && /\p{L}/u.test(form[end] ?? "")) end += 1;
  }
  end += caseEnding(form, end);
  return boundaryAt(form, end) ? end : -1;
}

type Digits = { value: number; shape: string } | { problem: string };

/** The value of digits with separators and a multiplier, or why it cannot be told. */
export function readDigits(digits: string, multiplier: number): Digits {
  const groups = digits.split(/[.,]/);
  const seps = digits.replace(/[0-9]/g, "");
  const first = groups[0] ?? "";
  if (seps.length === 0) {
    if (first.length > 1 && first.startsWith("0")) return { problem: "leading_zero" };
    return { value: Number(first) * multiplier, shape: multiplier === 1 ? "plain" : "unit" };
  }
  const groupedTail = (parts: string[]) => parts.every((part) => part.length === 3);
  const leadOk = first.length >= 1 && first.length <= 3 && !first.startsWith("0");
  const kinds = new Set(seps);
  if (kinds.size === 1) {
    const rest = groups.slice(1);
    if (leadOk && groupedTail(rest)) {
      if (multiplier === 1) return { value: Number(groups.join("")), shape: "grouped" };
      if (seps.length === 1) return { problem: "grouping_with_unit" };
      return { value: Number(groups.join("")) * multiplier, shape: "grouped_unit" };
    }
    const decimals = rest[0] ?? "";
    if (seps.length === 1 && decimals.length <= 2) {
      if (multiplier === 1) return { problem: "decimal_without_unit" };
      if (first.length > 1 && first.startsWith("0")) return { problem: "leading_zero" };
      const scale = 10 ** decimals.length;
      const whole = Number(first) * scale + Number(decimals);
      return { value: (whole * multiplier) / scale, shape: "decimal_unit" };
    }
    return { problem: "malformed" };
  }
  const last = seps[seps.length - 1];
  const head = seps.slice(0, -1);
  const tail = groups[groups.length - 1] ?? "";
  if (
    multiplier === 1 &&
    leadOk &&
    head.length >= 1 &&
    [...head].every((sep) => sep !== last) &&
    groupedTail(groups.slice(1, -1)) &&
    tail.length >= 1 &&
    tail.length <= 2
  ) {
    if (/^0+$/.test(tail)) return { value: Number(groups.slice(0, -1).join("")), shape: "mixed" };
    return { problem: "not_integer" };
  }
  return { problem: "malformed" };
}

export interface MoneyToken {
  start: number;
  end: number;
  raw: string;
  digits: string;
  multiplier: number;
  unit: string | null;
  rmt: boolean;
  value?: number;
  shape?: string;
  problem?: string;
}

/**
 * The number-like tokens of a text, left to right; digits glued to letters are no amount. `own`:
 * the server's money name (the profile's `money.label`, else the player's `trade.currency`): an
 * amount written with it, the name after it or the sign before it, is the server's money, not real
 * money, though the name is a real-money word or sign ("TL", "$"); every other one still is.
 */
export function tokenize(
  text: string,
  own: string | null = null,
): { form: string; tokens: MoneyToken[]; stray: number } {
  const form = moneyForm(text);
  const ownForm = own === null ? null : moneyForm(own);
  const tokens: MoneyToken[] = [];
  let stray = 0;
  let i = 0;
  while (i < form.length) {
    if (!isDigit(form[i])) {
      i += 1;
      continue;
    }
    const digits = /^\d+(?:[.,]\d+)*/.exec(form.slice(i))?.[0] ?? "";
    let end = i + digits.length;
    const before = form[i - 1];
    if (isWordChar(before)) {
      stray += 1;
      i = end;
      while (i < form.length && isWordChar(form[i])) i += 1;
      continue;
    }
    const loneSeparator = (before === "." || before === ",") && !isDigit(form[i - 2]);
    const sign = CURRENCY_SIGNS.includes(before ?? "")
      ? (before as string)
      : before === " " && CURRENCY_SIGNS.includes(form[i - 2] ?? "")
        ? (form[i - 2] as string)
        : null;
    let rmt = sign !== null && sign !== ownForm;
    let unit: Unit | null = null;
    const found = unitAt(form, end);
    if (found !== null) {
      unit = found.unit;
      end = found.end;
    } else if (form[end] === "'") {
      end += caseEnding(form, end);
    }
    const ownEnd = ownAt(form, end, ownForm);
    const currencyEnd = ownEnd === -1 ? currencyAt(form, end) : -1;
    if (ownEnd !== -1) {
      end = ownEnd;
    } else if (currencyEnd !== -1) {
      rmt = true;
      end = currencyEnd;
    }
    if (!boundaryAt(form, end)) {
      stray += 1;
      i = end;
      while (i < form.length && isWordChar(form[i])) i += 1;
      continue;
    }
    const multiplier = unit === null ? 1 : unit.value;
    const token: MoneyToken = {
      start: i,
      end,
      raw: form.slice(i, end),
      digits,
      multiplier,
      unit: unit === null ? null : unit.word,
      rmt,
    };
    if (loneSeparator) token.problem = "malformed";
    else Object.assign(token, readDigits(digits, multiplier));
    tokens.push(token);
    i = end;
  }
  return { form, tokens, stray };
}

/**
 * One amount the player types in a field ("40 bin", "40k", "40.000"): the number, or null when
 * it cannot be told for sure (the field then asks again). The same reading as the agent's. `own`:
 * as in `tokenize`.
 */
export function readAmount(
  text: string,
  maxDeal = GENERIC_MAX_DEAL,
  own: string | null = null,
): number | null {
  const { tokens } = tokenize(text, own);
  const [token] = tokens;
  if (tokens.length !== 1 || token === undefined || token.problem !== undefined) return null;
  const value = token.value ?? 0;
  if (!Number.isInteger(value) || value <= 0 || value > maxDeal || token.rmt) return null;
  return value;
}

// --- contact data (afkplus-text.mjs) ---------------------------------------------------------------

function ibanValid(candidate: string): boolean {
  const moved = `${candidate.slice(4)}${candidate.slice(0, 4)}`.toUpperCase();
  let rest = 0;
  for (const char of moved) {
    const value = /[0-9]/.test(char) ? Number(char) : char.charCodeAt(0) - 55;
    rest = Number(`${rest}${value}`) % 97;
  }
  return rest === 1;
}

function hasIban(form: string): boolean {
  for (const match of form.matchAll(/(?<![a-z0-9])[a-z]{2}\d{2}/gu)) {
    const tail = form.slice(match.index).replace(/^((?:[a-z0-9]| (?=[a-z0-9])){0,60}).*$/su, "$1");
    const compact = tail.replaceAll(" ", "").slice(0, 34);
    for (let length = 15; length <= compact.length; length++) {
      if (ibanValid(compact.slice(0, length))) return true;
    }
  }
  return false;
}

function hasTckn(form: string): boolean {
  for (const match of form.matchAll(/(?<!\d)\d{11}(?!\d)/gu)) {
    const d = [...match[0]].map(Number);
    if (d[0] === 0) continue;
    const at = (i: number) => d[i] ?? 0;
    const odd = at(0) + at(2) + at(4) + at(6) + at(8);
    const even = at(1) + at(3) + at(5) + at(7);
    const tenth = (((odd * 7 - even) % 10) + 10) % 10;
    const eleventh = d.slice(0, 10).reduce((sum, digit) => sum + digit, 0) % 10;
    if (at(9) === tenth && at(10) === eleventh) return true;
  }
  return false;
}

function hasCard(form: string): boolean {
  for (const match of form.matchAll(/(?<!\d)\d(?:[ -]?\d){12,18}(?!\d)/gu)) {
    const digits = match[0].replace(/[ -]/g, "");
    let sum = 0;
    for (let i = 0; i < digits.length; i++) {
      let digit = Number(digits[digits.length - 1 - i]);
      if (i % 2 === 1) {
        digit *= 2;
        if (digit > 9) digit -= 9;
      }
      sum += digit;
    }
    if (sum % 10 === 0) return true;
  }
  return false;
}

const EMAIL = /[\p{L}\p{N}._%+-]@[\p{L}\p{N}-]{2,}/u;

/** Contact details an order never carries: `email`, `iban`, `tckn`, `card`, or null. */
export function contactProblem(cleaned: string): string | null {
  const form = checkForm(cleaned);
  if (EMAIL.test(form)) return "email";
  if (hasIban(form)) return "iban";
  if (hasTckn(form)) return "tckn";
  if (hasCard(form)) return "card";
  return null;
}

/** AFK+ sends no link at all: `projects.gg` counts like any other domain here. */
export function anyLink(cleaned: string): boolean {
  return looksLikeLink(checkForm(cleaned).replaceAll("projects", "projectz"));
}

// --- the phrase lists (afkplus-intake.mjs; patterns.rs where they share its name) ------------------

/** patterns.rs `SELF_HARM`. */
const SELF_HARM: readonly string[] = [
  "intihar",
  "kendimi oldur",
  "kendimi oldurecegim",
  "olmek istiyorum",
  "kendime zarar",
  "yasamak istemiyorum",
  "kill myself",
  "suicide",
  "want to die",
  "end my life",
  "hurt myself",
];
/** patterns.rs `MINOR_SAFETY`. */
const MINOR_SAFETY: readonly string[] = [
  "kac yasindasin",
  "yasin kac",
  "kac yasinda",
  "foto at",
  "fotograf at",
  "fotografini at",
  "resim at",
  "resmini at",
  "kimseye soyleme",
  "kimseye deme",
  "sevgilin var mi",
  "nerede oturuyorsun",
  "hangi okul",
  "how old are you",
  "send a pic",
  "send pic",
  "send a photo",
  "dont tell anyone",
  "do not tell anyone",
  "where do you live",
  "are you single",
];
/** patterns.rs `IMPERSONATION`. */
const IMPERSONATION: readonly string[] = [
  "ben adminim",
  "ben admin",
  "yetkiliyim",
  "moderatorum",
  "projects ekibi",
  "projects ekibinden",
  "im an admin",
  "i am an admin",
  "staff here",
  "im staff",
];
/** patterns.rs `ACCOUNT`. */
const ACCOUNT: readonly string[] = [
  "sifre",
  "sifreni",
  "hesabini ver",
  "hesabina gir",
  "hesabini paylas",
  "password",
  "your account",
  "login code",
];
/** patterns.rs `SCAM`: in a typed order, an attempt at the `prepay` rule. */
const SCAM: readonly string[] = [
  "once sen",
  "once at",
  "once gonder",
  "ilk sen",
  "pay first",
  "send first",
  "you first",
  "pay me first",
];
/** patterns.rs `RMT`: in a typed order, an attempt at the `rmt` rule. */
const RMT: readonly string[] = [
  "papara",
  "iban",
  "steam kart",
  "steam karti",
  "hediye kart",
  "hediye karti",
  "gercek para",
  "gercek parayla",
  "real money",
  "paypal",
  "gift card",
  "ininal",
];

/**
 * Attempts to loosen a hard rule (`loosen:<rule>`), checked in this order. Phrases are folded words
 * (patterns.rs's form); a phrase's last word of 4 letters or more may carry an ending.
 */
export const LOOSEN: readonly (readonly [LoosenRule, readonly string[]])[] = [
  [
    "prepay",
    [
      ...SCAM,
      "once ben oder",
      "once ben odey",
      "once ben veri",
      "once ben gonder",
      "once ben yolla",
      "once ben atar",
      "once ode",
      "onden ode",
      "onden oder",
      "onden odey",
      "onden at",
      "onden gonder",
      "onden veri",
      "onden yolla",
      "guvenip ver",
      "guvenip gonder",
      "guvenip yolla",
      "i pay first",
      "ill pay first",
      "trust me",
      "pay upfront",
    ],
  ],
  [
    "pay_cmd",
    [
      "parayi gonder",
      "parayi yolla",
      "parayi at",
      "para gonder",
      "para yolla",
      "para at",
      "pay komut",
      "send money",
      "send the money",
      "send me money",
      "transfer money",
      "transfer the money",
    ],
  ],
  [
    "disclose",
    [
      "butcemi soyle",
      "butcemi yaz",
      "butcemi paylas",
      "bakiyemi soyle",
      "bakiyemi yaz",
      "bakiyemi paylas",
      "limitimi soyle",
      "limitimi yaz",
      "sinirimi soyle",
      "sinirlarimi soyle",
      "en fazla ne verecegimi soyle",
      "tell my budget",
      "tell them my budget",
      "share my budget",
      "say my budget",
      "tell my limit",
      "tell them my limit",
      "tell my balance",
    ],
  ],
  [
    "hide_ai",
    [
      "bot oldugunu soyleme",
      "bot oldugunu sakla",
      "bot oldugunu belli etme",
      "yapay zeka oldugunu soyleme",
      "yapay zeka oldugunu sakla",
      "insan gibi davran",
      "insanmis gibi",
      "gercek oyuncu gibi",
      "ben gibi davran",
      "dont say you are a bot",
      "dont say youre a bot",
      "do not say you are a bot",
      "pretend to be human",
      "pretend you are human",
      "act human",
      "act like a human",
    ],
  ],
  // `rmt` before `off_platform`: payment apps (papara, ininal, a steam card) are real money first.
  // Also an amount written with a real-money word or sign (afkplus-money.mjs `rmt`).
  [
    "rmt",
    [
      ...RMT,
      "tl ile",
      "tl ye",
      "tlye",
      "lira ile",
      "liraya",
      "dolarla",
      "euro ile",
      "gercek parali",
      "irl money",
      "for real money",
      "for cash",
      "nakit",
    ],
  ],
  // `off_platform`: also every name of PLATFORM_WORDS (afkplus-text.mjs) as a whole word.
  [
    "off_platform",
    ["baska uygulama", "ozelden konusalim", "dmden yaz", "other app", "move to dm", "add me on"],
  ],
  [
    "third_party",
    [
      "herkese yaz",
      "herkese mesaj",
      "herkese pm",
      "herkese ozel",
      "mesaj at",
      "mesaj gonder",
      "mesaj yolla",
      "pm at",
      "pm gonder",
      "spam yap",
      "spam at",
      "spamla",
      "message everyone",
      "dm everyone",
      "pm everyone",
      "spam",
    ],
  ],
];

/** "Any price": the order cannot be stored without a limit, so the limit question is asked. */
export const NO_LIMIT: readonly string[] = [
  "her fiyata",
  "her fiyat",
  "kac olursa",
  "ne kadar olursa",
  "kaca olursa",
  "fiyat fark etmez",
  "fiyati fark etmez",
  "fiyat farketmez",
  "fiyati farketmez",
  "limit yok",
  "limitsiz",
  "sinirsiz",
  "sinir yok",
  "any price",
  "whatever price",
  "whatever it costs",
  "no limit",
  "at any cost",
];

/** "Don't ask me": a note on the card (the authority setting decides, not the text). */
export const APPROVAL_PREF: readonly string[] = [
  "bana sorma",
  "bana sormadan",
  "sormadan al",
  "sormadan sat",
  "sormadan tamamla",
  "onaysiz",
  "onay isteme",
  "onay bekleme",
  "dont ask me",
  "do not ask me",
  "without asking",
  "no approval",
];

/**
 * Names of other platforms and payment apps (folded, whole words): AFK+ text never sends anyone
 * elsewhere (`platform`). The bot's own templates may name them; the model's text and persona
 * texts may not.
 */
export const PLATFORM_WORDS: readonly string[] = [
  "discord",
  "dc",
  "instagram",
  "insta",
  "snapchat",
  "snap",
  "telegram",
  "whatsapp",
  "watsap",
  "wp",
  "tiktok",
  "twitch",
  "youtube",
  "steam",
  "papara",
  "ininal",
  "skype",
  "facebook",
  "messenger",
  "kik",
];

// --- the check -------------------------------------------------------------------------------------

/** A text's words in the patterns' form (patterns.rs `words`). */
export function words(text: string): string[] {
  const form = moneyForm(checkForm(text).replaceAll("ı", "i"));
  return form.split(/[^a-z0-9]+/u).filter((word) => word.length > 0);
}

/** Consecutive words; the last may carry an ending when it has 4+ letters. */
export function phraseIn(list: string[], phrase: string): boolean {
  const parts = phrase.split(" ");
  const k = parts.length;
  if (k === 0 || list.length < k) return false;
  const last = parts[k - 1] ?? "";
  for (let i = 0; i + k <= list.length; i++) {
    let ok = true;
    for (let j = 0; j < k - 1; j++) {
      if (list[i + j] !== parts[j]) {
        ok = false;
        break;
      }
    }
    if (!ok) continue;
    const word = list[i + k - 1] ?? "";
    if (word === last || (last.length >= 4 && word.startsWith(last))) return true;
  }
  return false;
}

const anyPhrase = (list: string[], phrases: readonly string[]) =>
  phrases.some((phrase) => phraseIn(list, phrase));

/** The text as it is checked, stored as the order's `note` and sent on: cleaned, on one line. */
export function cleanOrderText(text: string): string {
  return cleanChatText(String(text)).replaceAll("\n", " ");
}

const cp = (text: string) => [...text].length;

/** A slash or backslash command, a section sign or a colour code. */
const COMMAND = /(?:^|\s)[/\\][a-z]/iu;
const COLOUR = /§|&[0-9a-fk-or]/iu;

function platformNamed(cleaned: string): boolean {
  const named = (word: string) =>
    PLATFORM_WORDS.some((name) => (name.length >= 5 ? word.startsWith(name) : word === name));
  return words(cleaned).some(named);
}

/**
 * The free rule check: `{result, notes}`. The first rule that holds answers, in this order:
 * length, link, command, personal, unsafe, loosen, too_big, orders_full, rate, daily.
 */
export function checkOrderText(text: string, ctx: RuleContext = {}): RuleCheck {
  const cleaned = cleanOrderText(text);
  const notes: string[] = [];
  const size = cp(cleaned);
  if (size < TEXT_MIN || size > TEXT_MAX || !hasText(cleaned)) {
    return { result: "reject:length", notes };
  }
  if (anyLink(cleaned)) return { result: "reject:link", notes };
  if (COMMAND.test(cleaned) || COLOUR.test(cleaned)) return { result: "reject:command", notes };
  if (looksLikePhone(cleaned) || contactProblem(cleaned) !== null) {
    return { result: "reject:personal", notes };
  }
  const list = words(cleaned);
  if (
    anyPhrase(list, SELF_HARM) ||
    anyPhrase(list, MINOR_SAFETY) ||
    anyPhrase(list, ACCOUNT) ||
    anyPhrase(list, IMPERSONATION)
  ) {
    return { result: "reject:unsafe", notes };
  }
  const own = ctx.currency ?? null;
  const { tokens } = tokenize(cleaned, own);
  // A name that is a real-money word ("TL"): the rmt phrases made of it ("tl ile") are no attempt.
  const ownForm = own === null ? null : moneyForm(own);
  const ownWord = ownForm !== null && CURRENCY_WORDS.includes(ownForm) ? ownForm : null;
  for (const [rule, all] of LOOSEN) {
    const phrases =
      rule === "rmt" && ownWord !== null
        ? all.filter((phrase) => !(phrase.split(" ")[0] ?? "").startsWith(ownWord))
        : all;
    let hit = anyPhrase(list, phrases);
    if (rule === "off_platform") hit = hit || platformNamed(cleaned);
    if (rule === "rmt") hit = hit || tokens.some((token) => token.rmt);
    if (hit) return { result: `reject:loosen:${rule}`, notes };
  }
  const maxDeal = ctx.maxDeal ?? GENERIC_MAX_DEAL;
  if (tokens.some((token) => token.problem === undefined && (token.value ?? 0) > maxDeal)) {
    return { result: "reject:too_big", notes };
  }
  if ((ctx.active ?? 0) >= (ctx.orderMax ?? 10)) return { result: "reject:orders_full", notes };
  if ((ctx.minute ?? 0) >= 3) return { result: "reject:rate", notes };
  if ((ctx.today ?? 0) >= (ctx.perDay ?? 20)) return { result: "reject:daily", notes };
  if (anyPhrase(list, APPROVAL_PREF)) notes.push("approval_pref");
  if (anyPhrase(list, NO_LIMIT)) return { result: "question:limit", notes };
  return { result: "ok", notes };
}

/** A check's result as the hint under the field reads it: the reason and the rule, or null. */
export function ruleHint(check: RuleCheck): { reason: RuleReason; rule?: LoosenRule } | null {
  if (!check.result.startsWith("reject:")) return null;
  const [, reason, rule] = check.result.split(":");
  return { reason: reason as RuleReason, rule: rule as LoosenRule | undefined };
}
