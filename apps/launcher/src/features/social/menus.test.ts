import { describe, expect, it } from "vitest";
import type { TeamMember } from "@/lib/api/social-types";
import {
  callMenu,
  categoryMenu,
  leavingOf,
  type PersonInput,
  type Plan,
  panelMenu,
  personMenu,
  planMenu,
  teamMenu,
  without,
} from "./menus";

/** A plan as words: actions, `-` for separators, `(off)` for unavailable rows, `✓` for checks. */
function words<A extends string>(plan: Plan<A>): string[] {
  return plan.map((row) =>
    row === null ? "-" : `${row.action}${row.disabled ? "(off)" : ""}${row.checked ? "✓" : ""}`,
  );
}

const member = (id: string): TeamMember => ({
  id,
  name: id,
  username: id,
  avatar: "discord",
  role: "member",
  moderator: false,
  me: false,
  friend: true,
  blocked: false,
  state: "offline",
  dnd: false,
  discordUrl: `https://discord.com/users/${id}`,
  joinedAt: 0,
});

describe("panelMenu", () => {
  const base = {
    categories: false,
    categoryCount: 0,
    hasFriends: true,
    teams: false,
    inTeam: false,
    hasCode: true,
  };

  it("offers adding a friend, the code and the status on an older service", () => {
    expect(words(panelMenu(base))).toEqual(["addFriend", "copyCode", "-", "status"]);
  });

  it("adds categories and teams only when the service offers them", () => {
    expect(words(panelMenu({ ...base, categories: true, teams: true }))).toEqual([
      "addFriend",
      "copyCode",
      "-",
      "newCategory",
      "createTeam",
      "-",
      "status",
    ]);
    expect(words(panelMenu({ ...base, teams: true, inTeam: true }))).toContain("openTeam");
    expect(words(panelMenu({ ...base, teams: true, inTeam: true }))).not.toContain("createTeam");
  });

  it("founds one more crew and calls several where the player has several", () => {
    const crews = { ...base, teams: true, inTeam: true };
    expect(words(panelMenu({ ...crews, canAddCrew: true, callCrews: "ready" }))).toEqual([
      "addFriend",
      "copyCode",
      "-",
      "openTeam",
      "createTeam",
      "callCrews",
      "-",
      "status",
    ]);
    expect(words(panelMenu({ ...crews, callCrews: "waiting" }))).toContain("callCrews(off)");
    expect(words(panelMenu({ ...crews, callCrews: "hidden" }))).not.toContain("callCrews");
    // Without a crew there is nothing to call, and "Ekip kur" is the only crew row.
    expect(
      words(panelMenu({ ...base, teams: true, canAddCrew: true, callCrews: "ready" })),
    ).toEqual(["addFriend", "copyCode", "-", "createTeam", "-", "status"]);
  });

  it("offers a category only with friends to put in it", () => {
    expect(words(panelMenu({ ...base, categories: true, hasFriends: false }))).not.toContain(
      "newCategory",
    );
  });

  it("keeps a new category out of reach at the service's limit", () => {
    expect(words(panelMenu({ ...base, categories: true, categoryCount: 20 }))).toContain(
      "newCategory(off)",
    );
    expect(words(panelMenu({ ...base, categories: true, categoryCount: 19 }))).toContain(
      "newCategory",
    );
  });

  it("leaves out the code while it is not known", () => {
    expect(words(panelMenu({ ...base, hasCode: false }))).toEqual(["addFriend", "-", "status"]);
  });
});

