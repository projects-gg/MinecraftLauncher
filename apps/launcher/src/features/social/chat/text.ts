// What the composer checks while the player types, the same way the friends service checks a
// message (projects-studio services/social: src/chattext.ts, src/chatlinks.ts; README "Chat" →
// "How the checks read the text" and "How links are checked"). CHAT1: links and phone numbers
// never go, a message is at most 300 characters and 5 lines. CHAT2 §3.4: symbols that spell
// letters, digits and dots read as them. CHAT2 §9: while Studio allows links, plain `https` links
// that pass the shared rule table (chat-links.json, copied from the service with its cases in
// text-vectors.json) and Studio's lists may go. The service checks again, and also its word list,
// which it never names and which never ships with the launcher (only 🖕 is checked here); this
// only says why before anything is sent. Mirror changes there; text.test.ts runs every shared case.

import table from "./chat-links.json";

/** Characters in a message (code points, after cleaning). */
export const CHAT_TEXT_MAX = 300;
export const CHAT_LINES_MAX = 5;
/** The counter shows from here on. */
export const CHAT_COUNT_FROM = 250;

/** Why a text cannot go as it is, before the service is asked. */
export type TextProblem = "link" | "phone" | "long";

const cp = (code: number) => String.fromCodePoint(code);

const JOINER = String.fromCharCode(0x200d);
const INVISIBLE = /[\p{Cc}\p{Cf}\p{Co}\p{Cs}\u115F\u1160\u3164\uFFA0\u2800]/gu;
const PICTOGRAPH = /^\p{Extended_Pictographic}$/u;
const EMOJI_TAIL = /^[\uFE0F\u{1F3FB}-\u{1F3FF}]$/u;
/** Variation selectors nobody needs in chat (VS1–VS14, VS17–VS256): invisible data. */
const SPARE_SELECTORS = new RegExp(
  `[${cp(0xfe00)}-${cp(0xfe0d)}${cp(0xe0100)}-${cp(0xe01ef)}]`,
  "gu",
);
/** VS15 and VS16 (text or emoji style) stay, one at a time. */
const STYLE_SELECTORS = new RegExp(
  `([${cp(0xfe0e)}${cp(0xfe0f)}])[${cp(0xfe0e)}${cp(0xfe0f)}]+`,
  "gu",
);

function keepEmojiJoiners(text: string): string {
  if (!text.includes(JOINER)) return text;
  const chars = [...text];
  let out = "";
  for (let i = 0; i < chars.length; i++) {
    const char = chars[i] ?? "";
    if (char !== JOINER) {
      out += char;
      continue;
    }
    let before = i - 1;
    while (before >= 0 && EMOJI_TAIL.test(chars[before] ?? "")) before -= 1;
    if (PICTOGRAPH.test(chars[before] ?? "") && PICTOGRAPH.test(chars[i + 1] ?? "")) out += char;
  }
  return out;
}

/**
 * The text as the service will keep it: NFC, invisible characters dropped (joiners only inside
 * emoji), variation selectors other than VS15/VS16 dropped and those two at most one in a row, at
 * most two marks on a letter, spaces collapsed, empty lines dropped, at most five lines (more join
 * the fifth), trimmed.
 */
export function cleanChatText(value: string): string {
  const text = keepEmojiJoiners(
    value
      .normalize("NFC")
      .replace(/\r\n?|[\u0085\u2028\u2029]/gu, "\n")
      .replace(/[\t\v\f]/g, " ")
      .replace(INVISIBLE, (char) => (char === "\n" || char === JOINER ? char : ""))
      .replace(SPARE_SELECTORS, "")
      .replace(STYLE_SELECTORS, "$1"),
  ).replace(/(\p{M}\p{M})\p{M}+/gu, "$1");
  const lines = text
    .split("\n")
    .map((line) => line.replace(/\s+/gu, " ").trim())
    .filter((line) => line.length > 0);
  if (lines.length > CHAT_LINES_MAX) {
    const rest = lines.splice(CHAT_LINES_MAX - 1).join(" ");
    lines.push(rest);
  }
  return lines.join("\n");
}

