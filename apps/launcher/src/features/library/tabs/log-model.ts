// Log viewer model: grouping live lines, flattening entries into fixed-height rows and finding
// search matches. Mirrors the parsing rules of pl-mc `gamefiles::logs`. Lines arrive masked:
// pl-mc masks the game's output at the source (`launch::process`) and saved logs when read.
import type { LogLevel, LogLine } from "@/lib/api";

export const levelRank: Record<LogLevel, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
  fatal: 5,
};

/** The level chips of the viewer (fatal counts as an error, trace as detail). */
export type LevelGroup = "error" | "warn" | "info" | "debug";
export const levelGroups: LevelGroup[] = ["error", "warn", "info", "debug"];

export function levelGroup(level: LogLevel): LevelGroup {
  if (level === "fatal" || level === "error") return "error";
  if (level === "warn" || level === "info") return level;
  return "debug";
}

const exceptionHeader =
  /^(?:[A-Za-z_$][\w$]*\.)+[A-Za-z_$][\w$]*(?:Exception|Error|Throwable)(?::|$)/;

/** A line that belongs to the entry above it (stack frames, mod lists, exception headers). */
export function isContinuation(text: string): boolean {
  return (
    /^[ \t]/.test(text) ||
    text.startsWith("Caused by: ") ||
    text.startsWith("Suppressed: ") ||
    text.startsWith("... ") ||
    exceptionHeader.test(text)
  );
}

/**
 * Append a live line. A plain continuation line joins the entry above (a stack trace under an
 * informational entry makes it an error), like the backend does for saved logs.
 */
export function appendLine(entries: LogLine[], line: LogLine): void {
  const last = entries[entries.length - 1];
  if (last && !line.time && !line.thread && isContinuation(line.message)) {
    const level =
      levelRank[last.level] < levelRank.warn && levelRank[line.level] >= levelRank.error
        ? "error"
        : last.level;
    entries[entries.length - 1] = { ...last, level, message: `${last.message}\n${line.message}` };
    return;
  }
  entries.push(line);
}

/**
 * Live lines that arrived while the saved log was being read may already be in it: drop the
 * longest run at the start of `pending` that repeats the end of `base`.
 */
export function withoutOverlap(base: LogLine[], pending: LogLine[]): LogLine[] {
  const key = (l: LogLine) => `${l.time ?? ""}|${l.level}|${l.message.split("\n", 1)[0]}`;
  const max = Math.min(pending.length, base.length);
  for (let k = max; k > 0; k--) {
    let same = true;
    for (let i = 0; i < k && same; i++) {
      same = key(pending[i] as LogLine) === key(base[base.length - k + i] as LogLine);
    }
    if (same) return pending.slice(k);
  }
  return pending;
}

/** `[time] [thread/LEVEL] [logger]: ` (the launcher's saved format). */
export function headPrefix(line: LogLine): string {
  const level = line.level.toUpperCase();
  if (!line.thread) return line.time ? `[${line.time}] [${level}]: ` : "";
  let out = line.time ? `[${line.time}] ` : "";
  out += `[${line.thread}/${level}]`;
  if (line.logger) out += ` [${line.logger}]`;
  return `${out}: `;
}

export const formatLine = (line: LogLine) => headPrefix(line) + line.message;

/** One fixed-height row of the viewer: an entry's first line, or a following message line. */
export interface LogRow {
  entry: number;
  head: boolean;
  text: string;
  /** Where the message starts (after the prefix) in `text`. */
  messageAt: number;
  level: LogLevel;
}

export function buildRows(entries: LogLine[], visible: (line: LogLine) => boolean): LogRow[] {
  const rows: LogRow[] = [];
  for (let entry = 0; entry < entries.length; entry++) {
    const line = entries[entry] as LogLine;
    if (!visible(line)) continue;
    const prefix = headPrefix(line);
    const parts = line.message.split("\n");
    for (let i = 0; i < parts.length; i++) {
      const part = (parts[i] as string).replace(/\r$/, "");
      rows.push(
        i === 0
          ? { entry, head: true, text: prefix + part, messageAt: prefix.length, level: line.level }
          : { entry, head: false, text: part, messageAt: 0, level: line.level },
      );
    }
  }
  return rows;
}

/** Search hits (case-insensitive), at most `limit`. */
export interface Match {
  row: number;
  start: number;
}

export function findMatches(rows: LogRow[], query: string, limit = 5000): Match[] {
  const needle = query.toLowerCase();
  const matches: Match[] = [];
  if (!needle) return matches;
  for (let row = 0; row < rows.length && matches.length < limit; row++) {
    const text = (rows[row] as LogRow).text.toLowerCase();
    let at = text.indexOf(needle);
    while (at >= 0 && matches.length < limit) {
      matches.push({ row, start: at });
      at = text.indexOf(needle, at + needle.length);
    }
  }
  return matches;
}

/** A styled piece of a row. */
export interface RowPart {
  text: string;
  tone: "dim" | "level" | "message";
  hit?: "hit" | "current";
}

/** Split a row into prefix/level/message parts, cut further at search hits. */
export function rowParts(
  row: LogRow,
  hits: number[] | undefined,
  hitLength: number,
  current: number | undefined,
  maxChars: number,
): RowPart[] {
  const text = row.text.length > maxChars ? `${row.text.slice(0, maxChars)}…` : row.text;
  const segments: Array<[number, number, RowPart["tone"]]> = [];
  if (row.head && row.messageAt > 0) {
    const token = `${row.level.toUpperCase()}]`;
    const levelAt = text.lastIndexOf(token, row.messageAt);
    if (levelAt > 0) {
      segments.push([0, levelAt, "dim"], [levelAt, levelAt + token.length - 1, "level"]);
      segments.push([levelAt + token.length - 1, row.messageAt, "dim"]);
    } else {
      segments.push([0, row.messageAt, "dim"]);
    }
  }
  segments.push([row.messageAt, text.length, "message"]);

  const parts: RowPart[] = [];
  for (const [segStart, segEnd, tone] of segments) {
    let pos = segStart;
    for (const start of hits ?? []) {
      const end = start + hitLength;
      if (end <= pos || start >= segEnd) continue;
      const a = Math.max(start, pos);
      const b = Math.min(end, segEnd);
      if (a > pos) parts.push({ text: text.slice(pos, a), tone });
      parts.push({ text: text.slice(a, b), tone, hit: start === current ? "current" : "hit" });
      pos = b;
    }
    if (pos < segEnd) parts.push({ text: text.slice(pos, segEnd), tone });
  }
  return parts;
}
