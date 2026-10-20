import { describe, expect, it } from "vitest";
import type { Friend, FriendGroups } from "@/lib/api/social-types";
import {
  addGroup,
  cleanGroupName,
  dragOrder,
  dropTarget,
  EMPTY_GROUPS,
  groupOf,
  groupSections,
  layoutTop,
  MAX_GROUPS,
  moveFriend,
  newGroupId,
  pruneMembers,
  REST,
  removeGroup,
  renameGroup,
  reorderGroups,
  sameGroups,
  stepFriend,
  stepGroup,
} from "./groups";

function friend(id: string, name: string, extra: Partial<Friend> = {}): Friend {
  return {
    id,
    name,
    username: name.toLowerCase(),
    avatar: "discord",
    state: "offline",
    dnd: false,
    discordUrl: `https://discord.com/users/${id}`,
    ...extra,
  };
}

const doc: FriendGroups = {
  version: 3,
  groups: [
    { id: "okul", name: "Okul" },
    { id: "pvp", name: "PvP" },
  ],
  members: { "1": "okul", "2": "pvp", "9": "okul" },
};

describe("changing categories", () => {
  it("adds at the end with a clean name, up to the limit, optionally with a friend", () => {
    const added = addGroup(doc, "aile", "  Aile   grubu ", "3");
    expect(added.groups.map((g) => g.name)).toEqual(["Okul", "PvP", "Aile grubu"]);
    expect(groupOf(added, "3")).toBe("aile");
    expect(addGroup(doc, "x", "   ")).toBe(doc);
    expect(addGroup(doc, "okul", "İkinci")).toBe(doc);
    const full = {
      ...EMPTY_GROUPS,
      groups: Array.from({ length: MAX_GROUPS }, (_, i) => ({ id: `g${i}`, name: `G${i}` })),
    };
    expect(addGroup(full, "one", "Bir fazla")).toBe(full);
    expect(cleanGroupName("x".repeat(40))).toHaveLength(24);
  });

  it("renames, deletes (friends go back to Arkadaşlar) and moves friends", () => {
    expect(renameGroup(doc, "pvp", "Savaş")).toMatchObject({
      groups: [{ name: "Okul" }, { name: "Savaş" }],
    });
    expect(renameGroup(doc, "pvp", " ")).toBe(doc);
    const removed = removeGroup(doc, "okul");
    expect(removed.groups.map((g) => g.id)).toEqual(["pvp"]);
    expect(groupOf(removed, "1")).toBe(REST);
    expect(removed.members).toEqual({ "2": "pvp" });

    expect(groupOf(moveFriend(doc, "1", "pvp"), "1")).toBe("pvp");
    expect(moveFriend(doc, "1", REST).members).toEqual({ "2": "pvp", "9": "okul" });
    expect(groupOf(moveFriend(doc, "1", "nope"), "1")).toBe(REST);
    // A stale entry pointing at a deleted category counts as Arkadaşlar.
    expect(groupOf({ ...doc, members: { "1": "gone" } }, "1")).toBe(REST);
  });

  it("reorders, steps categories and steps friends between them", () => {
    expect(reorderGroups(doc, ["pvp", "okul"]).groups.map((g) => g.id)).toEqual(["pvp", "okul"]);
    expect(reorderGroups(doc, ["pvp", "x"]).groups.map((g) => g.id)).toEqual(["pvp", "okul"]);
    expect(stepGroup(doc, "okul", 1)?.groups.map((g) => g.id)).toEqual(["pvp", "okul"]);
    expect(stepGroup(doc, "okul", -1)).toBeNull();
    expect(stepGroup(doc, "pvp", 1)).toBeNull();

    expect(groupOf(stepFriend(doc, "1", 1) as FriendGroups, "1")).toBe("pvp");
    expect(groupOf(stepFriend(doc, "2", 1) as FriendGroups, "2")).toBe(REST);
    expect(stepFriend(doc, "1", -1)).toBeNull();
    expect(groupOf(stepFriend(doc, "5", -1) as FriendGroups, "5")).toBe("pvp");
    expect(stepFriend(doc, "5", 1)).toBeNull();
  });

  it("drops entries of former friends and compares documents without versions", () => {
    expect(pruneMembers(doc, [friend("1", "A"), friend("2", "B")]).members).toEqual({
      "1": "okul",
      "2": "pvp",
    });
    expect(sameGroups(doc, { ...doc, version: 9 })).toBe(true);
    expect(sameGroups(doc, renameGroup(doc, "okul", "Ders"))).toBe(false);
    expect(sameGroups(doc, moveFriend(doc, "9", "pvp"))).toBe(false);
  });

  it("makes unused ids", () => {
    let calls = 0;
    // A fixed sequence of draws: 8 symbols of the alphabet, not taken.
    const id = newGroupId(doc.groups, () => (calls++ % 36) / 36);
    expect(id).toMatch(/^[a-z0-9]{8}$/);
    expect(doc.groups.some((g) => g.id === id)).toBe(false);
  });
});

