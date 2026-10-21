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

function crew(members: TeamMember[], extra: Partial<SocialTeam> = {}): SocialTeam {
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
    mute: { muted: false, muteUntil: null },
    ...extra,
  };
}

function call(extra: Partial<CrewCall> = {}): CrewCall {
  return {
    id: "c1",
    from: "1",
    mine: false,
    kind: "join",
    crews: [],
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
    expect(homeStrip({ ...view, crews: [team], calls: [call()] }, [], context)).toBe("call");
    expect(homeStrip({ ...view, crews: [team] }, [], context)).toBe("crew");
    // Answered, closed here, or the player's own: the next line.
    expect(
      homeStrip({ ...view, crews: [team], calls: [call({ myAnswer: "soon" })] }, [], context),
    ).toBe("crew");
    expect(
      homeStrip({ ...view, crews: [team], calls: [call()] }, [], {
        ...context,
        closedCalls: ["c1"],
      }),
    ).toBe("crew");
    expect(homeStrip({ ...view, crews: [team], calls: [call({ mine: true })] }, [], context)).toBe(
      "crew",
    );
    // Run out: gone.
    expect(
      homeStrip({ ...view, crews: [team], calls: [call({ endsAt: NOW / 1000 })] }, [], context),
    ).toBe("crew");
  });

  it("waits while a game is open, and says one crewmate on Projects like a friend", () => {
    const team = crew([member("1", "Ayşe", onProjects), member("2", "Bora", onProjects)]);
    expect(
      homeStrip({ ...view, crews: [team], calls: [call()] }, [], { ...context, gameBusy: true }),
    ).toBe("playing");
    const alone = crew([member("1", "Ayşe", onProjects), member("2", "Bora")]);
    expect(homeStrip({ ...view, crews: [alone] }, [], context)).toBe("playing");
    // Someone blocked is not on Projects for the player.
    const blocked = crew([
      member("1", "Ayşe", onProjects),
      member("2", "Bora", { ...onProjects, blocked: true }),
    ]);
    expect(homeStrip({ ...view, crews: [blocked] }, [], context)).toBe("playing");
  });

  it("counts crewmates on Projects across several crews once each", () => {
    const first = crew([member("1", "Ayşe", onProjects)]);
    const second = crew([member("1", "Ayşe", onProjects), member("3", "Cem")], {
      id: "fedcba9876543210",
    });
    // Ayşe in both crews is one person: no crew line yet.
    expect(homeStrip({ ...view, crews: [first, second] }, [], context)).toBe("playing");
    const third = crew([member("2", "Bora", onProjects)], { id: "1111222233334444" });
    expect(homeStrip({ ...view, crews: [first, second, third] }, [], context)).toBe("crew");
  });

  it("offers a call in game where one may start, after calls and the crew line", () => {
    const quiet = crew([member("1", "Ayşe"), member("2", "Bora")]);
    const playing = { ...context, gameBusy: true, callable: true };
    expect(homeStrip({ ...view, crews: [quiet] }, [], playing)).toBe("callCrew");
    // Not where a call may start (or nothing to call): the usual line.
    expect(homeStrip({ ...view, crews: [quiet] }, [], { ...playing, callable: false })).toBe("add");
    expect(homeStrip({ ...view, crews: [] }, [], playing)).toBe("add");
    // Out of the game the call and the crew line come first.
    const busy = crew([member("1", "Ayşe", onProjects), member("2", "Bora", onProjects)]);
    expect(homeStrip({ ...view, crews: [busy] }, [], { ...context, callable: true })).toBe("crew");
  });

  it("ignores the crew while crews are off", () => {
    const team = crew([member("1", "Ayşe", onProjects), member("2", "Bora", onProjects)]);
    expect(homeStrip({ ...view, teams: false, crews: [team], calls: [call()] }, [], context)).toBe(
      "add",
    );
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
    expect(playingPeople({ friends, teams: true, crews: [team] }, []).map((p) => p.id)).toEqual([
      "1",
      "2",
    ]);
    expect(playingPeople({ friends, teams: false, crews: [team] }, []).map((p) => p.id)).toEqual([
      "1",
    ]);
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

  it("keeps a call quiet where every crew it names is muted on its own", () => {
    const at = NOW / 1000;
    const loud = crew([member("1", "Ayşe")], { id: "a" });
    const quiet = crew([member("1", "Ayşe")], {
      id: "b",
      mute: { muted: true, muteUntil: at + 600 },
    });
    const both = { kind: "crewCall" as const, call: call({ crews: ["a", "b"] }) };
    const onlyQuiet = { kind: "crewCall" as const, call: call({ crews: ["b"] }) };
    expect(wantsNotice(both, "online", prefs, undefined, NOW, [loud, quiet])).toBe(true);
    expect(wantsNotice(onlyQuiet, "online", prefs, undefined, NOW, [loud, quiet])).toBe(false);
    // The mute ends by itself.
    expect(wantsNotice(onlyQuiet, "online", prefs, undefined, NOW + 600_000, [loud, quiet])).toBe(
      true,
    );
    // A crewmate is heard through any crew shared with them that is not muted.
    const asked = { ...prefs, notifyCrewProjects: true };
    const joined: SocialNotice = { kind: "crewmateOnProjects", id: "1" };
    const mates = { ...asked, notifyFavoriteProjects: false };
    expect(wantsNotice(joined, "online", mates, undefined, NOW, [loud, quiet])).toBe(true);
    expect(wantsNotice(joined, "online", mates, undefined, NOW, [quiet])).toBe(false);
    // The gold dot: the newest call of a crew not muted on its own.
    const older = call({ id: "old", crews: ["a"], startedAt: at - 300 });
    const newer = call({ id: "new", crews: ["b"], startedAt: at - 30 });
    expect(
      whatWaits(
        { incoming: [], invites: [], calls: [older, newer], crews: [loud, quiet] },
        true,
        NOW,
      ).call?.id,
    ).toBe("old");
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
