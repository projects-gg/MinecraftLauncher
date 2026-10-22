import { describe, expect, it } from "vitest";
import type { CrewBadge, CrewJoin, SocialTeam, TeamMember } from "@/lib/api/social-types";
import {
  type CardInput,
  canAsk,
  canGreet,
  cardMenu,
  cardRow,
  corridor,
  inTriangle,
  nextGreetEnd,
  refusalWords,
  refusedNow,
  rowsReachable,
  shownBadge,
  tagChoices,
  turkeyDayEnd,
  wearersOf,
  wearOutcome,
  wearState,
  withRefusal,
} from "./badges";

const badge = (crew: string, requests = true): CrewBadge => ({
  crew,
  tag: "GK",
  color: "navy",
  frame: "shield",
  emblem: "bolt",
  name: "Gece Kartalları",
  motto: "Gece bizimdir",
  createdOn: "2026-09-20",
  members: 12,
  requests,
});

const member = (id: string, worn?: string): TeamMember => ({
  id,
  name: id,
  username: id,
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
  badge: worn ? badge(worn) : undefined,
});

const team = (id: string, extra: Partial<SocialTeam> = {}): SocialTeam => ({
  id,
  name: id,
  tag: "GK",
  emblem: "bolt",
  color: "navy",
  frame: "shield",
  motto: "",
  code: "EKP-AAAA-BBBB",
  createdOn: "2026-09-20",
  role: "member",
  moderator: false,
  permissions: { invite: "members", call: "members", kick: "leader", edit: "leader" },
  members: [],
  invited: [],
  limit: 30,
  mute: { muted: false, muteUntil: null },
  ...extra,
});

/** A friend's tag of a crew the player is not in, which takes requests: "Katılma isteği gönder". */
const asking: CardInput = {
  badge: badge("gk"),
  own: false,
  friend: true,
  crews: ["mine"],
  crewLimit: 5,
  multi: true,
  invites: [],
  waiting: [],
  requestsOn: true,
  refusedToday: false,
};

describe("shownBadge", () => {
  it("shows a tag only while the service offers tags", () => {
    expect(shownBadge({ badges: true }, badge("gk"))?.crew).toBe("gk");
    expect(shownBadge({ badges: false }, badge("gk"))).toBeUndefined();
    expect(shownBadge({ badges: true }, undefined)).toBeUndefined();
  });
});

describe("cardRow", () => {
  it("offers the player's own tag to be taken off, and members the crew", () => {
    expect(cardRow({ ...asking, own: true, crews: ["gk"] })).toBe("own");
    expect(cardRow({ ...asking, crews: ["mine", "gk"] })).toBe("member");
    // Only a crewmate (not a friend) who is in the crew too: still a member's card.
    expect(cardRow({ ...asking, friend: false, crews: ["gk"] })).toBe("member");
  });

  it("takes an invite, and shows a request that waits (whichever way it was sent)", () => {
    expect(cardRow({ ...asking, invites: ["gk"] })).toBe("invited");
    expect(cardRow({ ...asking, waiting: ["gk"] })).toBe("waiting");
    // An invite wins over a request (the invite lets the player in).
    expect(cardRow({ ...asking, invites: ["gk"], waiting: ["gk"] })).toBe("invited");
  });

  it("lets a friend of the wearer ask when everything allows it", () => {
    expect(cardRow(asking)).toBe("ask");
    expect(canAsk(asking)).toBe(true);
  });

  it("hides the button, never greys it, when the player cannot ask", () => {
    // Only a crewmate of the wearer, not a friend.
    expect(cardRow({ ...asking, friend: false })).toBeNull();
    // The crew takes no requests from its tag now (its setting, full, the day's ten).
    expect(cardRow({ ...asking, badge: badge("gk", false) })).toBeNull();
    // The service has no requests from tags.
    expect(cardRow({ ...asking, requestsOn: false })).toBeNull();
    // At the crew limit.
    expect(cardRow({ ...asking, crews: ["a", "b", "c", "d", "e"] })).toBeNull();
    // One crew per player, in one already.
    expect(cardRow({ ...asking, multi: false, crewLimit: 1 })).toBeNull();
    expect(cardRow({ ...asking, multi: false, crewLimit: 1, crews: [] })).toBe("ask");
    // Three requests wait already.
    expect(cardRow({ ...asking, waiting: ["x", "y", "z"] })).toBeNull();
    // This device was refused today.
    expect(cardRow({ ...asking, refusedToday: true })).toBeNull();
  });

  it("gives the crew's menu to members and only a report to everyone else", () => {
    expect(cardMenu("own")).toBe("crew");
    expect(cardMenu("member")).toBe("crew");
    for (const row of ["invited", "waiting", "ask", null] as const) {
      expect(cardMenu(row)).toBe("report");
    }
  });

  it("keeps every row the bottom line may turn into in one cell", () => {
    expect(rowsReachable("ask")).toEqual(["ask", "waiting", "refused"]);
    expect(rowsReachable("waiting")).toEqual(["ask", "waiting", "refused"]);
    expect(rowsReachable("invited")).toEqual(["invited", "member"]);
    expect(rowsReachable("member")).toEqual(["member"]);
    expect(rowsReachable(null)).toEqual([]);
  });
});

