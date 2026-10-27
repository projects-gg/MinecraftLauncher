// Placeholders in auto-replies (CAFK-FEAT, the contract's "Rule placeholders"): `{u}` the sender's
// name and `{1}`…`{9}` the pattern's capturing groups, in a rule's plain reply. These checks only
// answer while typing; the backend checks again on save (pl_cloud_afk::placeholder_problem, with
// Rust's own group count) and the service has the last word. Shared cases:
// crates/pl-cloud-afk/tests/vectors/rule-placeholder-v1.json.
import { withoutLeadingIgnorable } from "./sensitive";

const PLACEHOLDER = /\{(u|[1-9])\}/g;
const STARTS_WITH_PLACEHOLDER = /^\{(?:u|[1-9])\}/;
const HAS_PLACEHOLDER = /\{(?:u|[1-9])\}/;

/** The longest value the client lets through, in UTF-16 code units. */
export const VALUE_MAX = 24;
const MAX_TEXT = 256;

export type PlaceholderReason =
  | "placeholder_row"
  | "placeholder_part"
  | "placeholder_sender"
  | "placeholder_start"
  | "placeholder_command"
  | "placeholder_length";

export interface PlaceholderProblem {
  reason: PlaceholderReason;
  part?: number;
}

/** The placeholder names of a text, left to right, repeats included: ["u", "1", "u"]. */
export function placeholderNames(text: string): string[] {
  return [...text.matchAll(PLACEHOLDER)].map((m) => m[1] ?? "");
}

export function hasPlaceholder(text: string): boolean {
  return HAS_PLACEHOLDER.test(text);
}

/** Index just after the bracketed class that starts at `chars[start]` (`[`), Rust regex syntax. */
function skipClass(chars: string[], start: number): number {
  let i = start + 1;
  if (chars[i] === "^") i++;
  if (chars[i] === "]") i++;
  while (i < chars.length) {
    const c = chars[i];
    if (c === "\\") i += 2;
    else if (c === "[") i = skipClass(chars, i);
    else if (c === "]") return i + 1;
    else i++;
  }
  return i;
}

/**
 * The capturing groups of a pattern (Rust syntax), numbered by their opening parenthesis, named
 * or not: `(…)`, `(?<name>…)`, `(?P<name>…)`; never `(?:…)`, flags, escaped parentheses or ones
 * inside a class.
 */
export function captureCount(pattern: string): number {
  const chars = [...pattern.trim()];
  let count = 0;
  let i = 0;
  while (i < chars.length) {
    const c = chars[i];
    if (c === "\\") {
      i += 2;
      continue;
    }
    if (c === "[") {
      i = skipClass(chars, i);
      continue;
    }
    if (c === "(") {
      const named = chars[i + 2] === "<" || (chars[i + 2] === "P" && chars[i + 3] === "<");
      if (chars[i + 1] !== "?" || named) count++;
    }
    i++;
  }
  return count;
}

export type PlaceholderRow =
  | { kind: "command"; text: string }
  | { kind: "console"; text: string }
  | { kind: "reply"; text: string; rule: { match: string; regex: boolean; from: string } };

/**
 * Why a plain row breaks the placeholder rules, or null (after the text rule and the password
 * commands, in the contract's order). `commands`: Studio's allow list.
 */
export function placeholderProblem(
  row: PlaceholderRow,
  commands: readonly string[],
): PlaceholderProblem | null {
  const names = placeholderNames(row.text);
  if (names.length === 0 || row.kind === "console") return null;
  if (row.kind === "command") return { reason: "placeholder_row" };
  const groups = row.rule.regex ? captureCount(row.rule.match) : 0;
  for (const name of names) {
    if (name !== "u" && Number(name) > groups) {
      return { reason: "placeholder_part", part: Number(name) };
    }
  }
  if (row.rule.from === "server" && names.includes("u")) return { reason: "placeholder_sender" };
  const lead = withoutLeadingIgnorable(row.text);
  if (STARTS_WITH_PLACEHOLDER.test(lead)) return { reason: "placeholder_start" };
  if (lead.startsWith("/")) {
    const word = lead.split(" ", 1)[0] ?? "";
    const command = word.slice(1).replace(/[A-Z]/g, (c) => c.toLowerCase());
    if (HAS_PLACEHOLDER.test(word) || !commands.includes(command)) {
      return { reason: "placeholder_command" };
    }
  }
  if (row.text.length + (VALUE_MAX - 3) * names.length > MAX_TEXT) {
    return { reason: "placeholder_length" };
  }
  return null;
}

/**
 * `token` typed into `text` where the caret is (`start`…`end` replaced), and where the caret goes
 * after it. A space is added where the token would touch a word, so "Selam{u}" reads "Selam {u}".
 */
export function insertToken(
  text: string,
  start: number,
  end: number,
  token: string,
): { text: string; caret: number } {
  const before = text.slice(0, start);
  const after = text.slice(end);
  const lead = before !== "" && !/\s$/.test(before) ? " " : "";
  const trail = after !== "" && !/^[\s.,!?;:]/.test(after) ? " " : "";
  const inserted = `${lead}${token}${trail}`;
  return {
    text: `${before}${inserted}${after}`,
    caret: before.length + lead.length + token.length,
  };
}

/** The placeholder chips a rule offers: the sender (not for server lines) and its pattern's parts. */
export function placeholderChips(rule: {
  pattern: string;
  regex: boolean;
  from: string;
}): string[] {
  const parts = rule.regex ? Math.min(captureCount(rule.pattern), 9) : 0;
  const chips = rule.from === "server" ? [] : ["u"];
  for (let n = 1; n <= parts; n++) chips.push(String(n));
  return chips;
}
