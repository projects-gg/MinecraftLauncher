// A console line's looks (CONSOLE-COLOR): the machine's parts (`st`) and events (`ev`) over the
// line's plain text, checked again here, since the view is the last guard. Parts carry only the
// game's colours and flags, never markup. A click is one of four kinds, with a value the contract
// allows, and nothing happens without the player. `text` stays the line: search, copy and
// everything else read it, never these.
import { codeLook, STYLE_CODE, type TextRun, type TooltipLine } from "../inventory/text";

/** `[n, code]`, or `[n, code, e]` with the index of its event: n UTF-16 code units of the text. */
export type ConsoleSpan = readonly [number, string] | readonly [number, string, number];

export type ConsoleClickAction =
  | "run_command"
  | "suggest_command"
  | "open_url"
  | "copy_to_clipboard";

export interface ConsoleClick {
  readonly a: ConsoleClickAction;
  readonly v: string;
}

export interface ConsoleHoverText {
  readonly text: string;
  readonly st?: readonly ConsoleSpan[] | null;
}

/** The item as the machine gives it (the inventory's item); the host maps it. */
export interface ConsoleHoverItem {
  readonly item: unknown;
}

export interface ConsoleHoverEntity {
  readonly entity: {
    readonly label?: { readonly tr?: string; readonly en?: string } | null;
    readonly name?: string | null;
    readonly st?: readonly ConsoleSpan[] | null;
  };
}

export type ConsoleHover = ConsoleHoverText | ConsoleHoverItem | ConsoleHoverEntity;

export interface ConsoleEvent {
  readonly click?: ConsoleClick;
  readonly hover?: ConsoleHover;
}

/** One part as the console draws it: its words, colour (none: the line's own) and flags. */
export interface ConsoleRun {
  text: string;
  color?: string;
  bold?: boolean;
  italic?: boolean;
  underlined?: boolean;
  strikethrough?: boolean;
  /** Its event, an index into the checked events. */
  event?: number;
}

export const MAX_SPANS = 64;
export const MAX_EVENTS = 8;
export const MAX_VALUE = 256;
export const MAX_HOVER = 256;
export const MAX_HOVER_SPANS = 16;
export const MAX_HOVER_LINES = 8;
export const MAX_NAME = 64;

const ACTIONS: ReadonlySet<string> = new Set([
  "run_command",
  "suggest_command",
  "open_url",
  "copy_to_clipboard",
]);

/**
 * The characters no click value, card text or hover carries (the contract's one list, the same in
 * MCC, the machine, the Worker and here): controls (C0, DEL, C1), the soft hyphen, the combining
 * grapheme joiner, the Arabic letter mark, the Hangul and Mongolian fillers, the zero-width spaces,
 * joiners and direction marks (U+200B–U+200F), the separators, embeddings and overrides
 * (U+2028–U+202E), the invisible operators (U+2060–U+2064), the isolates, U+FEFF and the
 * halfwidth Hangul filler. Tag characters (U+E0000–U+E007F) are checked as their surrogate pair.
 */
function forbidden(unit: number): boolean {
  return (
    unit <= 0x1f ||
    (unit >= 0x7f && unit <= 0x9f) ||
    unit === 0xad ||
    unit === 0x34f ||
    unit === 0x61c ||
    unit === 0x115f ||
    unit === 0x1160 ||
    unit === 0x180e ||
    (unit >= 0x200b && unit <= 0x200f) ||
    (unit >= 0x2028 && unit <= 0x202e) ||
    (unit >= 0x2060 && unit <= 0x2064) ||
    (unit >= 0x2066 && unit <= 0x2069) ||
    unit === 0x3164 ||
    unit === 0xfeff ||
    unit === 0xffa0
  );
}

