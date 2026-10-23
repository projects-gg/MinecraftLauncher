import { describe, expect, it } from "vitest";
import type {
  HostSession,
  HostView,
  JoinStage,
  WorldGuest,
  WorldInvite,
} from "@/lib/api/host-types";
import type { Friend, SocialTeam, TeamMember } from "@/lib/api/social-types";
import type { LaunchPhase } from "@/stores/launch";
import {
  defaultMode,
  failureWords,
  groupChoice,
  guestOrder,
  heldPlaces,
  homeInvite,
  hostMenuKey,
  hostRowAction,
  inviteFrom,
  inviteKeys,
  inviteList,
  invitePeople,
  joinActive,
  joinButton,
  personWorld,
  pickerCount,
  pickerGroups,
  retryWithCopy,
  sessionPeople,
  toggleGroup,
  toggleOne,
  waitingAsk,
} from "./rules";

function friend(id: string, name: string, state: Friend["state"] = "launcher"): Friend {
  return {
    id,
    name,
    username: name.toLowerCase(),
    avatar: "discord",
    state,
    dnd: false,
    discordUrl: `https://discord.com/users/${id}`,
  };
}

function member(id: string, extra: Partial<TeamMember> = {}): TeamMember {
  return {
    id,
    name: id,
    username: id,
    avatar: "discord",
    role: "member",
    moderator: false,
    me: false,
    friend: true,
    blocked: false,
    state: "launcher",
    dnd: false,
    discordUrl: "",
    joinedAt: 0,
    togetherMonth: 0,
    ...extra,
  } as TeamMember;
}

function crew(id: string, members: TeamMember[]): SocialTeam {
  return { id, name: id, members } as unknown as SocialTeam;
}

const guest = (friendId: string, state: WorldGuest["state"]): WorldGuest => ({
  friend: friendId,
  name: friendId,
  state,
});

function invite(extra: Partial<WorldInvite> = {}): WorldInvite {
  return {
    session: "s1",
    from: "303",
    name: "Mert",
    mc: "26.1.2",
    guests: 1,
    max: 7,
    at: 0,
    quiet: false,
    ...extra,
  };
}

const openSession = (guests: WorldGuest[], max = 7): HostSession => ({
  phase: "open",
  instanceId: "projects",
  folder: "Yeni Dünya",
  worldName: "Yeni Dünya",
  mode: "survival",
  commands: false,
  guests,
  max,
});

const NONE = new Set<string>();

describe("counts", () => {
  it("count the host in, as 2/8", () => {
    expect(invitePeople({ guests: 1, max: 7 })).toEqual({ count: 2, max: 8 });
    expect(
      sessionPeople(openSession([guest("a", "in"), guest("b", "joining"), guest("c", "in")])),
    ).toEqual({ count: 3, max: 8 });
  });

  it("hold a place for everyone picked, invited, joining or in", () => {
    expect(heldPlaces(null)).toBe(0);
    expect(heldPlaces(openSession([guest("a", "in"), guest("b", "invited")]))).toBe(2);
  });

  it("list guests in the world first, then on their way, invited and waiting", () => {
    const order = guestOrder([
      guest("w", "pending"),
      guest("i", "invited"),
      guest("x", "in"),
      guest("j", "joining"),
      guest("y", "in"),
    ]).map((g) => g.friend);
    expect(order).toEqual(["x", "y", "j", "i", "w"]);
  });
});

