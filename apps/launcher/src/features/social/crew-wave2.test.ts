// The crews' second wave (TEAM5) in the friends model: Home's plan line among the others, join
// requests behind the gold dot, which of its notices are said, and its switches in the backup.
import { describe, expect, it } from "vitest";
import type {
  CrewCall,
  CrewPlan,
  Friend,
  FriendRequest,
  JoinRequest,
  SocialInvite,
  SocialNotice,
  SocialTeam,
  TeamMember,
} from "@/lib/api/social-types";
import {
  buildBackup,
  homeStrip,
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
    role: "founder",
    moderator: false,
    permissions: {
      invite: "members",
      call: "members",
      kick: "moderators",
      edit: "leader",
      admit: "moderators",
    },
    members: [member("100", "Ben", { me: true }), ...members],
    invited: [],
    limit: 30,
    mute: { muted: false, muteUntil: null },
    join: "approval",
    requests: [],
    togetherWeek: 0,
    ...extra,
  };
}

const request = (id: string, sentAt = NOW / 1000 - 600): JoinRequest => ({
  id,
  name: `p${id}`,
  username: `p${id}`,
  avatar: "discord",
  sentAt,
});

const plan = (extra: Partial<CrewPlan> = {}): CrewPlan => ({
  id: "p1",
  crew: "0123456789abcdef",
  by: "1",
  byName: "Ayşe",
  mine: false,
  canCancel: false,
  at: NOW / 1000 + 1800,
  startsAt: NOW / 1000 + 1800,
  endsAt: NOW / 1000 + 9000,
  createdAt: NOW / 1000 - 3600,
  answers: [],
  ...extra,
});

describe("Home's plan line", () => {
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
  const busy = crew([member("1", "Ayşe", onProjects), member("2", "Bora", onProjects)]);

  it("puts a plan under way before crewmates on Projects, one within the hour after them", () => {
    expect(homeStrip({ ...view, crews: [busy] }, [], { ...context, plan: "live" })).toBe("plan");
    expect(homeStrip({ ...view, crews: [busy] }, [], { ...context, plan: "soon" })).toBe("crew");
    expect(homeStrip({ ...view, crews: [crew([])] }, [], { ...context, plan: "soon" })).toBe(
      "plan",
    );
  });

  it("waits while a game is open, and never comes without a crew", () => {
    expect(
      homeStrip({ ...view, crews: [crew([])] }, [], { ...context, plan: "live", gameBusy: true }),
    ).not.toBe("plan");
    expect(homeStrip({ ...view, crews: [] }, [], { ...context, plan: "live" })).not.toBe("plan");
  });
});

describe("join requests behind the gold dot", () => {
  const view = { incoming: [], invites: [], calls: [] };

  it("count what waits in crews the player answers, muted crews left out", () => {
    const team = crew([], { requests: [request("1"), request("2", NOW / 1000 - 8 * 86_400)] });
    expect(whatWaits({ ...view, crews: [team] }, true, NOW).joins).toBe(1);
    expect(somethingWaits({ ...view, crews: [team] }, true, NOW)).toBe(true);
    const muted = { ...team, mute: { muted: true, muteUntil: null } };
    expect(whatWaits({ ...view, crews: [muted] }, true, NOW).joins).toBe(0);
    expect(somethingWaits({ ...view, crews: [crew([])] }, true, NOW)).toBe(false);
  });
});

describe("the second wave's notices", () => {
  const prefs = { favorites: [], notifyFavoriteOnline: true, notifyFavoriteProjects: true };
  const team = crew([]);
  const asked: SocialNotice = { kind: "joinRequest", crew: team.id, request: request("1") };
  const planned: SocialNotice = { kind: "crewPlan", plan: plan() };

  it("say a request unless its crew is muted, do-not-disturb is on or the switch is off", () => {
    expect(wantsNotice(asked, "online", prefs, undefined, NOW, [team])).toBe(true);
    expect(
      wantsNotice(asked, "online", { ...prefs, notifyJoinRequests: false }, undefined, NOW, [team]),
    ).toBe(false);
    expect(wantsNotice(asked, "dnd", prefs, undefined, NOW, [team])).toBe(false);
    const muted = { ...team, mute: { muted: true, muteUntil: null } };
    expect(wantsNotice(asked, "online", prefs, undefined, NOW, [muted])).toBe(false);
    const allMuted = { calls: true, muted: true, muteUntil: null };
    expect(wantsNotice(asked, "online", prefs, allMuted, NOW, [team])).toBe(false);
  });

  it("say a crewmate's new plan unless the crew is muted; a taken request always", () => {
    expect(wantsNotice(planned, "online", prefs, undefined, NOW, [team])).toBe(true);
    const muted = { ...team, mute: { muted: true, muteUntil: NOW / 1000 + 60 } };
    expect(wantsNotice(planned, "online", prefs, undefined, NOW, [muted])).toBe(false);
    const taken: SocialNotice = { kind: "requestAccepted", crew: team.id, name: team.name };
    expect(wantsNotice(taken, "online", prefs, undefined, NOW, [])).toBe(true);
  });
});

describe("the second wave's switches in the backup", () => {
  it("travel when set and are read back", () => {
    const friends = {
      favorites: [],
      notifyFavoriteOnline: true,
      notifyFavoriteProjects: true,
      notifyJoinRequests: false,
      notifyPlans: true,
    };
    const document = buildBackup({}, friends);
    expect(document.friends.notifyJoinRequests).toBe(false);
    expect(document.friends.notifyPlans).toBe(true);
    const read = readBackup(document);
    expect(read.friends.notifyJoinRequests).toBe(false);
    expect(read.friends.notifyPlans).toBe(true);
    expect(readBackup({ friends: { notifyPlans: "yes" } }).friends.notifyPlans).toBeUndefined();
  });
});