describe("refusals", () => {
  it("words the crew's refusals as the card says them, the player's limits as TEAM4 and TEAM5", () => {
    expect(refusalWords("closed")).toEqual({ key: "team6.card.refused.closed" });
    expect(refusalWords("today")).toEqual({ key: "team6.card.refused.today" });
    expect(refusalWords("waitingFull")).toEqual({ key: "team6.card.refused.waitingFull" });
    expect(refusalWords("full")).toEqual({ key: "team6.card.refused.full" });
    expect(refusalWords("blocked")).toEqual({ key: "team6.card.refused.blocked" });
    expect(refusalWords("ownDaily")).toEqual({ key: "team6.card.refused.ownDaily" });
    expect(refusalWords("ownWaiting")).toEqual({ key: "crewWave2.join.askLimit" });
    expect(refusalWords("ownCrews")).toEqual({ key: "crews.limit", count: "crewLimit" });
    // In the crew already: the list says it.
    expect(refusalWords("member")).toBeNull();
  });

  it("ends the day in Turkey (UTC+3) at 21:00 UTC", () => {
    const end = Date.UTC(2026, 9, 22, 21, 0) / 1000;
    expect(turkeyDayEnd(Date.UTC(2026, 9, 22, 6, 0))).toBe(end);
    expect(turkeyDayEnd(Date.UTC(2026, 9, 22, 20, 59))).toBe(end);
    expect(turkeyDayEnd(Date.UTC(2026, 9, 22, 21, 0))).toBe(end + 86_400);
  });

  it("remembers a refused crew until the answer's end of day", () => {
    const morning = Date.UTC(2026, 9, 22, 6, 0);
    const until = turkeyDayEnd(morning);
    const memory = withRefusal({}, "gk", until, morning);
    expect(refusedNow(memory, "gk", morning + 3 * 3_600_000)).toBe(true);
    expect(refusedNow(memory, "other", morning)).toBe(false);
    // The next day in Turkey.
    expect(refusedNow(memory, "gk", Date.UTC(2026, 9, 22, 21, 1))).toBe(false);
    // A refusal on a new day forgets the old ones.
    const nextDay = Date.UTC(2026, 9, 23, 8, 0);
    expect(withRefusal(memory, "sky", turkeyDayEnd(nextDay), nextDay)).toEqual({
      sky: turkeyDayEnd(nextDay),
    });
    // The same crew twice is kept once.
    expect(Object.keys(withRefusal(memory, "gk", until, morning))).toEqual(["gk"]);
  });
});