/** Characters as the service counts them (code points of the cleaned text). */
export function chatLength(text: string): number {
  return [...cleanChatText(text)].length;
}

const LOOK_ALIKES: Record<string, string> = {
  "\u0430": "a",
  "\u0432": "b",
  "\u0435": "e",
  "\u043A": "k",
  "\u043C": "m",
  "\u043D": "h",
  "\u043E": "o",
  "\u0440": "p",
  "\u0441": "c",
  "\u0442": "t",
  "\u0443": "y",
  "\u0445": "x",
  "\u0455": "s",
  "\u0456": "i",
  "\u0458": "j",
  "\u0501": "d",
  "\u04CF": "l",
  "\u0261": "g",
  "\u03B1": "a",
  "\u03B2": "b",
  "\u03B5": "e",
  "\u03B9": "i",
  "\u03BA": "k",
  "\u03BD": "v",
  "\u03BF": "o",
  "\u03C1": "p",
  "\u03C4": "t",
  "\u03C5": "u",
  "\u03C7": "x",
  "\u03C9": "w",
};

const DIGIT_ZEROS = [0x0660, 0x06f0, 0x07c0, 0x0966, 0x09e6, 0x0a66, 0x0e50, 0x1040, 0x17e0];

/** Small capitals NFKC leaves alone, as the letters they show (ɪ is the dotted "i"). */
const SMALL_CAPITALS = new Map<string, string>(
  (
    [
      [0x1d00, "a"],
      [0x1d01, "ae"],
      [0x1d02, "ae"],
      [0x1d03, "b"],
      [0x1d04, "c"],
      [0x1d05, "d"],
      [0x1d06, "d"],
      [0x1d07, "e"],
      [0x1d08, "e"],
      [0x1d09, "i"],
      [0x1d0a, "j"],
      [0x1d0b, "k"],
      [0x1d0c, "l"],
      [0x1d0d, "m"],
      [0x1d0e, "n"],
      [0x1d0f, "o"],
      [0x1d10, "o"],
      [0x1d11, "o"],
      [0x1d12, "o"],
      [0x1d13, "o"],
      [0x1d14, "oe"],
      [0x1d15, "ou"],
      [0x1d16, "o"],
      [0x1d17, "o"],
      [0x1d18, "p"],
      [0x1d19, "r"],
      [0x1d1a, "r"],
      [0x1d1b, "t"],
      [0x1d1c, "u"],
      [0x1d1d, "u"],
      [0x1d1e, "u"],
      [0x1d1f, "m"],
      [0x1d20, "v"],
      [0x1d21, "w"],
      [0x1d22, "z"],
      [0x0299, "b"],
      [0xa730, "f"],
      [0x0262, "g"],
      [0x029c, "h"],
      [0x026a, "i"],
      [0x029f, "l"],
      [0x0274, "n"],
      [0xa7af, "q"],
      [0x0280, "r"],
      [0xa731, "s"],
      [0x028f, "y"],
    ] as [number, string][]
  ).map(([code, letters]) => [cp(code), letters]),
);

const letterAt = (index: number) => String.fromCharCode(0x61 + index);
const isRegionalIndicator = (code: number) => code >= 0x1f1e6 && code <= 0x1f1ff;

/**
 * The letter or number a symbol stands for where NFKC does not say it: negative circled and
 * squared letters, small capitals, dingbat digits (the tenth is "10"), negative circled 11–20,
 * double circled digits, the circled zeros and the keycap ten.
 */
function standIn(char: string, code: number): string | undefined {
  if (code >= 0x1f150 && code <= 0x1f169) return letterAt(code - 0x1f150);
  if (code >= 0x1f170 && code <= 0x1f189) return letterAt(code - 0x1f170);
  if (code >= 0x2776 && code <= 0x2793) return String(((code - 0x2776) % 10) + 1);
  if (code >= 0x24eb && code <= 0x24f4) return String(code - 0x24eb + 11);
  if (code >= 0x24f5 && code <= 0x24fe) return String(code - 0x24f5 + 1);
  if (code === 0x24ff || code === 0x1f10b || code === 0x1f10c) return "0";
  if (code === 0x1f51f) return "10";
  return SMALL_CAPITALS.get(char);
}

