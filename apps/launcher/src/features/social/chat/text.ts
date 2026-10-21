// What the composer checks while the player types (CHAT1), the same way the friends service checks
// a message (services/social/src/chattext.ts): links and phone numbers never go, a message is at
// most 300 characters and 5 lines. The service checks again (and also its word list, which it never
// names); this only says why before anything is sent. Mirror changes there.

/** Characters in a message (code points, after cleaning). */
export const CHAT_TEXT_MAX = 300;
export const CHAT_LINES_MAX = 5;
/** The counter shows from here on. */
export const CHAT_COUNT_FROM = 250;

/** Why a text cannot go as it is, before the service is asked. */
export type TextProblem = "link" | "phone" | "long";

const JOINER = String.fromCharCode(0x200d);
const INVISIBLE = /[\p{Cc}\p{Cf}\p{Co}\p{Cs}\u115F\u1160\u3164\uFFA0\u2800]/gu;
const PICTOGRAPH = /^\p{Extended_Pictographic}$/u;
const EMOJI_TAIL = /^[\uFE0F\u{1F3FB}-\u{1F3FF}]$/u;

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
 * emoji), at most two marks on a letter, spaces collapsed, empty lines dropped, at most five lines
 * (more join the fifth), trimmed.
 */
export function cleanChatText(value: string): string {
  const text = keepEmojiJoiners(
    value
      .normalize("NFC")
      .replace(/\r\n?|[\u0085\u2028\u2029]/gu, "\n")
      .replace(/[\t\v\f]/g, " ")
      .replace(INVISIBLE, (char) => (char === "\n" || char === JOINER ? char : "")),
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

/** The plain form the checks read (look-alike letters as Latin, other digits as ASCII). */
function checkForm(text: string): string {
  let form = "";
  const lower = text.normalize("NFKC").replaceAll("İ", "i").replaceAll("I", "ı").toLowerCase();
  for (const char of lower.normalize("NFKD")) {
    if (/[\p{M}\p{Cf}]/u.test(char)) continue;
    const code = char.codePointAt(0) ?? 0;
    const zero = DIGIT_ZEROS.find((start) => code >= start && code < start + 10);
    form += zero === undefined ? (LOOK_ALIKES[char] ?? char) : String(code - zero);
  }
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

const LABEL = "[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?";
const DOMAIN = new RegExp(`(?:^|[^a-z0-9-])((?:${LABEL}\\.)+([a-z]{2,24}))(?![a-z0-9-])`, "g");
const IPV4 = /(?:^|[^0-9.])(?:\d{1,3}\.){3}\d{1,3}(?![0-9.]*\d)/;
const OWN_DOMAIN = "projects.gg";

function linkForm(form: string): string {
  return form
    .replaceAll("ı", "i")
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

const NUMBER = /\+?\d(?:[\s.\-/()_,]{0,3}\d)+/g;

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

/** Why the text cannot go, or null when it may (an empty text is simply not sent). */
export function textProblem(text: string): TextProblem | null {
  const clean = cleanChatText(text);
  if (!clean) return null;
  if ([...clean].length > CHAT_TEXT_MAX) return "long";
  if (looksLikeLink(clean)) return "link";
  if (looksLikePhone(clean)) return "phone";
  return null;
}

/** The text has something to send. */
export function hasText(text: string): boolean {
  return (
    cleanChatText(text)
      .replaceAll(JOINER, "")
      .replace(/[\s\p{M}]/gu, "").length > 0
  );
}