/** The text holds none of the forbidden characters (`newlines`: a hover's `\n` may stay). */
export function clean(text: string, newlines: boolean): boolean {
  for (let i = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (newlines && unit === 0x0a) continue;
    if (forbidden(unit)) return false;
    // A tag character (U+E0000–U+E007F): U+DB40 and a low half from U+DC00 to U+DC7F.
    const next = text.charCodeAt(i + 1);
    if (unit === 0xdb40 && next >= 0xdc00 && next <= 0xdc7f) return false;
  }
  return true;
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const LABEL = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/i;

/**
 * An address the console may open, cut at its host: `https://`, no white space and none of the
 * forbidden characters, no sign-in before the host (`@`), a port of digits only, and a host of at
 * least two plain ASCII labels: no IDN (Unicode or `xn--` punycode, look-alike letters), no IP
 * address (v4 or v6), no `localhost`. Null when it is not one.
 */
export function urlParts(value: string): { host: string; rest: string } | null {
  if (!value.startsWith("https://") || /[\s\u0085]/.test(value) || !clean(value, false))
    return null;
  const tail = value.slice("https://".length);
  const end = tail.search(/[/?#]/);
  const authority = end === -1 ? tail : tail.slice(0, end);
  if (authority.includes("@") || authority.startsWith("[")) return null;
  const colon = authority.indexOf(":");
  const host = colon === -1 ? authority : authority.slice(0, colon);
  const port = colon === -1 ? null : authority.slice(colon + 1);
  if (port !== null && !/^\d{1,5}$/.test(port)) return null;
  const labels = host.split(".");
  if (labels.length < 2 || !labels.every((label) => LABEL.test(label))) return null;
  if (labels.some((label) => label.toLowerCase().startsWith("xn--"))) return null;
  if (/^\d+$/.test(labels[labels.length - 1] ?? "")) return null;
  const lower = host.toLowerCase();
  if (lower === "localhost" || lower.endsWith(".localhost")) return null;
  try {
    // The browser must read the same host: no other parser's surprise.
    const url = new URL(value);
    if (url.protocol !== "https:" || url.hostname !== lower || url.username || url.password) {
      return null;
    }
  } catch {
    return null;
  }
  return { host: authority, rest: end === -1 ? "" : tail.slice(end) };
}

/** A link the console may offer (`urlParts`). */
export function safeUrl(value: string): boolean {
  return urlParts(value) !== null;
}

/** What a copy's toast shows of the copied text: at most 60 units, then "…" (never half a character). */
export function copiedPreview(value: string): string {
  if (value.length <= 60) return value;
  let cut = 59;
  const unit = value.charCodeAt(cut - 1);
  if (unit >= 0xd800 && unit <= 0xdbff) cut -= 1;
  return `${value.slice(0, cut)}…`;
}

/** A click as the contract allows it, else null: a value is never cut, an odd one never shown. */
export function safeClick(value: unknown): ConsoleClick | null {
  if (!isObject(value)) return null;
  const { a, v } = value;
  if (typeof a !== "string" || !ACTIONS.has(a) || typeof v !== "string") return null;
  if (v.length < 1 || v.length > MAX_VALUE || !clean(v, false)) return null;
  if (a === "run_command" && (v.length < 2 || !v.startsWith("/"))) return null;
  if (a === "open_url" && !safeUrl(v)) return null;
  return { a: a as ConsoleClickAction, v };
}

/**
 * The parts of `text` as `st` gives them, or null when they are not the contract's (a code
 * outside it, more than `max` parts, a part between a character's halves, not covering the text
 * exactly). A part's event stays only when `events` has it.
 */
function partsOf(
  text: string,
  st: unknown,
  max: number,
  events: readonly (ConsoleEvent | null)[],
): ConsoleRun[] | null {
  if (!Array.isArray(st) || st.length === 0 || st.length > max) return null;
  const runs: ConsoleRun[] = [];
  let at = 0;
  for (const span of st) {
    if (!Array.isArray(span) || (span.length !== 2 && span.length !== 3)) return null;
    const [n, code, e] = span as unknown[];
    if (!Number.isInteger(n) || (n as number) < 1 || typeof code !== "string") return null;
    if (code.length > 12 || !STYLE_CODE.test(code)) return null;
    const end = at + (n as number);
    if (end > text.length) return null;
    const first = text.charCodeAt(at);
    const last = text.charCodeAt(end - 1);
    if ((first >= 0xdc00 && first <= 0xdfff) || (last >= 0xd800 && last <= 0xdbff)) return null;
    const { obfuscated: _still, ...look } = codeLook(code);
    const run: ConsoleRun = { text: text.slice(at, end), ...look };
    if (
      span.length === 3 &&
      Number.isInteger(e) &&
      (e as number) >= 0 &&
      (e as number) < events.length &&
      events[e as number]
    ) {
      run.event = e as number;
    }
    const tail = runs[runs.length - 1];
    if (tail && sameRun(tail, run)) tail.text += run.text;
    else runs.push(run);
    at = end;
  }
  return at === text.length ? runs : null;
}

const sameRun = (a: ConsoleRun, b: ConsoleRun) =>
  a.color === b.color &&
  Boolean(a.bold) === Boolean(b.bold) &&
  Boolean(a.italic) === Boolean(b.italic) &&
  Boolean(a.underlined) === Boolean(b.underlined) &&
  Boolean(a.strikethrough) === Boolean(b.strikethrough) &&
  a.event === b.event;

/** A hover as the contract allows it, else null (depth 1: its parts carry no events). */
export function safeHover(value: unknown): ConsoleHover | null {
  if (!isObject(value)) return null;
  if (typeof value.text === "string") {
    const { text } = value;
    if (text.length === 0 || text.length > MAX_HOVER || !clean(text, true)) return null;
    if (text.split("\n").length > MAX_HOVER_LINES) return null;
    const st = partsOf(text, value.st, MAX_HOVER_SPANS, []) ? (value.st as ConsoleSpan[]) : null;
    return { text, st };
  }
  if (isObject(value.item)) return { item: value.item };
  if (isObject(value.entity)) {
    const { label, name, st } = value.entity;
    const word = (v: unknown) =>
      typeof v === "string" && v.length > 0 && v.length <= MAX_NAME && clean(v, false)
        ? v
        : undefined;
    const tr = isObject(label) ? word(label.tr) : undefined;
    const en = isObject(label) ? word(label.en) : undefined;
    const own = word(name);
    if (!tr && !en && !own) return null;
    return {
      entity: {
        label: tr || en ? { tr, en } : null,
        name: own ?? null,
        st: own && partsOf(own, st, MAX_HOVER_SPANS, []) ? (st as ConsoleSpan[]) : null,
      },
    };
  }
  return null;
}

/**
 * The line's events, each checked; one that is not the contract's is null in its place, so the
 * parts' indexes still point where the machine meant.
 */
export function safeEvents(ev: unknown): (ConsoleEvent | null)[] {
  if (!Array.isArray(ev) || ev.length > MAX_EVENTS) return [];
  return ev.map((value) => {
    if (!isObject(value)) return null;
    const click = value.click === undefined ? null : safeClick(value.click);
    const hover = value.hover === undefined ? null : safeHover(value.hover);
    if (!click && !hover) return null;
    const event: { click?: ConsoleClick; hover?: ConsoleHover } = {};
    if (click) event.click = click;
    if (hover) event.hover = hover;
    return event;
  });
}

/**
 * The line in its parts, or null when it has none the contract allows: the line then shows as
 * today. The parts' words joined are `text` exactly (a copy is the line); obfuscated words stay
 * as they are, still.
 */
export function lineRuns(
  text: string,
  st: unknown,
  events: readonly (ConsoleEvent | null)[] = [],
): ConsoleRun[] | null {
  return partsOf(text, st, MAX_SPANS, events);
}

/** White: the game's hover text without a colour of its own. */
const HOVER_WHITE = "#ffffff";

function tooltipRuns(runs: readonly ConsoleRun[]): TextRun[] {
  return runs.map(({ event: _event, ...run }) => ({ ...run, color: run.color ?? HOVER_WHITE }));
}

/** A text hover's lines as the game's tooltip draws them (its line breaks kept, its looks). */
export function hoverTextLines(hover: ConsoleHoverText): TooltipLine[] {
  const runs = hover.st ? partsOf(hover.text, hover.st, MAX_HOVER_SPANS, []) : null;
  const lines: TooltipLine[] = [];
  let current: ConsoleRun[] = [];
  const flush = () => {
    const text = current.map((run) => run.text).join("");
    lines.push(
      current.length && runs
        ? { text, color: HOVER_WHITE, runs: tooltipRuns(current) }
        : { text, color: HOVER_WHITE },
    );
    current = [];
  };
  for (const run of runs ?? [{ text: hover.text }]) {
    const pieces = run.text.split("\n");
    pieces.forEach((piece, i) => {
      if (i > 0) flush();
      if (piece) current.push({ ...run, text: piece });
    });
  }
  flush();
  return lines;
}

/** An entity hover's lines: its name (in its looks), then its type, as the game shows them. */
export function hoverEntityLines(
  hover: ConsoleHoverEntity,
  lang: "tr" | "en",
  typeWord: string,
): TooltipLine[] {
  const { label, name, st } = hover.entity;
  const lines: TooltipLine[] = [];
  if (name) {
    const runs = st ? partsOf(name, st, MAX_HOVER_SPANS, []) : null;
    lines.push(
      runs
        ? { text: name, color: HOVER_WHITE, runs: tooltipRuns(runs) }
        : { text: name, color: HOVER_WHITE },
    );
  }
  const type = label ? (label[lang] ?? label.tr ?? label.en) : undefined;
  if (type) lines.push({ text: `${typeWord}: ${type}`, color: HOVER_WHITE });
  return lines;
}