/** Flags whose letters spell a listed word alone (🇦🇶, 🇸🇭🇮🇹, 🇨🇴🇨🇰): the word check reads them as flags. */
const FLAG_WORDS: ReadonlySet<string> = new Set(["aq", "shit", "cock"]);

/**
 * The plain form the checks read: NFKC, lower case the Turkish way, marks and format characters
 * out, look-alike letters as Latin, other digits as ASCII, symbols that spell as what they spell,
 * a run of regional indicators as its letters ("🇩🇮🇸🇨🇴🇷🇩.🇬🇬" reads "discord.gg"); with `words`
 * (the word check) a run in `FLAG_WORDS` reads as a space.
 */
export function checkForm(text: string, words = false): string {
  let form = "";
  let flags = "";
  const endFlags = () => {
    if (flags !== "") {
      form += words && FLAG_WORDS.has(flags) ? " " : flags;
      flags = "";
    }
  };
  const lower = text.normalize("NFKC").replaceAll("İ", "i").replaceAll("I", "ı").toLowerCase();
  for (const char of lower.normalize("NFKD")) {
    if (/[\p{M}\p{Cf}]/u.test(char)) continue;
    const code = char.codePointAt(0) ?? 0;
    if (isRegionalIndicator(code)) {
      flags += letterAt(code - 0x1f1e6);
      continue;
    }
    endFlags();
    const zero = DIGIT_ZEROS.find((start) => code >= start && code < start + 10);
    form +=
      zero === undefined ? (standIn(char, code) ?? LOOK_ALIKES[char] ?? char) : String(code - zero);
  }
  endFlags();
  return form;
}

const TLDS = new Set(
  `ac ad ae af ag ai al am ao aq ar as at au aw ax az ba bb bd be bf bg bh bi bj bm bn bo br bs bt
  bw by bz ca cc cd cf cg ch ci ck cl cm cn co cr cu cv cw cx cy cz de dj dk dm do dz ec ee eg er
  es et eu fi fj fk fm fo fr ga gd ge gf gg gh gi gl gm gn gp gq gr gs gt gu gw gy hk hm hn hr ht
  hu id ie il im in io iq ir is it je jm jo jp ke kg kh ki km kn kp kr kw ky kz la lb lc li lk lr
  ls lt lu lv ly ma mc md me mg mh mk ml mm mn mo mp mq mr ms mt mu mv mw mx my mz na nc ne nf ng
  ni nl no np nr nu nz om pa pe pf pg ph pk pl pm pn pr ps pt pw py qa re ro rs ru rw sa sb sc sd
  se sg sh si sk sl sm sn so sr ss st su sv sx sy sz tc td tf tg th tj tk tl tm tn to tr tt tv tw
  tz ua ug uk us uy uz va vc ve vg vi vn vu wf ws ye yt za zm zw
  com net org info biz xyz online site store shop app dev icu vip win bet pro top club live fun
  link click space website tech cloud email world life today news blog art lol game games porn sex
  xxx mobi name asia tel travel jobs cat edu gov mil int wiki fyi one plus best cool page rocks
  host press digital network software solutions agency studio design media social chat zone team
  center group global company services support systems tools city land red blue pink black green
  gold rest bond cfd sbs cyou monster quest buzz fit run gift gifts moe ink kim lat ltd mom pub
  uno vin wtf ninja guru town works zip mov stream tube video photo pics download free money
  cash shopping market trade bid loan casino poker cam dating date love sexy adult tattoo`
    .split(/\s+/)
    .filter((tld) => tld.length > 0),
);

