import { describe, expect, it } from "vitest";
import type { SocialActivity } from "@/lib/api/social-types";
import {
  canInvite,
  joinStep,
  joinTarget,
  onTarget,
  playInstance,
  targetAddress,
  targetName,
} from "./play-together";

const projects: SocialActivity = { state: "playing", host: "play.projects.gg" };
const other: SocialActivity = { state: "playing", host: "mc.example.net:25570" };

describe("canInvite", () => {
  it("is offered on projects.gg whatever the sharing", () => {
    expect(canInvite(projects, false)).toBe(true);
    expect(canInvite({ state: "playing", host: "projects.gg" }, false)).toBe(true);
  });

  it("is offered on another server only while its address is shared", () => {
    expect(canInvite(other, true)).toBe(true);
    expect(canInvite(other, false)).toBe(false);
  });

  it("is not offered where an invite cannot lead", () => {
    expect(canInvite({ state: "launcher" }, true)).toBe(false);
    expect(canInvite({ state: "singleplayer" }, true)).toBe(false);
    // In game, but not on a server the launcher knows (the menu, a LAN world).
    expect(canInvite({ state: "playing" }, true)).toBe(false);
  });
});

describe("joinTarget", () => {
  it("joins projects.gg", () => {
    expect(joinTarget({ where: "projects", host: "play.projects.gg" })).toEqual({
      kind: "projects",
    });
    expect(joinTarget({ where: "projects" })).toEqual({ kind: "projects" });
    // An address on projects.gg is projects.gg, whatever it was called.
    expect(joinTarget({ where: "other", host: "tr.projects.gg" })).toEqual({ kind: "projects" });
  });

  it("joins another server at its address", () => {
    expect(joinTarget({ where: "other", host: "mc.example.net:25570" })).toEqual({
      kind: "server",
      host: "mc.example.net",
      port: 25570,
      address: "mc.example.net:25570",
    });
    expect(joinTarget({ where: "other", host: "mc.example.net" })).toMatchObject({
      host: "mc.example.net",
      port: null,
    });
  });

  it("has nowhere to go without a server", () => {
    expect(joinTarget(undefined)).toBeNull();
    expect(joinTarget({ where: "singleplayer" })).toBeNull();
    expect(joinTarget({ where: "other" })).toBeNull();
    expect(joinTarget({ where: "other", host: "not an address" })).toBeNull();
  });

  it("names the place like the player knows it", () => {
    expect(targetName({ kind: "projects" })).toBe("Projects");
    expect(targetAddress({ kind: "projects" })).toBe("play.projects.gg");
    const server = joinTarget({ where: "other", host: "mc.example.net:25570" });
    expect(server && targetName(server)).toBe("mc.example.net:25570");
    expect(server && targetAddress(server)).toBe("mc.example.net:25570");
  });
});

describe("joinStep", () => {
  const server = joinTarget({ where: "other", host: "mc.example.net:25570" });
  if (!server) throw new Error("test address");

  it("asks once when the game is closed", () => {
    expect(joinStep({ kind: "projects" }, false, { state: "launcher" })).toBe("confirm");
    expect(joinStep(server, false, { state: "launcher" })).toBe("confirm");
  });

  it("never closes an open game: it says how to join from the game", () => {
    expect(joinStep({ kind: "projects" }, true, { state: "singleplayer" })).toBe("gameOpen");
    expect(joinStep(server, true, projects)).toBe("gameOpen");
    // Still opening (no server yet).
    expect(joinStep(server, true, { state: "launcher" })).toBe("gameOpen");
  });

  it("has nothing to do when the player is already there", () => {
    expect(joinStep({ kind: "projects" }, true, projects)).toBe("already");
    expect(joinStep(server, true, other)).toBe("already");
  });
});

describe("onTarget", () => {
  it("compares addresses without case and with the default port", () => {
    const server = joinTarget({ where: "other", host: "MC.Example.net" });
    if (!server) throw new Error("test address");
    expect(onTarget(server, { state: "playing", host: "mc.example.net:25565" })).toBe(true);
    expect(onTarget(server, { state: "playing", host: "mc.example.net:25566" })).toBe(false);
    expect(onTarget(server, { state: "playing" })).toBe(false);
    expect(onTarget({ kind: "projects" }, other)).toBe(false);
  });
});

describe("playInstance", () => {
  it("starts what OYNA would start, or leaves the choice to the Projects instance", () => {
    const instances = [{ id: "projects" }, { id: "fabric-1-21" }];
    expect(playInstance(instances, "fabric-1-21")).toBe("fabric-1-21");
    expect(playInstance(instances, "deleted")).toBeNull();
    expect(playInstance(instances, null)).toBeNull();
  });
});
