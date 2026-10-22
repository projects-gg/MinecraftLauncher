import { describe, expect, it } from "vitest";
import { CLOSE_GRACE_MS, type CloseStep, closeNext, FORCE_GRACE_MS, gameNow } from "./connect";
import { joinTarget, restartInstance } from "./play-together";

describe("gameNow", () => {
  it("tells a closed game, one on screen and one on its way", () => {
    expect(gameNow({ kind: "idle" })).toBe("closed");
    expect(gameNow({ kind: "error", code: "X", message: "", details: "" })).toBe("closed");
    expect(gameNow({ kind: "running", since: 1, shown: true, firstStart: false })).toBe("open");
    expect(gameNow({ kind: "running", since: 1, shown: false, firstStart: false })).toBe("opening");
    expect(gameNow({ kind: "launching" })).toBe("opening");
    expect(
      gameNow({
        kind: "working",
        stage: "assets",
        percent: 40,
        doneFiles: 1,
        totalFiles: 2,
        bytesPerSec: 0,
      }),
    ).toBe("opening");
  });
});

describe("closeNext", () => {
  const closing: CloseStep = { kind: "closing", since: 1_000 };

  it("ends in the server when the overlay takes the player there", () => {
    expect(closeNext({ kind: "inGame" }, { type: "inGame", done: true, now: 2_000 })).toBe(
      "connected",
    );
  });

  it("closes the game when the overlay cannot", () => {
    expect(closeNext({ kind: "inGame" }, { type: "inGame", done: false, now: 2_000 })).toEqual({
      kind: "closing",
      since: 2_000,
    });
  });

  it("waits for the game's own close, then offers to end it", () => {
    expect(closeNext(closing, { type: "asked", asked: true })).toBe(closing);
    expect(closeNext(closing, { type: "tick", now: 1_000 + CLOSE_GRACE_MS - 1 })).toBe(closing);
    expect(closeNext(closing, { type: "tick", now: 1_000 + CLOSE_GRACE_MS })).toEqual({
      kind: "overdue",
    });
    // Nothing of it could be asked: no waiting for a close that cannot come.
    expect(closeNext(closing, { type: "asked", asked: false })).toEqual({ kind: "overdue" });
  });

  it("ends the game only at the player's word, and may wait again", () => {
    expect(closeNext({ kind: "overdue" }, { type: "tick", now: 99_999 })).toEqual({
      kind: "overdue",
    });
    expect(closeNext({ kind: "overdue" }, { type: "force", now: 60_000 })).toEqual({
      kind: "forcing",
      since: 60_000,
    });
    expect(closeNext({ kind: "overdue" }, { type: "wait", now: 50_000 })).toEqual({
      kind: "closing",
      since: 50_000,
    });
    // "Zorla kapat" does nothing before the warning.
    expect(closeNext(closing, { type: "force", now: 2_000 })).toBe(closing);
  });

  it("goes back to the question when the end the player asked for does not come", () => {
    const forcing: CloseStep = { kind: "forcing", since: 60_000 };
    expect(closeNext(forcing, { type: "tick", now: 60_000 + FORCE_GRACE_MS - 1 })).toBe(forcing);
    expect(closeNext(forcing, { type: "tick", now: 60_000 + FORCE_GRACE_MS })).toEqual({
      kind: "overdue",
    });
  });

  it("starts the game again once it ended, however it did", () => {
    for (const step of [
      closing,
      { kind: "overdue" },
      { kind: "forcing", since: 1 },
      { kind: "stopping" },
      { kind: "inGame" },
    ] satisfies CloseStep[]) {
      expect(closeNext(step, { type: "exited" })).toBe("start");
    }
  });
});

describe("restartInstance", () => {
  const instances = [
    { id: "official", autoJoin: "play.projects.gg" },
    { id: "own-projects", autoJoin: "mc.projects.gg:25565" },
    { id: "fabric-1-21" },
  ];
  const server = joinTarget({ where: "other", host: "mc.example.net:25570" });
  if (!server) throw new Error("test address");

  it("starts the instance that ran again for another server", () => {
    expect(restartInstance(server, "fabric-1-21", instances)).toBe("fabric-1-21");
    expect(restartInstance(server, "official", instances)).toBe("official");
  });

  it("goes to Projects only with an instance that joins it by itself", () => {
    expect(restartInstance({ kind: "projects" }, "own-projects", instances)).toBe("own-projects");
    expect(restartInstance({ kind: "projects" }, "official", instances)).toBe("official");
    // Another instance: "Katıl" picks the Projects one, as with the game closed.
    expect(restartInstance({ kind: "projects" }, "fabric-1-21", instances)).toBeNull();
  });

  it("falls back to the usual pick when the instance is gone", () => {
    expect(restartInstance(server, "deleted", instances)).toBeNull();
    expect(restartInstance(server, null, instances)).toBeNull();
  });
});
