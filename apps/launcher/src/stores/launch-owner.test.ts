import { describe, expect, it } from "vitest";
import type { LaunchPhase, RunningGame } from "./launch";
import {
  gameStage,
  inProgress,
  type LaunchView,
  launchOwner,
  othersInGame,
  ownLaunch,
  planPlay,
  playingAccountIds,
  runningPhase,
} from "./launch-owner";

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
const kor = { id: "a2", name: "KorAtesi_" };

function game(run: number, account = ejder, instanceId = "projects", shown = true): RunningGame {
  return { run, game: run + 100, instanceId, account, since: run * 1000, shown, firstStart: false };
}

/** The store with `games` on screen and nothing on its way. */
function playing(...games: RunningGame[]): LaunchView {
  const newest = games.at(-1);
  return {
    phase: newest ? runningPhase(newest) : idle,
    instanceId: newest?.instanceId ?? null,
    account: newest?.account ?? null,
    games,
  };
}

/** A launch on its way (`account`: null while its files are prepared), beside `games`. */
function starting(
  phase: LaunchPhase,
  account: typeof ejder | null,
  instanceId = "projects",
  ...games: RunningGame[]
): LaunchView {
  return { phase, instanceId, account, games };
}

describe("gameStage", () => {
  it("names how far the launch is", () => {
    expect(gameStage(idle)).toBeNull();
    expect(gameStage(failed)).toBeNull();
    expect(gameStage(preparing)).toBe("preparing");
    expect(gameStage(launching)).toBe("opening");
    expect(gameStage(opening)).toBe("opening");
    expect(gameStage(open)).toBe("open");
    expect([preparing, launching, opening].every(inProgress)).toBe(true);
    expect([idle, failed, open].some(inProgress)).toBe(false);
  });
});

describe("launchOwner (PLAY-ACCT)", () => {
  it("is the account's that plays, every account's while files are prepared", () => {
    expect(launchOwner(idle, null, "a1")).toBe("none");
    expect(launchOwner(open, ejder, "a1")).toBe("mine");
    expect(launchOwner(open, ejder, "a2")).toBe("other");
    expect(launchOwner(preparing, null, "a2")).toBe("mine");
    // An older launcher's game, or no account to tell: the selected one's.
    expect(launchOwner(open, null, "a2")).toBe("mine");
    expect(launchOwner(open, ejder, undefined)).toBe("mine");
  });
});

describe("ownLaunch (PLAY-AGAIN): what the bar shows for the selected account", () => {
  it("is the account's newest game on screen, while others play too", () => {
    const view = playing(game(1), game(2, kor, "hardcore"));
    expect(ownLaunch(view, "a1")).toMatchObject({
      owner: "mine",
      instanceId: "projects",
      phase: { kind: "running", shown: true, since: 1000 },
    });
    expect(ownLaunch(view, "a2")).toMatchObject({ owner: "mine", instanceId: "hardcore" });
    // Two games of one account: the newer one.
    const twice = playing(game(1), game(3));
    expect(ownLaunch(twice, "a1").game?.run).toBe(3);
  });

  it("is nothing for an account with no game while others play", () => {
    const view = playing(game(1));
    expect(ownLaunch(view, "a2")).toEqual({
      owner: "other",
      phase: idle,
      instanceId: null,
      game: null,
    });
  });

  it("is the launch on its way when it is the account's or nobody's yet", () => {
    const files = starting(preparing, null, "fabric", game(1, kor));
    expect(ownLaunch(files, "a1")).toMatchObject({ owner: "mine", phase: preparing });
    // Bound to KorAtesi_: EjderAvcisi sees its own game, not that start.
    const theirs = starting(opening, kor, "fabric", game(1));
    expect(ownLaunch(theirs, "a1")).toMatchObject({ owner: "mine", instanceId: "projects" });
    expect(ownLaunch(theirs, "a1").phase).toMatchObject({ shown: true });
    expect(ownLaunch(starting(opening, kor, "fabric"), "a1").owner).toBe("other");
  });

  it("keeps the store's own phase when nothing runs (the last launch's error)", () => {
    expect(ownLaunch(starting(failed, null), "a1")).toMatchObject({ owner: "none", phase: failed });
  });

  it("gives the same phase object for the same game (views draw only on a change)", () => {
    const one = game(1);
    expect(runningPhase(one)).toBe(runningPhase(one));
    expect(runningPhase({ ...one })).not.toBe(runningPhase(one));
  });
});

describe("planPlay (PLAY-AGAIN)", () => {
  it("plays when nothing runs", () => {
    expect(planPlay(playing(), "projects", "a1")).toBe("play");
    expect(planPlay(starting(failed, null), "projects", "a2")).toBe("play");
  });

  it("does nothing for this account's launch of this instance on its way", () => {
    expect(planPlay(starting(preparing, null), "projects", "a2")).toBe("nothing");
    expect(planPlay(starting(opening, ejder), "projects", "a1")).toBe("nothing");
  });

  it("stops another launch on its way first (one at a time)", () => {
    expect(planPlay(starting(opening, ejder), "projects", "a2")).toBe("switch");
    expect(planPlay(starting(preparing, null), "fabric", "a1")).toBe("switch");
  });

  it("asks the owner's question when this account has a game on screen, any instance", () => {
    expect(planPlay(playing(game(1)), "projects", "a1")).toBe("again");
    expect(planPlay(playing(game(1)), "fabric", "a1")).toBe("again");
    // A game an older launcher left running counts as everyone's.
    expect(planPlay(playing(game(1, null as never)), "projects", "a2")).toBe("again");
  });

  it("starts beside other accounts' games", () => {
    expect(planPlay(playing(game(1)), "projects", "a2")).toBe("beside");
    expect(planPlay(playing(game(1), game(2, kor)), "hardcore", "a3")).toBe("beside");
    // A game still opening is a launch on its way: one at a time.
    expect(planPlay(starting(opening, ejder, "projects", game(1, kor)), "x", "a2")).toBe("switch");
  });
});

describe("Oyunda on the accounts, and the bar's quiet line (PLAY-AGAIN)", () => {
  it("marks every account with a game on screen, once", () => {
    const view = playing(game(1), game(2, kor), game(3), game(4, kor, "x", false));
    expect(playingAccountIds(view)).toEqual(["a1", "a2"]);
    expect(playingAccountIds(playing(game(1, null as never)))).toEqual([]);
  });

  it("names the other accounts in game while the selected one has none", () => {
    const view = playing(game(1), game(2, kor), game(3));
    expect(othersInGame(view, "a3")).toEqual(["EjderAvcisi", "KorAtesi_"]);
    expect(othersInGame(playing(game(1)), "a2")).toEqual(["EjderAvcisi"]);
    // The selected account plays (or starts): no line; the bar shows its own game.
    expect(othersInGame(view, "a1")).toEqual([]);
    expect(othersInGame(starting(preparing, null, "x", game(1)), "a2")).toEqual([]);
    // Nobody else, or no account to tell.
    expect(othersInGame(playing(), "a1")).toEqual([]);
    expect(othersInGame(view, null)).toEqual([]);
    // A game still opening is not "in game" yet.
    expect(othersInGame(playing(game(1, ejder, "projects", false)), "a2")).toEqual([]);
  });
});