const LABEL_PATTERN = "[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?";
const DOMAIN = new RegExp(
  `(?:^|[^a-z0-9-])((?:${LABEL_PATTERN}\\.)+([a-z]{2,24}))(?![a-z0-9-])`,
  "g",
);
const IPV4 = /(?:^|[^0-9.])(?:\d{1,3}\.){3}\d{1,3}(?![0-9.]*\d)/;
const OWN_DOMAIN = "projects.gg";
/** The ideographic full stop, a dot always (NFKC has made ｡ and ︒ into it already). */
const FULL_STOP = new RegExp(cp(0x3002), "gu");
/** Round and small square emoji written for a dot, only before a common ending. */
const DOT_EMOJI = [
  0x26ab, 0x26aa, 0x1f534, 0x1f535, 0x1f7e0, 0x1f7e1, 0x1f7e2, 0x1f7e3, 0x1f7e4, 0x1f518, 0x23fa,
  0x25cf, 0x25cb, 0x25c9, 0x25e6, 0x2981, 0x25aa, 0x25ab, 0x25fe, 0x25fd, 0x1f538, 0x1f539,
]
  .map(cp)
  .join("");
const DOT_ENDINGS =
  "com|net|org|gg|io|co|me|tv|xyz|tk|ru|tr|app|dev|site|shop|store|online|live|link|info";
const EMOJI_DOT = new RegExp(
  `(?<=[a-z0-9])[${DOT_EMOJI}]\\s*(?=(?:${DOT_ENDINGS})(?![a-z0-9]))`,
  "gu",
);

function linkForm(form: string): string {
  return form
    .replaceAll("ı", "i")
    .replace(FULL_STOP, ".")
    .replace(EMOJI_DOT, ".")
    .replace(/[·•∙⋅]/gu, ".")
    .replace(/\s*[([{<]\s*(?:\.|dot|nokta)\s*[)\]}>]\s*/g, ".")
    .replace(/\s+(?:dot|nokta)\s+(?=[a-z])/g, ".")
    .replace(/\s+\.\s*(?=[a-z0-9])/g, ".");
}

/** Anything like a web address, a domain or a server's IP (projects.gg may be written). */
export function looksLikeLink(text: string): boolean {
  const form = linkForm(checkForm(text));
  if (/[a-z]{3,8}:\/\/|:\/\/|https?:|www\./.test(form) || IPV4.test(form)) return true;
  const letters = form.replace(/[^a-z0-9]/g, "");
  if (/discord(?:app)?(?:gg|cominvite)|dscgg/.test(letters)) return true;
  for (const match of form.matchAll(DOMAIN)) {
    const domain = match[1] ?? "";
    const own = domain === OWN_DOMAIN || domain.endsWith(`.${OWN_DOMAIN}`);
    if (TLDS.has(match[2] ?? "") && !own) return true;
  }
  return false;
}

/** Digits with at most three separators between them; emoji and skin tones are separators too. */
const NUMBER = new RegExp(
  `\\+?\\d(?:[\\s.\\-/()_,\\p{Extended_Pictographic}${cp(0x1f3fb)}-${cp(0x1f3ff)}]{0,3}\\d)+`,
  "gu",
);

/** Turkish mobiles and landlines, and any number written as international with 10+ digits. */
export function looksLikePhone(text: string): boolean {
  const form = checkForm(text);
  for (const match of form.matchAll(NUMBER)) {
    const run = match[0];
    const digits = run.replace(/\D/g, "");
    if (digits.length < 10) continue;
    if (run.startsWith("+") && digits.length <= 16) return true;
    if (/^00\d{10,}/.test(digits) || /5[0-6]\d{8}/.test(digits) || /0[2-4]\d{9}/.test(digits)) {
      return true;
    }
  }
  return false;
}

/** 🖕 in any tone: the word list's picture, the one listed word the launcher knows. */
const MIDDLE_FINGER = cp(0x1f595);

function usesWords(text: string): boolean {
  return checkForm(text).includes(MIDDLE_FINGER);
}

// --- links (CHAT2) -------------------------------------------------------------------------------

/** Why a link may not go (`chat_rejected` with `link` and this `rule`). */
export type LinkRule =
  | "off"
  | "form"
  | "count"
  | "http"
  | "credentials"
  | "port"
  | "ip"
  | "long"
  | "shortener"
  | "grabber"
  | "adult"
  | "lookalike"
  | "scam"
  | "file"
  | "invite"
  | "blocked";

/** Studio's lists, as `hello.chat.links` brings them: hosts opened without the warning, hosts and
 * pages no link may lead to, whether Discord invites may be linked. */