describe("sections", () => {
  const friends = [
    friend("1", "Ayşe", { state: "playing", game: { where: "projects" } }),
    friend("9", "Zeki"),
    friend("2", "Bora", { state: "launcher" }),
    friend("3", "Can", { state: "launcher" }),
    friend("4", "Deniz", { lastSeen: "today" }),
  ];

  it("lists the player's categories in order, empty ones too, then Arkadaşlar", () => {
    const sections = groupSections(
      friends,
      { ...doc, groups: [...doc.groups, { id: "aile", name: "Aile" }] },
      [],
    );
    expect(sections.map((s) => [s.id, s.name, s.friends.map((f) => f.id), s.online])).toEqual([
      ["okul", "Okul", ["1", "9"], 1],
      ["pvp", "PvP", ["2"], 1],
      ["aile", "Aile", [], 0],
      [REST, null, ["3", "4"], 1],
    ]);
  });

  it("keeps favourites first inside a category", () => {
    const sections = groupSections(
      [friend("3", "Can", { state: "launcher" }), friend("5", "Ece", { state: "launcher" })],
      EMPTY_GROUPS,
      ["5"],
    );
    expect(sections).toHaveLength(1);
    expect(sections[0]?.friends.map((f) => f.id)).toEqual(["5", "3"]);
  });
});

describe("dragging", () => {
  const headers = [
    { id: "okul", top: 0 },
    { id: "pvp", top: 120 },
    { id: REST, top: 200 },
  ];

  it("a friend lands in the category whose block holds the pointer", () => {
    expect(dropTarget(-10, headers)).toBeNull();
    expect(dropTarget(5, headers)).toBe("okul");
    expect(dropTarget(119, headers)).toBe("okul");
    expect(dropTarget(120, headers)).toBe("pvp");
    expect(dropTarget(900, headers)).toBe(REST);
  });

  it("a category passes a neighbour once it crosses that neighbour's middle", () => {
    const blocks = new Map([
      ["okul", { top: 0, bottom: 100 }],
      ["pvp", { top: 132, bottom: 232 }],
      ["aile", { top: 232, bottom: 300 }],
    ]);
    const order = ["okul", "drag", "pvp", "aile"];
    expect(dragOrder(order, "drag", 150, 1, blocks)).toEqual(order);
    expect(dragOrder(order, "drag", 190, 1, blocks)).toEqual(["okul", "pvp", "drag", "aile"]);
    expect(dragOrder(order, "drag", 60, -1, blocks)).toEqual(order);
    expect(dragOrder(order, "drag", 40, -1, blocks)).toEqual(["drag", "okul", "pvp", "aile"]);
    expect(dragOrder(["drag"], "drag", 500, 1, blocks)).toEqual(["drag"]);
  });

  // Stand-ins for laid out elements: what `layoutTop` reads of them.
  const box = (offsetTop: number, offsetParent: object | null, clientTop = 0) =>
    ({ offsetTop, offsetParent, clientTop }) as unknown as HTMLElement;

  it("a header is measured in the list's layout while its row glides (FIX2)", () => {
    const list = box(0, null);
    // At rest the list is the header's offset parent.
    expect(layoutTop(box(95, list), list)).toBe(95);
    // Gliding, the row (a transform) is the offset parent: the header's own offset reads 0.
    const gliding = box(95, list);
    expect(layoutTop(box(0, gliding), list)).toBe(95);
    // A bordered parent on the way adds its border.
    const framed = box(40, list, 1);
    expect(layoutTop(box(10, framed), list)).toBe(51);
    // Not laid out inside the list: the caller measures on screen.
    expect(layoutTop(box(10, null), list)).toBeNull();
    expect(layoutTop(box(10, box(0, null)), list)).toBeNull();
  });
});
