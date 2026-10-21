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
import {
  buildBackup,
  homeStrip,
  playingPeople,
  readBackup,
  somethingWaits,
  wantsNotice,
  whatWaits,
} from "./model";

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

  it("keeps a muted crew quiet until the mute ends, and follows the service's call switch", () => {
    const at = NOW / 1000;
    const on = { calls: true, muted: false, muteUntil: null };
    const muted = { calls: true, muted: true, muteUntil: at + 3_600 };
    const joined: SocialNotice = { kind: "crewmateOnProjects", id: "2" };
    const asked = { ...prefs, notifyCrewProjects: true };
    expect(wantsNotice(crewCall, "online", prefs, muted, NOW)).toBe(false);
    expect(wantsNotice(joined, "online", asked, muted, NOW)).toBe(false);
    // A timed mute ends by itself.
    expect(wantsNotice(crewCall, "online", prefs, muted, NOW + 3_600_000)).toBe(true);
    expect(wantsNotice(joined, "online", asked, muted, NOW + 3_600_000)).toBe(true);
    // "Ben açana kadar" does not.
    const forever = { ...muted, muteUntil: null };
    expect(wantsNotice(crewCall, "online", prefs, forever, NOW + 86_400_000)).toBe(false);
    // Where the service keeps the switch, it wins over this computer's.
    expect(wantsNotice(crewCall, "online", { ...prefs, notifyCrewCalls: false }, on, NOW)).toBe(
      true,
    );
    expect(wantsNotice(crewCall, "online", prefs, { ...on, calls: false }, NOW)).toBe(false);
    // Calls off is not a mute: crewmates joining Projects are still said.
    expect(wantsNotice(joined, "online", asked, { ...on, calls: false }, NOW)).toBe(true);
    // Friends' notices are no crew notices.
    expect(wantsNotice({ kind: "friendOnline", id: "1" }, "online", prefs, forever, NOW)).toBe(
      true,
    );
  });

  it("lights the gold dot for a call only while calls are heard", () => {
    const waiting = { incoming: [], invites: [], calls: [call()] };
    const at = NOW / 1000;
    expect(somethingWaits(waiting, true, NOW)).toBe(true);
    expect(somethingWaits(waiting, false, NOW)).toBe(false);
    const muted = { calls: true, muted: true, muteUntil: at + 60 };
    expect(somethingWaits({ ...waiting, crewNotifications: muted }, true, NOW)).toBe(false);
    expect(somethingWaits({ ...waiting, crewNotifications: muted }, true, NOW + 60_000)).toBe(true);
    // A request still waits, muted or not.
    const request = { ...waiting, incoming: [{} as FriendRequest], crewNotifications: muted };
    expect(somethingWaits(request, true, NOW)).toBe(true);
    expect(somethingWaits({ incoming: [], invites: [], calls: [] }, true, NOW)).toBe(false);
  });

  it("tells a waiting call from a request or an invite (E2E6)", () => {
    const open = call();
    // A call alone is neither a request nor an invite.
    expect(whatWaits({ incoming: [], invites: [], calls: [open] }, true, NOW)).toEqual({
      asks: false,
      call: open,
    });
    // Both at once: both are said.
    const both = { incoming: [{} as FriendRequest], invites: [], calls: [open] };
    expect(whatWaits(both, true, NOW)).toEqual({ asks: true, call: open });
    // An answered call, the player's own call and a call not heard wait for nothing.
    for (const view of [
      { incoming: [], invites: [], calls: [call({ myAnswer: "soon" })] },
      { incoming: [], invites: [], calls: [call({ mine: true })] },
    ]) {
      expect(whatWaits(view, true, NOW)).toEqual({ asks: false, call: null });
    }
    expect(
      whatWaits({ incoming: [], invites: [{} as SocialInvite], calls: [open] }, false, NOW),
    ).toEqual({ asks: true, call: null });
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