export interface LinkLists {
  trusted: readonly string[];
  blocked: readonly string[];
  invites: boolean;
}

/** A plain link found in a text: where it is, as written, and its parts. */
export interface FoundLink {
  /** UTF-16 offsets in the text. */
  start: number;
  end: number;
  /** As written (trailing punctuation and a Turkish case ending left out). */
  url: string;
  scheme: string;
  /** What lies between `//` and the first `/`, `?` or `#`, as written. */
  authority: string;
  /** The path, query and fragment, as written. */
  rest: string;
}

/** The rule table's version (the service names the one it applies in `hello.chat.links.rules`). */
export const LINK_RULES_VERSION: number = table.version;
export const LINK_LIMITS = { links: table.limits.links, length: table.limits.length } as const;
/** What the service starts with while Studio has never set its lists. */
export const DEFAULT_LINK_LISTS: LinkLists = {
  trusted: table.studio.trusted,
  blocked: table.studio.blocked,
  invites: true,
};

const URL_CHARS = String.raw`[\p{L}\p{M}\p{N}\-._~:/?#\[\]@!$&'()*+,;=%]`;
const LINK = new RegExp(String.raw`(?<![\p{L}\p{N}])https?:\/\/${URL_CHARS}*`, "giu");
const CASE_ENDING = /'[\p{L}]+$/u;
const TRAILING = /[.,;:!?'*]$/;

function count(text: string, char: string): number {
  let n = 0;
  for (const c of text) if (c === char) n += 1;
  return n;
}

function trimLink(raw: string): string {
  let url = raw;
  for (;;) {
    const scheme = url.indexOf("://") + 3;
    if (url.length <= scheme) return url;
    const ending = CASE_ENDING.exec(url);
    if (ending !== null && ending.index > scheme) url = url.slice(0, ending.index);
    else if (TRAILING.test(url)) url = url.slice(0, -1);
    else if (url.endsWith(")") && count(url, "(") < count(url, ")")) url = url.slice(0, -1);
    else if (url.endsWith("]") && count(url, "[") < count(url, "]")) url = url.slice(0, -1);
    else return url;
  }
}

/** The plain links of a (cleaned) text, in order. */
export function findLinks(text: string): FoundLink[] {
  const out: FoundLink[] = [];
  for (const match of text.matchAll(LINK)) {
    const url = trimLink(match[0]);
    const start = match.index ?? 0;
    const after = url.indexOf("://") + 3;
    const tail = url.slice(after);
    const cut = tail.search(/[/?#]/);
    out.push({
      start,
      end: start + url.length,
      url,
      scheme: url.slice(0, after - 3).toLowerCase(),
      authority: cut < 0 ? tail : tail.slice(0, cut),
      rest: cut < 0 ? "" : tail.slice(cut),
    });
  }
  return out;
}

/** `text` with its plain links blanked out (the other checks read the rest). */
export function withoutLinks(text: string, links: readonly FoundLink[]): string {
  let out = "";
  let at = 0;
  for (const link of links) {
    out += `${text.slice(at, link.start)}${" ".repeat(link.end - link.start)}`;
    at = link.end;
  }
  return out + text.slice(at);
}

const LABEL = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;
const TLD = /^[a-z]{2,24}$/;

function under(host: string, domain: string): boolean {
  return host === domain || host.endsWith(`.${domain}`);
}

function anyUnder(host: string, domains: readonly string[]): boolean {
  return domains.some((domain) => under(host, domain));
}

const CONFUSABLES = table.lookalike.confusables as [string, string][];
const SECOND_LEVEL: ReadonlySet<string> = new Set(table.lookalike.secondLevel);

/** A name as the eye reads it: no hyphens, look-alike letters and digits as one letter. */
export function skeleton(name: string): string {
  let out = name.replaceAll("-", "");
  for (const [from, to] of CONFUSABLES) out = out.replaceAll(from, to);
  return out;
}

const BRANDS = table.lookalike.brands.map((brand) => ({
  keys: brand.keys.map(skeleton),
  official: brand.official,
}));
const OFFICIAL = BRANDS.flatMap((brand) => brand.official);
const LURES: ReadonlySet<string> = new Set(table.lookalike.lures.map(skeleton));
const LURE_MAX = Math.max(...[...LURES].map((lure) => lure.length));

/** Whether `text` is made of lure words only (or is empty). */
function lures(text: string): boolean {
  const can = new Array<boolean>(text.length + 1).fill(false);
  can[0] = true;
  for (let end = 1; end <= text.length; end++) {
    for (let start = Math.max(0, end - LURE_MAX); start < end && !can[end]; start++) {
      can[end] = can[start] === true && LURES.has(text.slice(start, end));
    }
  }
  return can[text.length] === true;
}

/** Edit distance with swaps of two neighbours (optimal string alignment). */
export function typoDistance(a: string, b: string): number {
  const rows = a.length + 1;
  const cols = b.length + 1;
  const d: number[][] = Array.from({ length: rows }, (_, i) =>
    Array.from({ length: cols }, (_, j) => (i === 0 ? j : j === 0 ? i : 0)),
  );
  for (let i = 1; i < rows; i++) {
    for (let j = 1; j < cols; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      const row = d[i] as number[];
      const up = d[i - 1] as number[];
      let best = Math.min(
        (up[j] as number) + 1,
        (row[j - 1] as number) + 1,
        (up[j - 1] as number) + cost,
      );
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        best = Math.min(best, ((d[i - 2] as number[])[j - 2] as number) + 1);
      }
      row[j] = best;
    }
  }
  return (d[a.length] as number[])[b.length] as number;
}

/** The label before the public suffix (`example` of `www.example.com.tr`). */
function mainLabel(labels: readonly string[]): string {
  const n = labels.length;
  const last = labels[n - 1] ?? "";
  const second = labels[n - 2] ?? "";
  return n >= 3 && last.length === 2 && SECOND_LEVEL.has(second) ? (labels[n - 3] ?? "") : second;
}

/** Whether a (valid, lower-case) host borrows a brand's name without being the brand's. */
export function lookalike(host: string): boolean {
  if (anyUnder(host, OFFICIAL)) return false;
  const labels = host.split(".");
  // An official domain as whole labels inside the host, more labels after it.
  for (const domain of OFFICIAL) {
    const parts = domain.split(".");
    for (let i = 0; i + parts.length < labels.length; i++) {
      if (parts.every((part, k) => labels[i + k] === part)) return true;
    }
  }
  const name = skeleton(mainLabel(labels));
  for (const brand of BRANDS) {
    for (const key of brand.keys) {
      if (name === key) return true;
      if (
        key.length >= 6 &&
        Math.abs(name.length - key.length) <= 1 &&
        typoDistance(name, key) === 1
      ) {
        return true;
      }
      if (key.length >= 5) {
        for (let at = name.indexOf(key); at >= 0; at = name.indexOf(key, at + 1)) {
          const before = name.slice(0, at);
          const after = name.slice(at + key.length);
          if (before.length + after.length > 0 && lures(before) && lures(after)) return true;
        }
      }
    }
  }
  return false;
}

const SCAM_LEET: Record<string, string> = {
  "0": "o",
  "1": "i",
  "3": "e",
  "4": "a",
  "5": "s",
  "7": "t",
  "@": "a",
  $: "s",
  "!": "i",
};
const SCAMS = table.scams.phrases;

/** Lower case the Turkish way, marks dropped, ı read as i, leet digits as letters, words apart. */
export function scamForm(text: string): string {
  const lower = text
    .replaceAll("İ", "i")
    .replaceAll("I", "ı")
    .toLowerCase()
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .replaceAll("ı", "i");
  let out = "";
  for (const char of lower) {
    const mapped = SCAM_LEET[char] ?? char;
    out += /^[a-z]$/.test(mapped) ? mapped : " ";
  }
  return ` ${out.replace(/ +/g, " ").trim()} `;
}

export function scammy(text: string): boolean {
  const form = scamForm(text);
  return SCAMS.some((phrase) => form.includes(` ${phrase} `));
}

const SHORTENERS = table.shorteners;
const GRABBERS = table.grabbers;
const MESSENGERS = table.messengers.hosts;
const FILES: ReadonlySet<string> = new Set(table.files.extensions);
const ADULT: ReadonlySet<string> = new Set(table.adult.tlds);
const PHONE_IN_LINK = /(?:^|[^0-9])\+?[0-9]{10,15}(?![0-9])/;

function decoded(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
}

/** The host of a link that passed the first checks, lower case, without a trailing dot. */
export function hostOf(link: FoundLink): string {
  return link.authority.toLowerCase().replace(/\.$/, "");
}

/** A trusted site (its subdomains too): its links open without the warning. */
export function trustedHost(host: string, lists: LinkLists): boolean {
  return anyUnder(host, lists.trusted);
}

/**
 * Whether a list entry names a link: a host (with its subdomains), or a host and the start of its
 * path in whole segments, without regard to case (`youtube.com/@kanal` names `/@kanal` and
 * `/@kanal/videos`, not `/@kanallar`).
 */
export function entryNames(entry: string, host: string, rest: string): boolean {
  const slash = entry.indexOf("/");
  if (slash < 0) return under(host, entry);
  const prefix = entry.slice(slash).toLowerCase();
  const path = rest.toLowerCase();
  if (!under(host, entry.slice(0, slash)) || !path.startsWith(prefix)) return false;
  const next = path.charAt(prefix.length);
  return prefix.endsWith("/") || next === "" || next === "/" || next === "?" || next === "#";
}

const INVITE_HOSTS = table.invites.hosts;
const INVITE_PATHS = table.invites.paths;

function discordInvite(host: string, rest: string): boolean {
  return (
    anyUnder(host, INVITE_HOSTS) || INVITE_PATHS.some((entry) => entryNames(entry, host, rest))
  );
}

/** Why one plain link may not go (or `phone`: it carries a phone number), null when it may. */
export function linkProblem(link: FoundLink, lists: LinkLists): LinkRule | "phone" | null {
  if (link.scheme !== "https") return "http";
  const authority = link.authority;
  if (authority.includes("@")) return "credentials";
  // An address with or without a port is an address first.
  const named = authority.replace(/:[^:]*$/, "");
  if (authority.startsWith("[") || /^[0-9.]+$/.test(named)) return "ip";
  if (authority.includes(":")) return "port";
  const host = hostOf(link);
  const labels = host.split(".");
  if (/[^\x20-\x7e]/.test(host) || labels.some((label) => label.startsWith("xn--"))) {
    return "lookalike";
  }
  if (
    host.length > 253 ||
    labels.length < 2 ||
    !labels.every((label) => LABEL.test(label)) ||
    !TLD.test(labels[labels.length - 1] ?? "")
  ) {
    return "form";
  }
  if ([...link.url].length > LINK_LIMITS.length) return "long";
  if (anyUnder(host, SHORTENERS)) return "shortener";
  if (anyUnder(host, GRABBERS)) return "grabber";
  if (ADULT.has(labels[labels.length - 1] ?? "")) return "adult";
  if (!trustedHost(host, lists) && lookalike(host)) return "lookalike";
  if (anyUnder(host, MESSENGERS) && PHONE_IN_LINK.test(decoded(link.rest))) return "phone";
  if (scammy(`${host} ${decoded(link.rest)}`)) return "scam";
  const path = decoded(link.rest.split(/[?#]/)[0] ?? "").toLowerCase();
  for (const segment of path.split("/")) {
    const dot = segment.lastIndexOf(".");
    if (dot >= 0 && FILES.has(segment.slice(dot + 1))) return "file";
  }
  if (!lists.invites && discordInvite(host, link.rest)) return "invite";
  if (lists.blocked.some((entry) => entryNames(entry, host, link.rest))) return "blocked";
  return null;
}

/** What the link rules say about a text's plain links (the text checks read the rest). */
export type LinksVerdict =
  | { ok: true; links: { url: string; host: string; trusted: boolean }[]; rest: string }
  | { ok: false; reason: "link"; rule: LinkRule }
  | { ok: false; reason: "phone" };

/** A cleaned text's plain links against the rules: at most three, each allowed, no scam beside. */
export function checkLinks(text: string, lists: LinkLists): LinksVerdict {
  const found = findLinks(text);
  if (found.length > LINK_LIMITS.links) return { ok: false, reason: "link", rule: "count" };
  const links: { url: string; host: string; trusted: boolean }[] = [];
  for (const link of found) {
    const problem = linkProblem(link, lists);
    if (problem === "phone") return { ok: false, reason: "phone" };
    if (problem !== null) return { ok: false, reason: "link", rule: problem };
    const host = hostOf(link);
    links.push({ url: link.url, host, trusted: trustedHost(host, lists) });
  }
  const rest = withoutLinks(text, found);
  if (found.length > 0 && scammy(rest)) return { ok: false, reason: "link", rule: "scam" };
  return { ok: true, links, rest };
}

/** A link of a message as the conversation draws it now. */
export interface MessageLink extends FoundLink {
  host: string;
  /** It may be pressed: it passes the rules and Studio's lists as they are now. */
  ok: boolean;
  /** A trusted site: it opens without the warning. */
  trusted: boolean;
  /** Why not, when it may not be pressed. */
  rule?: LinkRule;
}

/**
 * The links of a message's text against the rules as they are now (a site Studio blocks later
 * turns back into plain text in older messages too); `lists` null while links are off.
 */
export function messageLinks(text: string, lists: LinkLists | null): MessageLink[] {
  return findLinks(text).map((link, index) => {
    const host = hostOf(link);
    const problem =
      lists === null ? "off" : index >= LINK_LIMITS.links ? "count" : linkProblem(link, lists);
    const rule: LinkRule | undefined =
      problem === null ? undefined : problem === "phone" ? "form" : problem;
    return {
      ...link,
      host,
      ok: rule === undefined,
      trusted: rule === undefined && lists !== null && trustedHost(host, lists),
      ...(rule === undefined ? {} : { rule }),
    };
  });
}

// --- the whole message ---------------------------------------------------------------------------

/** Why a message's text may not go, as the service answers it (the word list aside). */
export type MessageProblem =
  | { reason: "long" }
  | { reason: "link"; rule: LinkRule }
  | { reason: "phone" }
  /** Only 🖕: the word list itself stays the service's. */
  | { reason: "words" };

/**
 * Why a message's text may not go, or null when it may (an empty text is simply not sent).
 * `links`: Studio's lists while links are on (CHAT2; `hello.chat.links`), null while they are off
 * or the service is older: then anything that reads as a link is `link` with rule `off`. With
 * links on, the plain links go by the rules first, the rest of the text (links blanked out) by
 * CHAT1's checks.
 */
export function messageProblem(text: string, links: LinkLists | null): MessageProblem | null {
  const clean = cleanChatText(text);
  if (!clean) return null;
  if ([...clean].length > CHAT_TEXT_MAX) return { reason: "long" };
  if (links === null) {
    const form = checkForm(clean);
    if (looksLikeLink(form)) return { reason: "link", rule: "off" };
    if (looksLikePhone(form)) return { reason: "phone" };
    return usesWords(clean) ? { reason: "words" } : null;
  }
  const verdict = checkLinks(clean, links);
  if (!verdict.ok) {
    return verdict.reason === "phone"
      ? { reason: "phone" }
      : { reason: "link", rule: verdict.rule };
  }
  const rest = checkForm(verdict.rest);
  if (looksLikeLink(rest)) return { reason: "link", rule: "form" };
  if (looksLikePhone(rest)) return { reason: "phone" };
  return usesWords(clean) ? { reason: "words" } : null;
}

/**
 * Why the text cannot go, or null when it may (an empty text is simply not sent). Without `links`
 * (CHAT1, or links off) every link is refused; with Studio's lists (CHAT2) plain links that pass
 * the rules may go. `messageProblem` says which rule.
 */
export function textProblem(text: string, links: LinkLists | null = null): TextProblem | null {
  const problem = messageProblem(text, links);
  return problem === null || problem.reason === "words" ? null : problem.reason;
}

/** The text has something to send. */
export function hasText(text: string): boolean {
  return (
    cleanChatText(text)
      .replaceAll(JOINER, "")
      .replace(/[\s\p{M}]/gu, "").length > 0
  );
}
