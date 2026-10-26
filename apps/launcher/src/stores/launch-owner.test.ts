import { describe, expect, it } from "vitest";
import type { LaunchPhase } from "./launch";
import { gameStage, launchOwner, planPlay, playingAccountId } from "./launch-owner";

const idle: LaunchPhase = { kind: "idle" };
const preparing: LaunchPhase = {
  kind: "working",
  stage: "assets",
  percent: 42,
  doneFiles: 10,
  totalFiles: 20,
  bytesPerSec: 0,
};
const launching: LaunchPhase = { kind: "launching" };
const opening: LaunchPhase = { kind: "running", since: 1, shown: false, firstStart: false };
const open: LaunchPhase = { kind: "running", since: 1, shown: true, firstStart: false };
const failed: LaunchPhase = { kind: "error", code: "X", message: "m", details: "d" };

const ejder = { id: "a1", name: "EjderAvcisi" };

describe("gameStage", () => {
  it("names how far the launch is", () => {
    expect(gameStage(idle)).toBeNull();
    expect(gameStage(failed)).toBeNull();
    expect(gameStage(preparing)).toBe("preparing");
    expect(gameStage(launching)).toBe("opening");
    expect(gameStage(opening)).toBe("opening");
    expect(gameStage(open)).toBe("open");
  });
});

describe("launchOwner (PLAY-ACCT)", () => {
  it("is nobody's while nothing runs", () => {
    expect(launchOwner(idle, null, "a1")).toBe("none");
    expect(launchOwner(failed, ejder, "a1")).toBe("none");
  });

  it("is the account's that plays, from the game's start on", () => {
    expect(launchOwner(opening, ejder, "a1")).toBe("mine");
    expect(launchOwner(open, ejder, "a1")).toBe("mine");
    // The owner's report: another account selected while the game runs is not in game.
    expect(launchOwner(open, ejder, "a2")).toBe("other");
    expect(launchOwner(opening, ejder, "a2")).toBe("other");
  });

  it("is every account's while its files are prepared (it starts with the one selected then)", () => {
    expect(launchOwner(preparing, null, "a1")).toBe("mine");
    expect(launchOwner(preparing, null, "a2")).toBe("mine");
    expect(launchOwner(launching, null, "a2")).toBe("mine");
  });

  it("counts a game whose account is not known, or no account to tell, as the selected one's", () => {
    // A game an older launcher left running.
    expect(launchOwner(open, null, "a2")).toBe("mine");
    // Accounts still loading, or none at all: nothing else could play.
    expect(launchOwner(open, ejder, undefined)).toBe("mine");
    expect(launchOwner(open, ejder, null)).toBe("mine");
    expect(launchOwner(open, { id: "", name: "" }, "a2")).toBe("mine");
  });
});

describe("planPlay", () => {
  const game = (phase: LaunchPhase, account = ejder) => ({
    phase,
    instanceId: "projects",
    account,
  });

  it("plays when nothing runs", () => {
    expect(planPlay({ phase: idle, instanceId: null, account: null }, "projects", "a1")).toBe(
      "play",
    );
    expect(planPlay(game(failed), "projects", "a2")).toBe("play");
  });

  it("does nothing for the game this account already has on (or on its way)", () => {
    expect(planPlay(game(open), "projects", "a1")).toBe("nothing");
    expect(planPlay(game(opening), "projects", "a1")).toBe("nothing");
    expect(planPlay({ ...game(preparing), account: null }, "projects", "a2")).toBe("nothing");
  });

  it("asks first for another account's game, even of the same instance", () => {
    expect(planPlay(game(open), "projects", "a2")).toBe("switch");
    expect(planPlay(game(opening), "projects", "a2")).toBe("switch");
  });

  it("asks first for another instance while a game runs (one game at a time)", () => {
    expect(planPlay(game(open), "fabric", "a1")).toBe("switch");
    expect(planPlay({ ...game(preparing), account: null }, "fabric", "a1")).toBe("switch");
  });
});

describe("playingAccountId", () => {
  it("is the account whose game window is on screen", () => {
    expect(playingAccountId(open, ejder)).toBe("a1");
  });

  it("is nobody while the game opens, when nothing runs or when the account is not known", () => {
    expect(playingAccountId(opening, ejder)).toBeNull();
    expect(playingAccountId(preparing, null)).toBeNull();
    expect(playingAccountId(idle, ejder)).toBeNull();
    expect(playingAccountId(open, null)).toBeNull();
  });
});
