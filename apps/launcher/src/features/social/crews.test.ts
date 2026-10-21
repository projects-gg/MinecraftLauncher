import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import type { CrewCall, SocialTeam, TeamMember } from "@/lib/api/social-types";
import {
  busiestCrew,
  callableCrews,
  callerIn,
  callLineKey,
  callLineShown,
  callSentenceOf,
  callWhere,
  canAddCrew,
  crewFirst,
  crewsWithCall,
  inviteTargets,
  moveCrew,
  NEW_CREW_TAB,
  refusalNow,
  shownCrew,
} from "./crews";

const NOW = 1_792_521_000_000;
const at = NOW / 1000;

function member(id: string, extra: Partial<TeamMember> = {}): TeamMember {
  return {
    id,
    name: `p${id}`,
    username: `p${id}`,
    avatar: "discord",
    role: "member",
    moderator: false,
    me: false,
    friend: false,
    blocked: false,
    state: "offline",
    dnd: false,
    discordUrl: "",
    joinedAt: 0,
    ...extra,
  };
}

const onProjects = { state: "playing" as const, game: { where: "projects" as const } };

function crew(id: string, members: TeamMember[] = [], extra: Partial<SocialTeam> = {}): SocialTeam {
  return {
    id,
    name: `Crew ${id}`,
    tag: id.toUpperCase(),
    emblem: "sword",
    color: "gold",
    frame: "cut",
    motto: "",
    code: "EKP-AAAA-BBBB",
    createdOn: "2026-09-20",
    role: "member",
    moderator: false,
    permissions: { invite: "members", call: "members", kick: "moderators", edit: "leader" },
    members: [member("me", { me: true }), ...members],
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
    startedAt: at - 60,
    endsAt: at + 600,
    answers: [],
    ...extra,
  };
}

/** Keys and their values, so the sentence's choice shows. */
const t = ((key: string, values?: Record<string, unknown>) =>
  values ? `${key} ${JSON.stringify(values)}` : key) as unknown as TFunction;

const features = { calls: true, roles: true };

describe("the crew page with several crews", () => {
  const a = crew("a");
  const b = crew("b");

  it("shows the crew picked, else the primary one, and the add view when asked or crewless", () => {
    expect(shownCrew([a, b], "b")).toBe(b);
    expect(shownCrew([a, b], null)).toBe(a);
    // A crew the player left: back to the primary one.
    expect(shownCrew([a, b], "gone")).toBe(a);
    expect(shownCrew([a, b], NEW_CREW_TAB)).toBe(NEW_CREW_TAB);
    expect(shownCrew([], null)).toBe(NEW_CREW_TAB);
  });

  it("founds or joins one more below the limit, and with one crew per player only crewless", () => {
    expect(canAddCrew([a, b], 5, true)).toBe(true);
    expect(canAddCrew([a, b], 2, true)).toBe(false);
    // A limit of 0 is never less than one crew.
    expect(canAddCrew([], 0, true)).toBe(true);
    expect(canAddCrew([a], 5, false)).toBe(false);
    expect(canAddCrew([], 1, false)).toBe(true);
  });

  it("moves a crew one place, or first, and nowhere past the ends", () => {
    expect(moveCrew(["a", "b", "c"], "b", -1)).toEqual(["b", "a", "c"]);
    expect(moveCrew(["a", "b", "c"], "b", 1)).toEqual(["a", "c", "b"]);
    expect(moveCrew(["a", "b", "c"], "a", -1)).toBeNull();
    expect(moveCrew(["a", "b", "c"], "c", 1)).toBeNull();
    expect(moveCrew(["a", "b"], "z", 1)).toBeNull();
    expect(crewFirst(["a", "b", "c"], "c")).toEqual(["c", "a", "b"]);
    expect(crewFirst(["a", "b", "c"], "a")).toBeNull();
    expect(crewFirst(["a", "b"], "z")).toBeNull();
  });

  it("names on Home the crew with the most crewmates on Projects, ties to the player's order", () => {
    const one = crew("a", [member("1", onProjects)]);
    const two = crew("b", [member("2", onProjects), member("3", onProjects)]);
    const tie = crew("c", [member("4", onProjects), member("5", onProjects)]);
    expect(busiestCrew([one, two, tie])?.id).toBe("b");
    expect(busiestCrew([crew("a"), crew("b")])).toBeUndefined();
  });
});