describe("personMenu", () => {
  // A friend in the friend list, as the list sees them.
  const friend: PersonInput = {
    friend: true,
    blocked: false,
    joinable: false,
    here: false,
    invite: "ready",
    chat: true,
    discord: true,
    notifyOnline: false,
    categories: true,
    team: null,
    report: true,
  };
  // A crewmate who is not a friend, seen by a plain member.
  const crewmate: PersonInput = {
    friend: false,
    blocked: false,
    discord: true,
    addFriend: "ready",
    crew: { roles: true, leader: false, moderator: false, moderatorsFull: false, removable: false },
    report: true,
  };

  it("keeps one order: together, the friend list, the crew, the relationship", () => {
    expect(words(personMenu({ ...friend, joinable: true, team: "invitable" }))).toEqual([
      "join",
      "invite",
      "chat",
      "message",
      "-",
      "favorite",
      "notifyOnline",
      "category",
      "teamInvite",
      "-",
      "removeFriend",
      "block",
      "report",
    ]);
  });

  it("gives a friend in a crew the leader's crew rows too, in their own group", () => {
    const leader = {
      roles: true,
      leader: true,
      moderator: false,
      moderatorsFull: false,
      removable: true,
    };
    expect(words(personMenu({ ...friend, categories: false, crew: leader }))).toEqual([
      "invite",
      "chat",
      "message",
      "-",
      "favorite",
      "notifyOnline",
      "-",
      "makeModerator",
      "makeFounder",
      "removeMember",
      "-",
      "removeFriend",
      "block",
      "report",
    ]);
    expect(words(personMenu({ ...friend, crew: { ...leader, moderator: true } }))).toContain(
      "unmakeModerator",
    );
    expect(words(personMenu({ ...friend, crew: { ...leader, moderatorsFull: true } }))).toContain(
      "makeModerator(off)",
    );
    // A service without roles: no moderators at all, the leadership still goes on.
    const plain = words(personMenu({ ...friend, crew: { ...leader, roles: false } }));
    expect(plain).not.toContain("makeModerator");
    expect(plain).toContain("makeFounder");
    // A moderator removes where the crew lets them, and nothing more.
    expect(
      words(personMenu({ ...crewmate, crew: { ...leader, leader: false, removable: true } })),
    ).toEqual(["message", "addFriend", "-", "removeMember", "-", "block", "report"]);
  });

  it("offers a crewmate who is not a friend a friend request instead of a message", () => {
    // Direct messages stay between friends: no "Mesaj gönder", no invite to play.
    expect(words(personMenu(crewmate))).toEqual(["message", "addFriend", "-", "block", "report"]);
    expect(words(personMenu({ ...crewmate, addFriend: "sent" }))).toContain("addFriend(off)✓");
    expect(words(personMenu({ ...crewmate, addFriend: "received" }))).toContain("acceptFriend");
    // An older service: nothing to offer but Discord.
    expect(words(personMenu({ ...crewmate, addFriend: "hidden" }))).toEqual([
      "message",
      "-",
      "block",
      "report",
    ]);
  });

  it("offers Katıl on projects.gg, off when the player is there already, never to someone blocked", () => {
    expect(words(personMenu({ ...crewmate, joinable: true }))[0]).toBe("join");
    expect(words(personMenu({ ...crewmate, joinable: true, here: true }))[0]).toBe("join(off)");
    expect(words(personMenu({ ...crewmate, joinable: true, blocked: true }))).toEqual([
      "message",
      "-",
      "unblock",
      "report",
    ]);
  });

  it("shows the invite's wait, and hides the chat in its own conversation", () => {
    expect(words(personMenu({ ...friend, invite: "waiting" }))).toContain("invite(off)");
    expect(words(personMenu({ ...friend, invite: "hidden" }))[0]).toBe("chat");
    expect(words(personMenu({ ...friend, chat: false }))).not.toContain("chat");
    expect(words(personMenu({ ...friend, notifyOnline: true }))).toContain("notifyOnline✓");
    expect(words(personMenu({ ...friend, team: "invited" }))).toContain("teamInvite(off)");
    expect(words(personMenu({ ...friend, team: "full" }))).toContain("teamInvite(off)");
    expect(words(personMenu({ ...friend, team: "member" })).join()).not.toContain("teamInvite");
  });

  it("leads a request's own row with its answers", () => {
    const asker: PersonInput = { friend: false, blocked: false, report: true };
    expect(words(personMenu({ ...asker, request: "incoming" }))).toEqual([
      "acceptRequest",
      "declineRequest",
      "-",
      "block",
      "report",
    ]);
    expect(words(personMenu({ ...asker, request: "join" }))).toEqual([
      "acceptRequest",
      "declineRequest",
      "-",
      "block",
      "report",
    ]);
    // The player's own request: take it back; nothing to report about someone asked.
    expect(words(personMenu({ ...asker, request: "outgoing", report: false }))).toEqual([
      "cancelRequest",
      "-",
      "block",
    ]);
    // A crewmate's request is answered on its row, not offered again.
    expect(
      words(personMenu({ ...crewmate, request: "incoming", addFriend: "received" })),
    ).not.toContain("acceptFriend");
  });

  it("has nothing for the player's own row", () => {
    expect(personMenu({ ...friend, me: true })).toEqual([]);
  });
});

