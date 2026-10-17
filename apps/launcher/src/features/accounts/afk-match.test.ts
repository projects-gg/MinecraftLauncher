import { describe, expect, it } from "vitest";
import type { AfkAccount, AfkState } from "@/lib/api";
import {
  afkSummary,
  findAfkSession,
  isAfkOnline,
  isAfkRunning,
  onlineAfkSession,
  sameName,
} from "./afk-match";

const session = (id: string, nickname: string, state: AfkState): AfkAccount => ({
  id,
  nickname,
  server: "play.projects.gg",
  autoStart: false,
  mcVersion: "26.3",
  mcVersionRecommended: true,
  state,
});

const sessions: AfkAccount[] = [
  session("1", "KasaBekcisi", { kind: "connected", since: "2026-10-16T10:00:00Z" }),
  session("2", "TarlaBot_1", {
    kind: "reconnecting",
    attempt: 2,
    nextAttemptAt: "2026-10-16T10:00:00Z",
  }),
  session("3", "Madenci07", { kind: "stopped" }),
  session("4", "Uykucu", { kind: "error", reason: "kicked" }),
  session("5", "Sirada", { kind: "queued", inSecs: 15 }),
];

describe("AFK name matching", () => {
  it("compares names like the server does", () => {
    expect(sameName("KasaBekcisi", "kasabekcisi")).toBe(true);
    expect(sameName(" Steve", "steve ")).toBe(true);
    expect(sameName("Steve", "Steve2")).toBe(false);
  });

  it("knows which states hold the name", () => {
    const running = (state: AfkState) => isAfkRunning(state);
    expect(running({ kind: "connected", since: "" })).toBe(true);
    expect(running({ kind: "connecting" })).toBe(true);
    expect(running({ kind: "starting" })).toBe(true);
    expect(running({ kind: "queued", inSecs: 3 })).toBe(true);
    expect(running({ kind: "reconnecting", attempt: 1, nextAttemptAt: "" })).toBe(true);
    expect(running({ kind: "stopping" })).toBe(false);
    expect(running({ kind: "stopped" })).toBe(false);
    expect(running({ kind: "error", reason: "" })).toBe(false);
    expect(isAfkOnline({ kind: "connecting" })).toBe(false);
  });

  it("finds the session that would clash with a launch", () => {
    expect(findAfkSession("kasabekcisi", sessions)?.id).toBe("1");
    expect(findAfkSession("TARLABOT_1", sessions)?.id).toBe("2");
    expect(findAfkSession("sirada", sessions)?.id).toBe("5");
    expect(findAfkSession("Madenci07", sessions)).toBeNull();
    expect(findAfkSession("Uykucu", sessions)).toBeNull();
    expect(findAfkSession("Baskasi", sessions)).toBeNull();
  });

  it("shows the chip only while the session is online", () => {
    expect(onlineAfkSession("KASABEKCISI", sessions)?.id).toBe("1");
    expect(onlineAfkSession("TarlaBot_1", sessions)).toBeNull();
    expect(onlineAfkSession("Madenci07", sessions)).toBeNull();
  });

  it("summarizes the AFK accounts", () => {
    expect(afkSummary(sessions)).toEqual({ total: 5, online: 1 });
    expect(afkSummary([])).toEqual({ total: 0, online: 0 });
  });
});