describe("wearing", () => {
  const nowMs = Date.UTC(2026, 9, 22, 12, 0);

  it("says whether a crew's tag may be put on, is worn, or why it cannot be", () => {
    expect(wearState(team("gk"), null, nowMs)).toBe("wear");
    expect(wearState(team("gk"), "gk", nowMs)).toBe("worn");
    expect(wearState(team("gk"), "sky", nowMs)).toBe("wear");
    expect(wearState(team("gk", { hidden: true }), null, nowMs)).toBe("hidden");
    const locked = team("gk", { textLockedUntil: nowMs / 1000 + 3600 });
    expect(wearState(locked, null, nowMs)).toBe("locked");
    // A lock that ended.
    const unlocked = team("gk", { textLockedUntil: nowMs / 1000 - 1 });
    expect(wearState(unlocked, null, nowMs)).toBe("wear");
  });

  it("words a change: put on, switched from another crew's, taken off", () => {
    expect(wearOutcome(null, "gk")).toBe("wore");
    expect(wearOutcome("sky", "gk")).toBe("switched");
    expect(wearOutcome("gk", null)).toBe("off");
    expect(wearOutcome("gk", "gk")).toBeNull();
    expect(wearOutcome(null, null)).toBeNull();
  });

  it("counts the members who wear the crew's own tag", () => {
    const crew = team("gk", {
      members: [member("1", "gk"), member("2", "sky"), member("3"), member("4", "gk")],
    });
    expect(wearersOf(crew)).toBe(2);
  });

  it("lists the crews in the player's order, the first as the primary crew", () => {
    const nowSec = nowMs / 1000;
    const crews = [
      team("a"),
      team("b", { hidden: true }),
      team("c", { textLockedUntil: nowSec + 60 }),
    ];
    expect(tagChoices(crews, nowMs).map((c) => [c.crew.id, c.main, c.blocked])).toEqual([
      ["a", true, null],
      ["b", false, "hidden"],
      ["c", false, "locked"],
    ]);
    // One crew: no "Ana ekip" note.
    expect(tagChoices([team("a")], nowMs)[0]?.main).toBe(false);
  });
});

describe("Hoş geldin", () => {
  const at = Date.UTC(2026, 9, 22, 18, 0) / 1000;
  const join = (id: string, greetUntil?: number, mine = false): CrewJoin => ({
    crew: "gk",
    id,
    name: id,
    mine,
    at,
    greetUntil,
  });

  it("offers 'Selam ver' until its time ends, never on the player's own line", () => {
    const ece = join("ece", at + 600);
    expect(canGreet(ece, (at + 1) * 1000)).toBe(true);
    expect(canGreet(ece, (at + 600) * 1000)).toBe(false);
    // Pressed already (the backend took its time away), or the player's own arrival.
    expect(canGreet(join("ece"), (at + 1) * 1000)).toBe(false);
    expect(canGreet(join("me", at + 600, true), (at + 1) * 1000)).toBe(false);
  });

  it("wakes up once, when the next 'Selam ver' goes", () => {
    const joins = [join("a", at + 600), join("b", at + 300), join("c"), join("me", at + 100, true)];
    expect(nextGreetEnd(joins, (at + 1) * 1000)).toBe((at + 300) * 1000);
    expect(nextGreetEnd(joins, (at + 400) * 1000)).toBe((at + 600) * 1000);
    expect(nextGreetEnd(joins, (at + 700) * 1000)).toBeNull();
  });
});

describe("the corridor to a card", () => {
  const card = { left: 100, top: 200, right: 364, bottom: 380 };

  it("runs from where the pointer left the chip to the card's near edge", () => {
    // The card under the chip.
    const down = corridor({ x: 120, y: 190 }, card);
    expect(down).toEqual([
      { x: 120, y: 190 },
      { x: 100, y: 200 },
      { x: 364, y: 200 },
    ]);
    // The card above the chip (it flipped).
    expect(corridor({ x: 120, y: 390 }, card)?.[1]).toEqual({ x: 100, y: 380 });
    // Beside it.
    expect(corridor({ x: 90, y: 250 }, card)?.[2]).toEqual({ x: 100, y: 380 });
    expect(corridor({ x: 370, y: 250 }, card)?.[1]).toEqual({ x: 364, y: 200 });
    // Over the card already.
    expect(corridor({ x: 200, y: 250 }, card)).toBeNull();
  });

  it("knows a point on its way to the card from one going elsewhere", () => {
    const path = corridor({ x: 120, y: 190 }, card);
    if (!path) throw new Error("no corridor");
    expect(inTriangle({ x: 130, y: 196 }, path)).toBe(true);
    expect(inTriangle({ x: 250, y: 199 }, path)).toBe(true);
    // Back up and away from the card.
    expect(inTriangle({ x: 120, y: 170 }, path)).toBe(false);
    // Sideways, outside the triangle.
    expect(inTriangle({ x: 60, y: 195 }, path)).toBe(false);
  });
});