describe("planMenu", () => {
  const base = {
    joinable: false,
    here: false,
    notes: true,
    hasNote: false,
    canWrite: false,
    canClear: false,
    report: true,
    cancel: false,
  };

  it("answers, reports a crewmate's plan and leaves its note to its maker", () => {
    expect(words(planMenu({ ...base, answer: "maybe" }))).toEqual([
      "yes",
      "maybe✓",
      "no",
      "-",
      "report",
    ]);
  });

  it("lets its maker write, change and clear the note before it starts", () => {
    const mine = { ...base, report: false, cancel: true, canWrite: true, canClear: true };
    expect(words(planMenu(mine))).toEqual(["yes", "maybe", "no", "-", "writeNote", "-", "cancel"]);
    expect(words(planMenu({ ...mine, hasNote: true }))).toEqual([
      "yes",
      "maybe",
      "no",
      "-",
      "editNote",
      "clearNote",
      "-",
      "cancel",
    ]);
    // Under way: the note stays, "Katıl" leads after the answers.
    expect(words(planMenu({ ...mine, hasNote: true, canWrite: false, joinable: true }))).toEqual([
      "yes",
      "maybe",
      "no",
      "-",
      "join",
      "-",
      "clearNote",
      "-",
      "cancel",
    ]);
  });

  it("lets the crew's leaders only clear a note, and an older service none at all", () => {
    const leader = { ...base, cancel: true, hasNote: true, canClear: true };
    expect(words(planMenu(leader))).toEqual([
      "yes",
      "maybe",
      "no",
      "-",
      "clearNote",
      "-",
      "report",
      "cancel",
    ]);
    expect(words(planMenu({ ...leader, notes: false }))).not.toContain("clearNote");
  });
});

describe("categoryMenu", () => {
  it("only folds the implicit category or adds another one", () => {
    expect(
      words(categoryMenu({ custom: false, canMoveUp: false, canMoveDown: false, canAdd: true })),
    ).toEqual(["toggle", "-", "newCategory"]);
  });

  it("renames, moves and deletes the player's own", () => {
    expect(
      words(categoryMenu({ custom: true, canMoveUp: false, canMoveDown: true, canAdd: false })),
    ).toEqual([
      "toggle",
      "-",
      "rename",
      "moveUp(off)",
      "moveDown",
      "newCategory(off)",
      "-",
      "remove",
    ]);
  });
});

