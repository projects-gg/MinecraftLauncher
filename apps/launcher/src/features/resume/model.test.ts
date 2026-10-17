import { describe, expect, it } from "vitest";
import type { ResumeTarget } from "@/lib/api";
import { parseAddress, sameAsAutoJoin } from "./model";

const server = (host: string, port: number | null): ResumeTarget => ({
  kind: "server",
  name: "Projects",
  quickPlay: { kind: "multiplayer", host, port },
});

const world: ResumeTarget = {
  kind: "world",
  name: "Ejderha Adası",
  quickPlay: { kind: "singleplayer", world: "Ejderha Adasi" },
};

describe("parseAddress", () => {
  it("reads hosts, ports and IPv6 literals", () => {
    expect(parseAddress("Play.Projects.GG.")).toEqual({ host: "play.projects.gg", port: 25565 });
    expect(parseAddress("mc.example.com:25570")).toEqual({ host: "mc.example.com", port: 25570 });
    expect(parseAddress("[2001:db8::1]:25566")).toEqual({ host: "2001:db8::1", port: 25566 });
    expect(parseAddress("2001:db8::1")).toEqual({ host: "2001:db8::1", port: 25565 });
    expect(parseAddress("host:abc")).toBeNull();
    expect(parseAddress("  ")).toBeNull();
  });
});

describe("sameAsAutoJoin", () => {
  it("hides the shortcut when OYNA joins the same server", () => {
    expect(sameAsAutoJoin(server("play.projects.gg", null), "play.projects.gg", true)).toBe(true);
    expect(sameAsAutoJoin(server("PLAY.projects.gg", 25565), "play.projects.gg", true)).toBe(true);
  });

  it("keeps it for other servers, worlds, or with auto-join off", () => {
    expect(sameAsAutoJoin(server("play.projects.gg", null), "play.projects.gg", false)).toBe(false);
    expect(sameAsAutoJoin(server("mc.hypixel.net", null), "play.projects.gg", true)).toBe(false);
    expect(sameAsAutoJoin(server("play.projects.gg", 25566), "play.projects.gg", true)).toBe(false);
    expect(sameAsAutoJoin(world, "play.projects.gg", true)).toBe(false);
    expect(sameAsAutoJoin(server("play.projects.gg", null), undefined, true)).toBe(false);
  });
});