describe("the invite window", () => {
  const ayse = friend("1", "Ayşe", "playing");
  const mert = friend("2", "Mert", "launcher");
  const elif = friend("3", "Elif", "playing");
  const bora = friend("4", "Bora", "offline");
  const cem = friend("5", "Cem", "launcher");

  it("opens in the world's own mode, survival for hardcore and spectator worlds", () => {
    expect(defaultMode({ gameMode: "creative", hardcore: false })).toBe("creative");
    expect(defaultMode({ gameMode: "adventure", hardcore: false })).toBe("adventure");
    expect(defaultMode({ gameMode: "survival", hardcore: true })).toBe("survival");
    expect(defaultMode({ gameMode: "spectator", hardcore: false })).toBe("survival");
    expect(defaultMode({ gameMode: "unknown", hardcore: false })).toBe("survival");
  });

  it("lists online friends, in game first, crews first and each person once", () => {
    const groups = pickerGroups({
      friends: [ayse, mert, elif, bora, cem],
      crews: [
        crew("c1", [member("1"), member("2"), member("me", { me: true })]),
        crew("c2", [member("2"), member("3"), member("9", { friend: false })]),
      ],
      blocked: NONE,
      taken: NONE,
      query: "",
    });
    expect(groups.map((g) => [g.key, g.people.map((p) => p.name)])).toEqual([
      ["crew:c1", ["Ayşe", "Mert"]],
      ["crew:c2", ["Elif"]],
      ["friends", ["Cem"]],
    ]);
    expect(pickerCount(groups)).toBe(4);
  });

  it("keeps blocked people out, and offline ones unless they hold a place", () => {
    const groups = pickerGroups({
      friends: [ayse, mert, bora],
      crews: [],
      blocked: new Set(["2"]),
      taken: new Set(["4"]),
      query: "",
    });
    expect(groups[0]?.people.map((p) => p.name)).toEqual(["Ayşe", "Bora"]);
  });

  it("finds people by name or username, Turkish letters folded", () => {
    const groups = pickerGroups({
      friends: [ayse, mert, cem],
      crews: [],
      blocked: NONE,
      taken: NONE,
      query: "ayse",
    });
    expect(groups[0]?.people.map((p) => p.name)).toEqual(["Ayşe"]);
  });

  it("ticks a crew's people while places are left, then clears them", () => {
    const people = [ayse, mert, elif];
    const some = toggleGroup(people, new Set(["9"]), NONE, 3);
    expect([...some]).toEqual(["9", "1", "2"]);
    expect(groupChoice(people, some, NONE)).toBe("all");
    const all = toggleGroup(people, new Set(), NONE, 7);
    expect(groupChoice(people, all, NONE)).toBe("clear");
    expect([...toggleGroup(people, all, NONE, 7)]).toEqual([]);
  });

  it("leaves people already in the session fixed", () => {
    const taken = new Set(["1"]);
    expect(groupChoice([ayse], new Set(), taken)).toBe("none");
    expect([...toggleGroup([ayse, mert], new Set(), taken, 7)]).toEqual(["2"]);
  });

  it("does not tick past the room", () => {
    expect([...toggleOne(new Set(["1", "2"]), "3", 2)]).toEqual(["1", "2"]);
    expect([...toggleOne(new Set(["1", "2"]), "2", 2)]).toEqual(["1"]);
  });
});

describe("a world's row", () => {
  const base = {
    enabled: true,
    place: { shown: true },
    session: null,
    instanceId: "projects",
    folder: "Yeni Dünya",
    locked: false,
    readable: true,
  };

  it("offers playing with friends only where the place shows it", () => {
    expect(hostRowAction(base)).toEqual({ kind: "play" });
    expect(hostRowAction({ ...base, place: { shown: false } })).toEqual({ kind: "hidden" });
    expect(hostRowAction({ ...base, enabled: false })).toEqual({ kind: "hidden" });
    expect(hostRowAction({ ...base, readable: false })).toEqual({ kind: "hidden" });
  });

  it("waits while the game runs, unless it runs this very world", () => {
    expect(hostRowAction({ ...base, locked: true })).toEqual({ kind: "play", wait: "locked" });
    expect(
      hostRowAction({ ...base, locked: true, place: { shown: true, playing: "Yeni Dünya" } }),
    ).toEqual({ kind: "play" });
  });

  it("shows this world's session and keeps the others waiting", () => {
    const session = openSession([]);
    expect(hostRowAction({ ...base, session })).toEqual({ kind: "open" });
    expect(hostRowAction({ ...base, session, folder: "Başka" })).toEqual({
      kind: "play",
      wait: "busy",
    });
    const opening: HostSession = { ...session, phase: "opening", step: "backup", percent: 40 };
    expect(hostRowAction({ ...base, session: opening })).toEqual({
      kind: "opening",
      step: "backup",
      percent: 40,
    });
    const failed: HostSession = { ...session, phase: "failed", reason: "limit" };
    expect(hostRowAction({ ...base, session: failed })).toEqual({
      kind: "failed",
      reason: "limit",
    });
    // A failed opening of another world does not hold this one back.
    expect(hostRowAction({ ...base, session: failed, folder: "Başka" })).toEqual({
      kind: "play",
    });
  });
});