describe("calls with several crews", () => {
  const lead = crew("a", [member("1", { name: "Ayşe" })], { role: "founder" });
  const plain = crew("b", [member("2", { name: "Bora" })], {
    permissions: { invite: "members", call: "leader", kick: "moderators", edit: "leader" },
  });

  it("calls only crews whose permissions let the player", () => {
    expect(callableCrews([lead, plain], features).map((c) => c.id)).toEqual(["a"]);
    // A service without calls: none.
    expect(callableCrews([lead, plain], { calls: false, roles: true })).toEqual([]);
  });

  it("knows the crews with someone else's call on", () => {
    const calls = [
      call({ crews: ["b"] }),
      call({ id: "mine", mine: true, crews: ["a"] }),
      call({ id: "over", crews: ["a"], endsAt: at }),
    ];
    expect([...crewsWithCall(calls, [lead, plain], NOW)]).toEqual(["b"]);
  });

  it("finds the caller in any crew, else takes what the call says", () => {
    expect(callerIn(call({ from: "2" }), [lead, plain]).name).toBe("Bora");
    expect(callerIn(call({ from: "9", name: "Cem" }), [lead, plain]).name).toBe("Cem");
    expect(callerIn(call({ from: "9" }), [lead]).name).toBe("");
  });

  it("names the crews a call is for only where the player has several", () => {
    const third = crew("c");
    const from = { from: "1" };
    expect(callSentenceOf(call(from), [lead], t)).toBe('teams.call.theirs {"name":"Ayşe"}');
    expect(callSentenceOf(call({ ...from, crews: ["b"] }), [lead, plain], t)).toBe(
      'crews.call.theirsOne {"name":"Ayşe","crew":"Crew b"}',
    );
    expect(callSentenceOf(call({ ...from, crews: ["b", "a"] }), [lead, plain], t)).toBe(
      'crews.call.theirsTwo {"name":"Ayşe","first":"Crew a","second":"Crew b"}',
    );
    expect(callSentenceOf(call({ ...from, crews: ["a", "b", "c"] }), [lead, plain, third], t)).toBe(
      'crews.call.theirsMany {"name":"Ayşe","count":3}',
    );
    // On a crew's own page the crew goes without saying.
    expect(callSentenceOf(call({ ...from, crews: ["b"] }), [lead, plain], t, true)).toBe(
      'teams.call.theirs {"name":"Ayşe"}',
    );
    expect(callSentenceOf(call({ from: "9" }), [lead], t)).toBe(
      'teams.call.theirs {"name":"teams.call.someone"}',
    );
  });

  it("takes the player where the call is: Projects, another server, or nowhere (older calls)", () => {
    expect(callWhere(call())).toEqual({ kind: "projects" });
    expect(callWhere(call({ server: { where: "projects", host: "play.projects.gg" } }))).toEqual({
      kind: "projects",
    });
    expect(callWhere(call({ server: { where: "other", host: "mc.example.com" } }))).toEqual({
      kind: "server",
      host: "mc.example.com",
    });
    // A projects.gg address said as another server is still Projects.
    expect(callWhere(call({ server: { where: "other", host: "eu.projects.gg" } }))).toEqual({
      kind: "projects",
    });
    expect(callWhere(call({ server: { where: "other" } }))).toEqual({ kind: "ready" });
    expect(callWhere(call({ kind: "ready" }))).toEqual({ kind: "ready" });
  });

  it("keeps the reason under the button while the place still says no", () => {
    expect(refusalNow(null, "needsServer")).toBeNull();
    expect(refusalNow({ reason: "needsServer", place: "needsServer" }, "needsServer")).toBe(
      "needsServer",
    );
    // The player moved on to another server with its address hidden: that reason now.
    expect(refusalNow({ reason: "needsServer", place: "needsServer" }, "serverHidden")).toBe(
      "serverHidden",
    );
    // Now on a server a call may start from: the line goes.
    expect(refusalNow({ reason: "needsServer", place: "needsServer" }, "ok")).toBeNull();
    // The service said no where the launcher saw no reason: said until the place changes.
    expect(refusalNow({ reason: "serverHidden", place: "ok" }, "ok")).toBe("serverHidden");
  });
});

describe("Home's call line", () => {
  const mine = crew("a", [member("1")]);
  const base = {
    place: "ok" as const,
    activity: { state: "playing" as const, host: "play.projects.gg" },
    closedOn: null,
    crews: [mine],
    features,
    calls: [] as CrewCall[],
    refusedUntil: 0,
    lastCallAt: null,
    now: NOW,
  };

  it("shows in game where a call may start, with a crew to call", () => {
    expect(callLineShown(base)).toBe(true);
    expect(callLineShown({ ...base, place: "needsServer" })).toBe(false);
    expect(callLineShown({ ...base, place: "serverHidden" })).toBe(false);
    expect(callLineShown({ ...base, crews: [] })).toBe(false);
    expect(callLineShown({ ...base, features: { calls: false, roles: true } })).toBe(false);
  });

  it("waits while the player's call is on or its five minutes run, and while the crew has a call", () => {
    expect(callLineShown({ ...base, calls: [call({ mine: true })] })).toBe(false);
    expect(callLineShown({ ...base, lastCallAt: NOW - 60_000 })).toBe(false);
    expect(callLineShown({ ...base, lastCallAt: NOW - 5 * 60_000 })).toBe(true);
    expect(callLineShown({ ...base, refusedUntil: NOW + 1 })).toBe(false);
    expect(callLineShown({ ...base, calls: [call({ crews: ["a"] })] })).toBe(false);
  });

  it("stays closed on the server it was closed on", () => {
    const key = callLineKey(base.activity);
    expect(key).toBe("play.projects.gg");
    expect(callLineShown({ ...base, closedOn: key })).toBe(false);
    const elsewhere = { state: "playing" as const, host: "mc.example.com" };
    expect(callLineShown({ ...base, activity: elsewhere, closedOn: key })).toBe(true);
    expect(callLineKey({ state: "launcher" })).toBeNull();
    expect(callLineKey({ state: "playing" })).toBe("projects");
  });
});

describe("inviting a friend with several crews", () => {
  it("says for each crew the player may invite to how it stands", () => {
    const full = crew("a", [member("2")], { limit: 2 });
    const has = crew("b", [member("7")]);
    const asked = crew("c", [], { invited: ["7"] });
    const open = crew("d");
    const closed = crew("e", [], {
      permissions: { invite: "leader", call: "members", kick: "moderators", edit: "leader" },
    });
    expect(
      inviteTargets([full, has, asked, open, closed], "7", features).map(
        ({ crew: c, state }) => `${c.id}:${state}`,
      ),
    ).toEqual(["a:full", "b:member", "c:invited", "d:invitable"]);
  });
});
