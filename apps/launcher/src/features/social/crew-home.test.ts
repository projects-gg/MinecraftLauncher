import { describe, expect, it } from "vitest";
import type {
  CrewCall,
  Friend,
  FriendRequest,
  SocialInvite,
  SocialNotice,
  SocialTeam,
  TeamMember,
} from "@/lib/api/social-types";
import { buildBackup, homeStrip, playingPeople, readBackup, wantsNotice } from "./model";

const NOW = 1_792_521_000_000;

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

const onProjects = { state: "playing" as const, game: { where: "projects" as const } };

function crew(members: TeamMember[]): SocialTeam {
  return {
    id: "0123456789abcdef",
    name: "Gece Kartalları",
    tag: "GKR",
    emblem: "controller",
    color: "teal",
    frame: "banner",
    motto: "",
    code: "EKP-7Q2X-9KDA",
    createdOn: "2026-09-20",
    role: "member",
    moderator: false,
    permissions: { invite: "members", call: "members", kick: "moderators", edit: "leader" },
    members: [member("100", "Ben", { me: true }), ...members],
    invited: [],
    limit: 30,
  };
}

function call(extra: Partial<CrewCall> = {}): CrewCall {
  return {
    id: "c1",
    from: "1",
    mine: false,
    kind: "join",
    startedAt: NOW / 1000 - 60,
    endsAt: NOW / 1000 + 600,
    answers: [],
    ...extra,
  };
}

describe("Home's line with a crew", () => {
  const view = {
    enabled: true,
    auth: "signedIn" as const,
    loaded: true,
    friends: [] as Friend[],
    incoming: [] as FriendRequest[],
    outgoing: [] as FriendRequest[],
    invites: [] as SocialInvite[],
    teams: true,
    calls: [] as CrewCall[],
  };
  const context = { closedCalls: [], gameBusy: false, now: NOW };

  it("puts an unanswered call first, then two or more crewmates on Projects", () => {
    const team = crew([member("1", "Ayşe", onProjects), member("2", "Bora", onProjects)]);
    expect(homeStrip({ ...view, team, calls: [call()] }, [], context)).toBe("call");
    expect(homeStrip({ ...view, team }, [], context)).toBe("crew");
    // Answered, closed here, or the player's own: the next line.
    expect(homeStrip({ ...view, team, calls: [call({ myAnswer: "soon" })] }, [], context)).toBe(
      "crew",
    );
    expect(
      homeStrip({ ...view, team, calls: [call()] }, [], { ...context, closedCalls: ["c1"] }),
    ).toBe("crew");
    expect(homeStrip({ ...view, team, calls: [call({ mine: true })] }, [], context)).toBe("crew");
    // Run out: gone.
    expect(homeStrip({ ...view, team, calls: [call({ endsAt: NOW / 1000 })] }, [], context)).toBe(
      "crew",
    );
  });

  it("waits while a game is open, and says one crewmate on Projects like a friend", () => {
    const team = crew([member("1", "Ayşe", onProjects), member("2", "Bora", onProjects)]);
    expect(homeStrip({ ...view, team, calls: [call()] }, [], { ...context, gameBusy: true })).toBe(
      "playing",
    );
    const alone = crew([member("1", "Ayşe", onProjects), member("2", "Bora")]);
    expect(homeStrip({ ...view, team: alone }, [], context)).toBe("playing");
    // Someone blocked is not on Projects for the player.
    const blocked = crew([
      member("1", "Ayşe", onProjects),
      member("2", "Bora", { ...onProjects, blocked: true }),
    ]);
    expect(homeStrip({ ...view, team: blocked }, [], context)).toBe("playing");
  });

  it("ignores the crew while crews are off", () => {
    const team = crew([member("1", "Ayşe", onProjects), member("2", "Bora", onProjects)]);
    expect(homeStrip({ ...view, teams: false, team, calls: [call()] }, [], context)).toBe("add");
  });

  it("names friends in game first, then crewmates on Projects who are not friends", () => {
    const friends: Friend[] = [
      {
        id: "1",
        name: "Ayşe",
        username: "ayse",
        avatar: "discord",
        state: "playing",
        dnd: false,
        game: { where: "projects" },
        discordUrl: "",
      },
    ];
    const team = crew([
      member("1", "Ayşe", { ...onProjects, friend: true }),
      member("2", "Bora", onProjects),
    ]);
    expect(playingPeople({ friends, teams: true, team }, []).map((p) => p.id)).toEqual(["1", "2"]);
    expect(playingPeople({ friends, teams: false, team }, []).map((p) => p.id)).toEqual(["1"]);
  });
});

describe("crew notices", () => {
  const prefs = {
    favorites: ["1"],
    notifyFavoriteOnline: true,
    notifyFavoriteProjects: true,
  };
  const crewCall: SocialNotice = { kind: "crewCall", call: call() };

  it("tells calls unless turned off, and never on do-not-disturb", () => {
    expect(wantsNotice(crewCall, "online", prefs)).toBe(true);
    expect(wantsNotice(crewCall, "online", { ...prefs, notifyCrewCalls: false })).toBe(false);
    expect(wantsNotice(crewCall, "dnd", prefs)).toBe(false);
    expect(
      wantsNotice({ kind: "crewCallJoin", call: call() }, "online", {
        ...prefs,
        notifyCrewCalls: false,
      }),
    ).toBe(false);
  });

  it("tells crewmates joining Projects only when asked, once per person", () => {
    const joined = (id: string): SocialNotice => ({ kind: "crewmateOnProjects", id });
    expect(wantsNotice(joined("2"), "online", prefs)).toBe(false);
    expect(wantsNotice(joined("2"), "online", { ...prefs, notifyCrewProjects: true })).toBe(true);
    // A favourite friend's own notice says it already.
    expect(wantsNotice(joined("1"), "online", { ...prefs, notifyCrewProjects: true })).toBe(false);
    expect(
      wantsNotice(joined("1"), "online", {
        ...prefs,
        notifyCrewProjects: true,
        notifyFavoriteProjects: false,
      }),
    ).toBe(true);
  });

  it("carries the crew switches in the settings backup, older backups keeping the defaults", () => {
    const backup = buildBackup(
      {},
      {
        favorites: [],
        notifyFavoriteOnline: true,
        notifyFavoriteProjects: true,
        notifyCrewCalls: false,
        notifyCrewProjects: true,
      },
    );
    expect(backup.friends).toMatchObject({ notifyCrewCalls: false, notifyCrewProjects: true });
    expect(readBackup(backup).friends).toMatchObject({
      notifyCrewCalls: false,
      notifyCrewProjects: true,
    });
    const older = readBackup({ v: 1, prefs: {}, friends: { favorites: [] } });
    expect(older.friends.notifyCrewCalls).toBeUndefined();
    expect(readBackup({ friends: { notifyCrewCalls: "yes" } }).friends.notifyCrewCalls).toBe(
      undefined,
    );
  });
});
