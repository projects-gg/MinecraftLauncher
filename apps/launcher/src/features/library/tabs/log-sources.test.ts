import { describe, expect, it } from "vitest";
import type { GameLogSession } from "@/lib/api";
import { formatDay, logFileWhen } from "../when";
import { gameLogKind, logKey, pickLog } from "./log-sources";

const log = (fileName: string, source: GameLogSession["source"] = "launcher"): GameLogSession => ({
  fileName,
  source,
  sizeBytes: 100,
  live: false,
});

const logs = [
  log("game-projects-20261016-120000.log"),
  log("game-projects-20261015-090000.log"),
  log("latest.log", "game"),
  log("2026-10-15-1.log.gz", "game"),
];

describe("pickLog", () => {
  it("shows the newest captured session by default", () => {
    expect(pickLog(logs, null)?.fileName).toBe("game-projects-20261016-120000.log");
    expect(pickLog([], null)).toBeUndefined();
  });

  it("opens the session the crash dialog asks for", () => {
    expect(pickLog(logs, null, "game-projects-20261015-090000.log")?.fileName).toBe(
      "game-projects-20261015-090000.log",
    );
    // A name that is not a captured session falls back to the newest.
    expect(pickLog(logs, null, "latest.log")?.source).toBe("launcher");
    expect(pickLog(logs, null, "gone.log")?.fileName).toBe("game-projects-20261016-120000.log");
  });

  it("keeps the player's pick while it exists", () => {
    const pick = logKey(logs[3] as GameLogSession);
    expect(pickLog(logs, pick, "game-projects-20261015-090000.log")?.fileName).toBe(
      "2026-10-15-1.log.gz",
    );
    expect(pickLog(logs.slice(0, 3), pick)?.fileName).toBe("game-projects-20261016-120000.log");
  });

  it("falls back to the game's files when nothing was captured", () => {
    expect(pickLog(logs.slice(2), null)?.fileName).toBe("latest.log");
    expect(logKey(logs[2] as GameLogSession)).toBe("game:latest.log");
  });
});

describe("log file dates", () => {
  const now = new Date(2026, 9, 16, 21, 0);

  it("names an archive's day and a live file's last write", () => {
    expect(formatDay(new Date(2026, 9, 16).toISOString(), now)).toBe("Bugün");
    expect(formatDay(new Date(2026, 9, 15).toISOString(), now)).toBe("Dün");
    expect(formatDay(new Date(2026, 9, 3).toISOString(), now)).toBe("3 Ekim");
    expect(formatDay(new Date(2025, 2, 3).toISOString(), now)).toBe("3 Mart 2025");
    const archive = {
      ...log("2026-10-03-2.log.gz", "game"),
      startedAt: new Date(2026, 9, 3).toISOString(),
    };
    expect(logFileWhen(archive, now)).toBe("3 Ekim");
    const latest = {
      ...log("latest.log", "game"),
      updatedAt: new Date(2026, 9, 16, 20, 5).toISOString(),
    };
    expect(logFileWhen(latest, now)).toBe("Bugün 20:05");
  });
});

describe("gameLogKind", () => {
  it("reads what each of the game's files is", () => {
    expect(gameLogKind("latest.log")).toEqual({ kind: "latest" });
    expect(gameLogKind("debug.log")).toEqual({ kind: "debug" });
    expect(gameLogKind("2026-10-15-2.log.gz")).toEqual({ kind: "dated", session: 2 });
    expect(gameLogKind("debug-3.log.gz")).toEqual({ kind: "debugArchive" });
    expect(gameLogKind("fml-client-latest.log")).toEqual({ kind: "other" });
  });
});