describe("Katıl", () => {
  const idle = { phase: { kind: "idle" } as LaunchPhase, instanceId: null };
  const ready: JoinStage = { kind: "ready", instanceId: "projects", port: 1, missing: 0 };
  const launching = (phase: LaunchPhase) => ({ phase, instanceId: "projects" });

  it("says where it is, in place", () => {
    expect(joinButton(undefined, idle)).toEqual({ kind: "join" });
    expect(joinButton({ kind: "preparing", percent: 30 }, idle)).toEqual({
      kind: "busy",
      step: "preparing",
      percent: 30,
    });
    expect(joinButton({ kind: "connecting" }, idle)).toEqual({ kind: "busy", step: "connecting" });
    expect(joinButton({ kind: "in" }, idle)).toEqual({ kind: "in" });
    expect(joinButton({ kind: "lost" }, idle)).toEqual({ kind: "rejoin" });
    expect(joinButton({ kind: "failed", failure: "full" }, idle)).toEqual({ kind: "retry" });
  });

  it("follows the launch once ready", () => {
    expect(joinButton(ready, idle)).toEqual({ kind: "join" });
    expect(joinButton(ready, launching({ kind: "launching" }))).toEqual({
      kind: "busy",
      step: "opening",
    });
    const shown: LaunchPhase = { kind: "running", since: 0, shown: true, firstStart: false };
    expect(joinButton(ready, launching(shown))).toEqual({ kind: "busy", step: "connecting" });
    // Another instance's game (the "Bağlan" question is up): "Katıl" again.
    expect(joinButton(ready, { phase: shown, instanceId: "other" })).toEqual({ kind: "join" });
  });

  it("is under way until it ends", () => {
    expect(joinActive(undefined)).toBe(false);
    expect(joinActive({ kind: "preparing" })).toBe(true);
    expect(joinActive({ kind: "in" })).toBe(true);
    expect(joinActive({ kind: "lost" })).toBe(false);
    expect(joinActive({ kind: "failed", failure: "full" })).toBe(false);
  });

  it("tries again with a copy after missing mods", () => {
    expect(retryWithCopy({ kind: "failed", failure: "mods" })).toBe(true);
    expect(retryWithCopy({ kind: "failed", failure: "full" })).toBe(false);
    expect(retryWithCopy(undefined)).toBe(false);
  });

  it("asks one question at a time, the oldest first", () => {
    const copy = invite({
      session: "old",
      join: {
        kind: "ask",
        ask: { kind: "copy", mods: 2, bytes: 1, unknown: 0 },
      },
    });
    const name = invite({
      session: "new",
      join: { kind: "ask", ask: { kind: "name", verdict: "own", name: "Steve" } },
    });
    expect(waitingAsk([name, copy])?.session).toBe("old");
    expect(waitingAsk([invite()])).toBeNull();
  });

  it("words a failure and offers its one action", () => {
    expect(failureWords("unreachable")).toEqual({
      key: "host.join.failed.unreachable",
      action: "retry",
    });
    expect(failureWords("mods").action).toBe("copy");
    expect(failureWords("removed").action).toBeNull();
  });
});

