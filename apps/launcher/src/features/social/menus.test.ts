import { describe, expect, it } from "vitest";
import type { TeamMember } from "@/lib/api/social-types";
import {
  callMenu,
  categoryMenu,
  type FriendInput,
  friendMenu,
  leavingOf,
  memberMenu,
  type Plan,
  panelMenu,
  teamMenu,
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

describe("friendMenu", () => {
  const base: FriendInput = {
    joinable: false,
    gameBusy: false,
    invite: "ready",
    notifyOnline: false,
    categories: false,
    team: null,
  };

  it("lists the actions in the card's order, the destructive ones apart", () => {
    expect(words(friendMenu(base))).toEqual([
      "invite",
      "message",
      "-",
      "favorite",
      "notifyOnline",
      "-",
      "remove",
      "block",
    ]);
  });

  it("offers Katıl on projects.gg, waiting while a game is busy", () => {
    expect(words(friendMenu({ ...base, joinable: true }))[0]).toBe("join");
    expect(words(friendMenu({ ...base, joinable: true, gameBusy: true }))[0]).toBe("join(off)");
  });

  it("shows the invite's wait and hides it when it cannot be sent", () => {
    expect(words(friendMenu({ ...base, invite: "waiting" }))).toContain("invite(off)");
    expect(words(friendMenu({ ...base, invite: "hidden" }))).not.toContain("invite");
    expect(words(friendMenu({ ...base, invite: "hidden" }))[0]).toBe("message");
  });

  it("marks the online notice when it is on", () => {
    expect(words(friendMenu({ ...base, notifyOnline: true }))).toContain("notifyOnline✓");
  });

  it("moves to a category and invites to the team only when those exist", () => {
    expect(words(friendMenu({ ...base, categories: true }))).toContain("category");
    expect(words(friendMenu({ ...base, team: "invitable" }))).toContain("teamInvite");
    expect(words(friendMenu({ ...base, team: "invited" }))).toContain("teamInvite(off)");
    expect(words(friendMenu({ ...base, team: "member" })).join()).not.toContain("teamInvite");
    // Every crew full: nothing to invite to.
    expect(words(friendMenu({ ...base, team: "full" }))).toContain("teamInvite(off)");
  });

  it("opens the chat and reports only where those are on (CHAT1, MOD1)", () => {
    expect(words(friendMenu({ ...base, chat: true, report: true }))).toEqual([
      "invite",
      "chat",
      "message",
      "-",
      "favorite",
      "notifyOnline",
      "-",
      "remove",
      "block",
      "report",
    ]);
  });
});

describe("memberMenu", () => {
  const base = { joinable: false, gameBusy: false, founder: false, blocked: false };

  it("lets a member message and block a teammate", () => {
    expect(words(memberMenu(base))).toEqual(["message", "-", "block"]);
  });

  it("lets the founder hand the role on and remove", () => {
    expect(words(memberMenu({ ...base, founder: true, joinable: true }))).toEqual([
      "join",
      "message",
      "-",
      "makeFounder",
      "remove",
      "-",
      "block",
    ]);
  });

  it("lets the leader name and un-name moderators where the service has roles", () => {
    const leader = { ...base, founder: true, roles: true };
    expect(words(memberMenu(leader))).toEqual([
      "message",
      "-",
      "makeModerator",
      "makeFounder",
      "remove",
      "-",
      "block",
    ]);
    expect(words(memberMenu({ ...leader, moderator: true }))).toContain("unmakeModerator");
    expect(words(memberMenu({ ...leader, moderatorsFull: true }))).toContain("makeModerator(off)");
    // A service without roles: no moderators at all.
    expect(words(memberMenu({ ...leader, roles: false }))).not.toContain("makeModerator");
  });

  it("lets a moderator remove where the crew lets them, and nothing more", () => {
    expect(words(memberMenu({ ...base, roles: true, removable: true }))).toEqual([
      "message",
      "-",
      "remove",
      "-",
      "block",
    ]);
    expect(words(memberMenu({ ...base, roles: true, removable: false }))).toEqual([
      "message",
      "-",
      "block",
    ]);
  });

  it("offers no Katıl to someone blocked, and lifts the block", () => {
    expect(words(memberMenu({ ...base, joinable: true, blocked: true }))).toEqual([
      "message",
      "-",
      "unblock",
    ]);
    expect(words(memberMenu({ ...base, joinable: true, gameBusy: true }))[0]).toBe("join(off)");
  });

  it("opens the chat with a friend in the crew and reports (CHAT1, MOD1)", () => {
    expect(words(memberMenu({ ...base, chat: true, report: true }))).toEqual([
      "chat",
      "message",
      "-",
      "block",
      "report",
    ]);
    // Blocked: no conversation.
    expect(words(memberMenu({ ...base, chat: true, blocked: true }))).toEqual([
      "message",
      "-",
      "unblock",
    ]);
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
    expect(words(callMenu({ joinable: true, gameBusy: false }))).toEqual([
      "yes",
      "soon",
      "no",
      "-",
      "join",
      "open",
    ]);
    expect(words(callMenu({ joinable: false, gameBusy: false, answer: "soon" }))).toEqual([
      "yes",
      "soon✓",
      "no",
      "-",
      "open",
    ]);
    expect(words(callMenu({ joinable: true, gameBusy: true }))).toContain("join(off)");
  });
});
