// The smart console's kinds (CAFK-FEAT, the contract's "Console kinds"): what a line is, the
// filter chips, "Adın geçenler" and the colours. A line from before console kinds (no `c`) shows
// as it always did, and only under "Hepsi".
import { matchesQuery } from "@pl/ui";

export interface KindLine {
  at: number;
  text: string;
  k: string;
  c?: string | null;
  who?: string | null;
  goto?: string | null;
  /** Console looks (CONSOLE-COLOR): the parts and events, checked by `@pl/ui/console`. */
  st?: unknown;
  ev?: unknown;
  /** A private line's direction (`in`, `out`) when the machine knows it. */
  dir?: string | null;
}

export type LineKind = "command" | "chat" | "pm" | "sys" | "title" | "cli" | "err" | "console";

/** A command row's line (`k: "s"`) whatever its `c`; else its `c` (an unknown one as `cli`). */
export function lineKind(line: KindLine): LineKind {
  if (line.c == null) return "console";
  if (line.k === "s") return "command";
  switch (line.c) {
    case "chat":
    case "pm":
    case "sys":
    case "title":
    case "err":
      return line.c;
    default:
      return "cli";
  }
}

export const KIND_FILTERS = ["all", "chat", "pm", "server", "mine", "link", "mentions"] as const;
export type KindFilter = (typeof KIND_FILTERS)[number];

const asciiLower = (text: string) => text.replace(/[A-Z]/g, (c) => c.toLowerCase());

/** A chat or private line that names the account (ASCII letter case aside), written by someone else. */
export function mentions(line: KindLine, name: string): boolean {
  const kind = lineKind(line);
  if (kind !== "chat" && kind !== "pm") return false;
  const me = asciiLower(name);
  if (!me || asciiLower(line.who ?? "") === me) return false;
  return asciiLower(line.text).includes(me);
}

export function matchesFilter(line: KindLine, filter: KindFilter, name: string): boolean {
  if (filter === "all") return true;
  const kind = lineKind(line);
  switch (filter) {
    case "chat":
      return kind === "chat";
    case "pm":
      return kind === "pm";
    case "server":
      return kind === "sys" || kind === "title";
    case "mine":
      return kind === "command";
    case "link":
      return kind === "cli" || kind === "err";
    case "mentions":
      return mentions(line, name);
  }
}

/** The lines a chip and the search keep, in order. */
export function visibleLines<T extends KindLine>(
  lines: T[],
  filter: KindFilter,
  name: string,
  query: string,
): T[] {
  if (filter === "all" && !query.trim()) return lines;
  return lines.filter(
    (line) =>
      matchesFilter(line, filter, name) && (!query.trim() || matchesQuery(line.text, query)),
  );
}

/** A line's text colour (docs/design.md tokens): chat bone, private gold, the server quieter. */
export function kindTone(line: KindLine): string {
  switch (lineKind(line)) {
    case "chat":
      return "text-bone-100";
    case "pm":
      return "text-gold-200";
    case "sys":
    case "title":
      return "text-ash-300";
    case "command":
      return "text-gold-300/80";
    case "cli":
      return "text-ash-400";
    case "err":
      return "text-flare-400";
    default:
      return line.k === "e"
        ? "text-flare-400"
        : line.k === "s"
          ? "text-gold-300/80"
          : "text-bone-100";
  }
}

/**
 * A private line's text split at the other player's name, so the name can stand out: the parts
 * before, the name, after (null when the line does not show the name).
 */
export function splitAtName(line: KindLine): [string, string, string] | null {
  if (lineKind(line) !== "pm" || !line.who) return null;
  const at = line.text.indexOf(line.who);
  if (at < 0) return null;
  return [line.text.slice(0, at), line.who, line.text.slice(at + line.who.length)];
}