describe("teamMenu", () => {
  const team = (role: "founder" | "member", size: number, limit = 30) => ({
    role,
    members: Array.from({ length: size }, (_, i) => member(String(i + 1))),
    invited: [],
    limit,
  });

  it("opens the crew's chat and reports the crew where those are on (CHAT1, MOD1)", () => {
    expect(
      words(teamMenu({ team: team("member", 4), friendsOutside: 3, chat: true, report: true })),
    ).toEqual(["open", "toggle", "-", "chat", "copyCode", "invite", "-", "report", "leave"]);
  });

  it("is the same menu on the crew page, without what means nothing there (FIX2)", () => {
    const full = teamMenu({ team: team("member", 4), friendsOutside: 3, chat: true, report: true });
    // The rail: no folding (only the friend list folds a crew).
    expect(words(without(full, ["toggle"]))).toEqual([
      "open",
      "-",
      "chat",
      "copyCode",
      "invite",
      "-",
      "report",
      "leave",
    ]);
    // The crew's own card: it is on screen already, and a whole group may go with its separator.
    expect(words(without(full, ["toggle", "open"]))).toEqual([
      "chat",
      "copyCode",
      "invite",
      "-",
      "report",
      "leave",
    ]);
  });

  it("lets a member open, fold, share and leave", () => {
    expect(words(teamMenu({ team: team("member", 4), friendsOutside: 3 }))).toEqual([
      "open",
      "toggle",
      "-",
      "copyCode",
      "invite",
      "-",
      "leave",
    ]);
  });

  it("lets the founder edit and disband (or close it when alone)", () => {
    expect(words(teamMenu({ team: team("founder", 4), friendsOutside: 3 }))).toEqual([
      "open",
      "toggle",
      "-",
      "copyCode",
      "invite",
      "edit",
      "-",
      "disband",
    ]);
    expect(words(teamMenu({ team: team("founder", 1), friendsOutside: 3 })).at(-1)).toBe("close");
  });

  it("cannot invite into a full team or with nobody left to invite", () => {
    expect(words(teamMenu({ team: team("member", 30), friendsOutside: 3 }))).toContain(
      "invite(off)",
    );
    expect(words(teamMenu({ team: team("member", 4), friendsOutside: 0 }))).toContain(
      "invite(off)",
    );
  });

  it("calls the crew where the player may, waiting out its five minutes", () => {
    const plan = (call: "ready" | "waiting" | "hidden") =>
      words(teamMenu({ team: team("member", 4), friendsOutside: 3, call }));
    expect(plan("ready")).toEqual([
      "open",
      "toggle",
      "-",
      "call",
      "copyCode",
      "invite",
      "-",
      "leave",
    ]);
    expect(plan("waiting")).toContain("call(off)");
    expect(plan("hidden")).not.toContain("call");
  });

  it("moves one of several crews in the player's order", () => {
    const plan = (first: boolean, last: boolean) =>
      words(teamMenu({ team: team("member", 4), friendsOutside: 3, order: { first, last } }));
    expect(plan(false, false)).toEqual([
      "open",
      "toggle",
      "-",
      "copyCode",
      "invite",
      "-",
      "makeMain",
      "moveUp",
      "moveDown",
      "-",
      "leave",
    ]);
    // The primary crew is first already; the last goes no lower.
    expect(plan(true, false)).not.toContain("makeMain");
    expect(plan(true, false)).toContain("moveUp(off)");
    expect(plan(false, true)).toContain("moveDown(off)");
    // One crew: no order at all.
    expect(words(teamMenu({ team: team("member", 4), friendsOutside: 3 }))).not.toContain("moveUp");
  });

  it("invites and edits only where the crew lets the player", () => {
    const plan = words(
      teamMenu({ team: team("member", 4), friendsOutside: 3, canInvite: false, canEdit: true }),
    );
    expect(plan).toEqual(["open", "toggle", "-", "copyCode", "edit", "-", "leave"]);
  });

  it("mutes the crew, or unmutes it, where the service keeps its settings", () => {
    const plan = (mute?: "muted" | "unmuted" | "hidden") =>
      words(teamMenu({ team: team("member", 4), friendsOutside: 3, mute }));
    expect(plan("unmuted")).toEqual([
      "open",
      "toggle",
      "-",
      "copyCode",
      "invite",
      "-",
      "mute",
      "-",
      "leave",
    ]);
    expect(plan("muted")).toContain("unmute");
    expect(plan("muted")).not.toContain("mute");
    expect(plan("hidden")).toEqual(plan());
    expect(plan()).not.toContain("mute");
  });

  it("names the way out like the team page", () => {
    expect(leavingOf(team("member", 1))).toBe("leave");
    expect(leavingOf(team("founder", 1))).toBe("close");
    expect(leavingOf(team("founder", 2))).toBe("disband");
  });
});

describe("callMenu", () => {
  it("answers, joins the caller on Projects and opens the crew", () => {
    expect(words(callMenu({ joinable: true, here: false }))).toEqual([
      "yes",
      "soon",
      "no",
      "-",
      "join",
      "open",
    ]);
    expect(words(callMenu({ joinable: false, here: false, answer: "soon" }))).toEqual([
      "yes",
      "soon✓",
      "no",
      "-",
      "open",
    ]);
    expect(words(callMenu({ joinable: true, here: true }))).toContain("join(off)");
  });
});
