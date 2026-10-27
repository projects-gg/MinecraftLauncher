import { describe, expect, it } from "vitest";
import type { SocialTeam, TeamMember } from "@/lib/api/social-types";
import {
  afterHandingOn,
  allows,
  canRemove,
  cleanCrewText,
  crewAnniversary,
  crewOnProjects,
  crewRights,
  DEFAULT_PERMISSIONS,
  daysSince,
  formatTeamCodeInput,
  formatTeamTag,
  membersMayHave,
  mottoLength,
  mottoProblem,
  normalizeTeamCode,
  rosterGroups,
  rosterOrder,
  switchesOf,
  switchLevel,
  teammates,
  teammatesOnline,
  teamNameProblem,
  teamTagValid,
  whereabouts,
} from "./teams";

function member(id: string, name: string, extra: Partial<TeamMember> = {}): TeamMember {
  return {
    id,
    name,
    username: name.toLowerCase(),
    avatar: "discord",
    role: "member",
    moderator: false,
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

describe("crew codes", () => {
  it("formats while typing with the crew prefix", () => {
    expect(formatTeamCodeInput("ekp7q2x")).toBe("EKP-7Q2X");
    expect(formatTeamCodeInput("7q2x9kda")).toBe("EKP-7Q2X-9KDA");
    expect(formatTeamCodeInput("o1il")).toBe("EKP-0111");
    expect(formatTeamCodeInput("")).toBe("");
    // What the field shows, typed on: the prefix stays a prefix.
    expect(formatTeamCodeInput("EKP-7Q2X-9")).toBe("EKP-7Q2X-9");
  });

  it("takes the code before crews and pasted messages, the last whole code winning", () => {
    expect(normalizeTeamCode("tkm 7q2x 9kda")).toBe("EKP-7Q2X-9KDA");
    expect(formatTeamCodeInput("Takım kodu: TKM-7Q2X-9KDA")).toBe("EKP-7Q2X-9KDA");
    expect(formatTeamCodeInput("Ekip kodu: EKP-7Q2X-9KDA")).toBe("EKP-7Q2X-9KDA");
    expect(formatTeamCodeInput("eski TKM-AAAA-BBBB, yeni EKP_CCCC_DDDD")).toBe("EKP-CCCC-DDDD");
    // E, K, P are code symbols too: a code that starts with them stays whole.
    expect(normalizeTeamCode("EKP-EKPA-1234")).toBe("EKP-EKPA-1234");
    expect(normalizeTeamCode("EKP-7Q2X")).toBeNull();
    expect(normalizeTeamCode("")).toBeNull();
    expect(normalizeTeamCode("PRJ-7Q2X-9KDA")).toBe(null);
  });
});

describe("names, tags and mottos", () => {
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

  it("checks mottos as the service does, an empty one being no motto", () => {
    expect(mottoProblem("")).toBeNull();
    expect(mottoProblem("   ​ ")).toBeNull();
    expect(mottoProblem("Birlikte güçlüyüz!")).toBeNull();
    expect(mottoProblem("Hazır mısınız, gidelim?")).toBeNull();
    expect(mottoProblem("Kim gelir? Biz & siz - hep beraber_")).toBeNull();
    expect(mottoProblem("x".repeat(41))).toBe("long");
    expect(mottoProblem("x".repeat(40))).toBeNull();
    expect(mottoProblem("discord.gg/abc")).toBe("chars");
    expect(mottoProblem("@herkes gelsin")).toBe("chars");
    expect(mottoProblem("Ejder 🐉")).toBe("chars");
    expect(mottoProblem("!!! ???")).toBe("letters");
  });

  it("cleans and counts like the service (code points, invisible characters dropped)", () => {
    expect(cleanCrewText("  Biz​   buradayız  ")).toBe("Biz buradayız");
    expect(mottoLength("  Biz  buradayız ")).toBe(13);
    // NFC first (e + acute is one é); a mark with no composed form counts on its own.
    expect(mottoLength("é")).toBe(1);
    expect(mottoLength("q̇b")).toBe(3);
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
      member("6", "Deniz", { state: "playing", game: { where: "projects" }, blocked: true }),
    ],
  };

  it("shows the others in presence order and counts who is online", () => {
    expect(teammates(team).map((m) => m.id)).toEqual(["4", "6", "3", "2", "5"]);
    // Someone blocked shows offline.
    expect(teammatesOnline(team)).toBe(3);
    expect(crewOnProjects(team).map((m) => m.id)).toEqual(["4"]);
  });

  it("puts the leader first in the crew's order", () => {
    const led = {
      members: [
        member("1", "Ben", { me: true }),
        member("2", "Zeki", { role: "founder" }),
        member("5", "Can", { state: "launcher" }),
      ],
    };
    expect(rosterOrder(led).map((m) => m.id)).toEqual(["2", "1", "5"]);
  });

  it("groups the crew page's teammates by where they are, leader and moderators first", () => {
    const crew = {
      members: [
        member("1", "Ben", { me: true, role: "founder" }),
        member("2", "Zeki", { state: "launcher", role: "founder" }),
        member("3", "Ayşe", { state: "launcher", moderator: true }),
        member("8", "Ali", { state: "launcher" }),
        member("4", "Bora", { state: "playing", game: { where: "projects" } }),
        member("5", "Can", { state: "playing", game: { where: "singleplayer" } }),
        member("6", "Deniz", { state: "playing", game: { where: "projects" }, blocked: true }),
        member("7", "Ece"),
      ],
    };
    const groups = rosterGroups(crew);
    // The player is in none: their own row sits above the groups.
    expect(groups.map((g) => [g.where, g.members.map((m) => m.id)])).toEqual([
      ["projects", ["4"]],
      ["playing", ["5"]],
      ["launcher", ["2", "3", "8"]],
      ["offline", ["6", "7"]],
    ]);
    // So the groups online add up to the card's "N çevrimiçi" (SMALL12: 4 over 3 + 2).
    const online = groups
      .filter((g) => g.where !== "offline")
      .reduce((sum, g) => sum + g.members.length, 0);
    expect(online).toBe(teammatesOnline(crew));
    // Someone blocked shows offline, as everywhere.
    expect(whereabouts(crew.members[6] as TeamMember)).toBe("offline");
  });
});

describe("rights", () => {
  const base: Pick<SocialTeam, "role" | "moderator" | "permissions"> = {
    role: "member",
    moderator: false,
    permissions: DEFAULT_PERMISSIONS,
  };
  const features = { calls: true, roles: true };

  it("gives a right to the roles at and above its level", () => {
    expect(allows("members", base)).toBe(true);
    expect(allows("moderators", base)).toBe(false);
    expect(allows("moderators", { ...base, moderator: true })).toBe(true);
    expect(allows("leader", { ...base, moderator: true })).toBe(false);
    expect(allows("leader", { ...base, role: "founder" })).toBe(true);
  });

  it("says what the player's role may do", () => {
    expect(crewRights(base, features)).toEqual({
      invite: true,
      call: true,
      look: false,
      lead: false,
    });
    const editing = {
      ...base,
      moderator: true,
      permissions: { ...DEFAULT_PERMISSIONS, edit: "moderators" as const },
    };
    expect(crewRights(editing, features).look).toBe(true);
    // Without roles on the service nobody but the leader edits; without calls nobody calls.
    expect(crewRights(editing, { calls: false, roles: false })).toEqual({
      invite: true,
      call: false,
      look: false,
      lead: false,
    });
    expect(crewRights({ ...base, role: "founder" }, features)).toEqual({
      invite: true,
      call: true,
      look: true,
      lead: true,
    });
    const quiet = {
      ...base,
      permissions: {
        ...DEFAULT_PERMISSIONS,
        invite: "moderators" as const,
        call: "leader" as const,
      },
    };
    expect(crewRights(quiet, features)).toMatchObject({ invite: false, call: false });
  });

  it("lets a moderator remove plain members only, the leader anyone", () => {
    const moderator = { ...base, moderator: true };
    const plain = member("2", "Zeki");
    const fellow = member("3", "Ayşe", { moderator: true });
    const leader = member("4", "Bora", { role: "founder" });
    expect(canRemove(moderator, plain, true)).toBe(true);
    expect(canRemove(moderator, fellow, true)).toBe(false);
    expect(canRemove(moderator, leader, true)).toBe(false);
    expect(canRemove(moderator, plain, false)).toBe(false);
    expect(
      canRemove(
        { ...moderator, permissions: { ...DEFAULT_PERMISSIONS, kick: "leader" } },
        plain,
        true,
      ),
    ).toBe(false);
    expect(canRemove(base, plain, true)).toBe(false);
    expect(canRemove({ ...base, role: "founder" }, fellow, true)).toBe(true);
    expect(canRemove({ ...base, role: "founder" }, member("1", "Ben", { me: true }), true)).toBe(
      false,
    );
  });

  it("moves the two switches of a right together, as its level", () => {
    expect(switchesOf("members")).toEqual({ moderators: true, members: true });
    expect(switchesOf("moderators")).toEqual({ moderators: true, members: false });
    expect(switchesOf("leader")).toEqual({ moderators: false, members: false });
    // Members on: moderators on too. Moderators off: members off too.
    expect(switchLevel("leader", "members", true)).toBe("members");
    expect(switchLevel("members", "moderators", false)).toBe("leader");
    expect(switchLevel("members", "members", false)).toBe("moderators");
    expect(switchLevel("leader", "moderators", true)).toBe("moderators");
    expect(switchLevel("members", "moderators", true)).toBe("members");
    expect(membersMayHave("invite") && membersMayHave("call")).toBe(true);
    expect(membersMayHave("kick") || membersMayHave("edit")).toBe(false);
  });

  it("says who the leader becomes after handing on", () => {
    const crew = {
      members: [
        member("1", "Ben", { me: true, role: "founder" }),
        member("2", "A", { moderator: true }),
        member("3", "B", { moderator: true }),
        member("4", "C", { moderator: true }),
        member("5", "D", { moderator: true }),
        member("6", "E"),
      ],
    };
    // Four other moderators: no room, a member. Handing on to one of them frees a place.
    expect(afterHandingOn(crew, "6", true)).toBe("member");
    expect(afterHandingOn(crew, "2", true)).toBe("moderator");
    expect(afterHandingOn(crew, "2", false)).toBe("member");
  });
});

describe("the crew's days", () => {
  it("marks months and years on the founding day of the month", () => {
    expect(crewAnniversary("2026-09-20", new Date(2026, 9, 20))).toEqual({ months: 1 });
    expect(crewAnniversary("2025-10-20", new Date(2026, 9, 20))).toEqual({ years: 1 });
    expect(crewAnniversary("2026-09-20", new Date(2026, 9, 21))).toBeNull();
    expect(crewAnniversary("2026-10-20", new Date(2026, 9, 20))).toBeNull();
    // Founded on the 31st: the shorter month's last day.
    expect(crewAnniversary("2026-08-31", new Date(2026, 8, 30))).toEqual({ months: 1 });
    expect(crewAnniversary("nonsense", new Date(2026, 9, 20))).toBeNull();
  });

  it("counts whole days in the crew", () => {
    const now = Date.UTC(2026, 9, 20, 12);
    expect(daysSince(now / 1000 - 12 * 86_400 - 60, now)).toBe(12);
    expect(daysSince(now / 1000 - 60, now)).toBe(0);
  });
});
