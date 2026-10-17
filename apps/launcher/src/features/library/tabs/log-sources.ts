// Which log the Logs tab shows: captured sessions and the game's own files share one list.
import type { GameLogSession } from "@/lib/api";
import i18n from "@/lib/i18n";
import { formatDay } from "../when";

/** Identity of a log in the list (a captured session and a game file never share it). */
export const logKey = (log: Pick<GameLogSession, "source" | "fileName">) =>
  `${log.source}:${log.fileName}`;

/** What one of the game's own log files is, read from its name (never shown as such). */
export type GameLogKind =
  | { kind: "latest" }
  | { kind: "debug" }
  | { kind: "dated"; session: number }
  | { kind: "debugArchive" }
  | { kind: "other" };

/**
 * `latest.log` (the last session), `debug.log` (its detailed log), `2026-10-15-2.log.gz` (that
 * day's second session), `debug-2.log.gz` (an older detailed log), anything else a mod wrote.
 */
export function gameLogKind(fileName: string): GameLogKind {
  const name = fileName.toLowerCase();
  if (name === "latest.log") return { kind: "latest" };
  if (name === "debug.log") return { kind: "debug" };
  const dated = /^\d{4}-\d{2}-\d{2}-(\d+)\.log(?:\.gz)?$/.exec(name);
  if (dated) return { kind: "dated", session: Number(dated[1]) };
  if (/^debug-\d+\.log(?:\.gz)?$/.test(name)) return { kind: "debugArchive" };
  return { kind: "other" };
}

/** What the list says a game file is: "Son oturum", "2. oturum", "Ayrıntılı kayıt". */
export function gameLogKindLabel(fileName: string): string {
  const what = gameLogKind(fileName);
  return what.kind === "dated"
    ? i18n.t("instance.logs.kinds.dated", { n: what.session })
    : i18n.t(`instance.logs.kinds.${what.kind}`);
}

/** The viewer's title for a game file: "15 Ekim, 2. oturum", "Son oturum". */
export function gameLogTitle(file: Pick<GameLogSession, "fileName" | "startedAt">): string {
  const what = gameLogKind(file.fileName);
  if (what.kind === "dated" && file.startedAt) {
    return i18n.t("instance.logs.datedTitle", { day: formatDay(file.startedAt), n: what.session });
  }
  return gameLogKindLabel(file.fileName);
}

/**
 * The log to show: the player's pick while it exists, else the captured session asked for by
 * name (the crash dialog's "show log"), else the first one (the newest captured session, or the
 * game's newest file when nothing was captured).
 */
export function pickLog(
  logs: GameLogSession[],
  picked: string | null,
  wanted?: string,
): GameLogSession | undefined {
  const chosen = picked ? logs.find((log) => logKey(log) === picked) : undefined;
  const asked = wanted
    ? logs.find((log) => log.source === "launcher" && log.fileName === wanted)
    : undefined;
  return chosen ?? asked ?? logs[0];
}