describe("where invites show", () => {
  it("finds a friend's invite and Home's line", () => {
    const quiet = invite({ session: "q", from: "1", quiet: true });
    const live = invite({ session: "l", from: "2" });
    expect(inviteFrom([quiet, live], "2")?.session).toBe("l");
    expect(homeInvite([quiet, live], [])?.session).toBe("l");
    expect(homeInvite([quiet, live], ["l"])).toBeNull();
    expect(homeInvite([invite({ join: { kind: "in" } })], [])).toBeNull();
  });
});

describe("keys that keep lists and menus still", () => {
  it("read back the invites a list shows, names with spaces included", () => {
    const keys = inviteKeys([
      invite({ session: "a", name: "Elif Su" }),
      invite({ session: "b", name: "Mert" }),
    ]);
    expect(inviteList(keys)).toEqual([
      { session: "a", name: "Elif Su" },
      { session: "b", name: "Mert" },
    ]);
    expect(inviteList("")).toEqual([]);
    // A join's progress does not change them.
    expect(inviteKeys([invite({ join: { kind: "preparing", percent: 10 } })])).toBe(
      inviteKeys([invite({ join: { kind: "preparing", percent: 80 } })]),
    );
  });

  it("change the person menu's key only when what it shows changes", () => {
    const base: HostView = {
      seq: 1,
      enabled: true,
      maxGuests: 7,
      session: openSession([guest("1", "invited")]),
      invites: [invite({ join: { kind: "preparing", percent: 10 } })],
    };
    const later = { ...base, invites: [invite({ join: { kind: "preparing", percent: 90 } })] };
    expect(hostMenuKey(later)).toBe(hostMenuKey(base));
    // A guest who comes in changes nothing the menu shows either; one more guest does.
    expect(hostMenuKey({ ...base, session: openSession([guest("1", "in")]) })).toBe(
      hostMenuKey(base),
    );
    expect(
      hostMenuKey({ ...base, session: openSession([guest("1", "in"), guest("2", "invited")]) }),
    ).not.toBe(hostMenuKey(base));
    expect(hostMenuKey({ ...base, invites: [invite()] })).not.toBe(hostMenuKey(base));
    expect(hostMenuKey({ ...base, enabled: false })).toBe("");
  });
});

describe("the person menu", () => {
  const view = (patch: Partial<HostView>): HostView => ({
    seq: 1,
    enabled: true,
    maxGuests: 7,
    session: null,
    invites: [],
    ...patch,
  });

  it("joins a friend's world", () => {
    expect(personWorld(view({ invites: [invite()] }), "303", true)).toEqual({ join: "ready" });
    expect(
      personWorld(view({ invites: [invite({ join: { kind: "preparing" } })] }), "303", true),
    ).toEqual({ join: "here" });
  });

  it("invites friends to the open world and removes its guests", () => {
    const session = openSession([guest("1", "in")], 2);
    expect(personWorld(view({ session }), "1", true)).toEqual({
      host: { guest: true, invite: "invited" },
    });
    expect(personWorld(view({ session }), "2", true)).toEqual({
      host: { guest: false, invite: "ready" },
    });
    // Not a friend and not a guest: nothing about the world.
    expect(personWorld(view({ session }), "9", false)).toEqual({});
    const full = openSession([guest("1", "in"), guest("2", "invited")], 2);
    expect(personWorld(view({ session: full }), "3", true).host?.invite).toBe("full");
  });

  it("says nothing while the feature is off or the world is not open", () => {
    expect(personWorld(view({ enabled: false, invites: [invite()] }), "303", true)).toEqual({});
    const opening: HostSession = { ...openSession([]), phase: "opening", step: "starting" };
    expect(personWorld(view({ session: opening }), "1", true)).toEqual({});
  });
});
