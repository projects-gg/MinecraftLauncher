import { describe, expect, it } from "vitest";
import type { TeamMember } from "@/lib/api/social-types";
import {
  formatTeamCodeInput,
  formatTeamTag,
  normalizeTeamCode,
  rosterOrder,
  teammates,
  teammatesOnline,
  teamNameProblem,
  teamTagValid,
} from "./teams";

function member(id: string, name: string, extra: Partial<TeamMember> = {}): TeamMember {
  return {
    id,
    name,
    username: name.toLowerCase(),
    avatar: "discord",
    role: "member",
    me: false,
    friend: false,
    blocked: false,
    state: "offline",
    dnd: false,
    discordUrl: `https://discord.com/users/${id}`,
    joinedAt: 0,
    ...extra,
  };
}

describe("team codes", () => {
  it("formats while typing and accepts pasted messages", () => {
    expect(formatTeamCodeInput("tkm7q2x")).toBe("TKM-7Q2X");
    expect(formatTeamCodeInput("Takım kodu: TKM-7Q2X-9KDA")).toBe("TKM-7Q2X-9KDA");
    expect(formatTeamCodeInput("o1il")).toBe("TKM-0111");
    expect(normalizeTeamCode("tkm 7q2x 9kda")).toBe("TKM-7Q2X-9KDA");
    expect(normalizeTeamCode("TKM-7Q2X")).toBeNull();
    expect(normalizeTeamCode("")).toBeNull();
  });
});

describe("names and tags", () => {
  it("checks names as the service does", () => {
    expect(teamNameProblem("Gece Kartalları")).toBeNull();
    expect(teamNameProblem("  Okul   9-B ")).toBeNull();
    expect(teamNameProblem("ab")).toBe("short");
    expect(teamNameProblem("!!!")).toBe("short");
    expect(teamNameProblem("a.b.c")).toBe("chars");
    expect(teamNameProblem("x".repeat(25))).toBe("long");
    expect(teamNameProblem("discord.gg/abc")).toBe("chars");
    expect(teamNameProblem("Ejder 🐉")).toBe("chars");
  });

  it("types tags in Turkish upper case", () => {
    expect(formatTeamTag("gkr")).toBe("GKR");
    expect(formatTeamTag("iş")).toBe("İŞ");
    expect(formatTeamTag("a-b c.d e")).toBe("ABCD");
    expect(teamTagValid("GKR")).toBe(true);
    expect(teamTagValid("G")).toBe(false);
  });
});

describe("members", () => {
  const team = {
    members: [
      member("1", "Ben", { me: true, role: "founder" }),
      member("2", "Zeki", { state: "launcher" }),
      member("3", "Ayşe", { state: "playing", game: { where: "other" } }),
      member("4", "Bora", { state: "playing", game: { where: "projects" } }),
      member("5", "Can"),
    ],
  };

  it("shows the others in presence order and counts who is online", () => {
    expect(teammates(team).map((m) => m.id)).toEqual(["4", "3", "2", "5"]);
    expect(teammatesOnline(team)).toBe(3);
  });

  it("puts the founder first on the team page", () => {
    const led = {
      members: [
        member("1", "Ben", { me: true }),
        member("2", "Zeki", { role: "founder" }),
        member("5", "Can", { state: "launcher" }),
      ],
    };
    expect(rosterOrder(led).map((m) => m.id)).toEqual(["2", "1", "5"]);
    expect(rosterOrder(team).map((m) => m.id)).toEqual(["1", "4", "3", "2", "5"]);
  });
});
