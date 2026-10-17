import { describe, expect, it } from "vitest";
import {
  movePinned,
  type OrderedAccount,
  pinnedIds,
  reorderPinnedInList,
  setPinnedInList,
  sortAccounts,
  withPinnedOrder,
} from "./account-order";

const hours = (h: number) => new Date(Date.UTC(2026, 9, 16, 12) - h * 3_600_000).toISOString();

function account(id: string, extra: Partial<OrderedAccount> = {}): OrderedAccount {
  return { id, active: false, addedAt: hours(1000), ...extra };
}

const ids = (list: readonly OrderedAccount[]) => list.map((a) => a.id);

describe("sortAccounts", () => {
  it("puts pinned accounts first in saved order, then the active one, then by last play", () => {
    const list = [
      account("old", { lastUsedAt: hours(300) }),
      account("pinB", { pinned: true, lastUsedAt: hours(900) }),
      account("recent", { lastUsedAt: hours(2) }),
      account("active", { active: true, lastUsedAt: hours(50) }),
      account("pinA", { pinned: true }),
      account("never"),
    ];
    expect(ids(sortAccounts(list))).toEqual(["pinB", "pinA", "active", "recent", "old", "never"]);
  });

  it("keeps a pinned active account in its pinned place", () => {
    const list = [
      account("a", { pinned: true }),
      account("b", { pinned: true, active: true }),
      account("c", { lastUsedAt: hours(1) }),
    ];
    expect(ids(sortAccounts(list))).toEqual(["a", "b", "c"]);
  });

  it("sorts never-played accounts by when they were added, then by saved order", () => {
    const list = [
      account("first", { addedAt: hours(30) }),
      account("newest", { addedAt: hours(1) }),
      account("twinA", { addedAt: hours(10) }),
      account("twinB", { addedAt: hours(10) }),
      account("noDate", { addedAt: undefined }),
    ];
    expect(ids(sortAccounts(list))).toEqual(["newest", "twinA", "twinB", "first", "noDate"]);
  });

  it("does not change the input", () => {
    const list = [account("a"), account("b", { active: true })];
    sortAccounts(list);
    expect(ids(list)).toEqual(["a", "b"]);
  });
});

describe("pinning", () => {
  const list = [account("a"), account("b"), account("c"), account("d")];

  it("joins the end of the pinned accounts", () => {
    let next = setPinnedInList(list, "c", true);
    expect(ids(next)).toEqual(["c", "a", "b", "d"]);
    next = setPinnedInList(next, "a", true);
    expect(ids(next)).toEqual(["c", "a", "b", "d"]);
    next = setPinnedInList(next, "d", true);
    expect(ids(next)).toEqual(["c", "a", "d", "b"]);
    expect(pinnedIds(next)).toEqual(["c", "a", "d"]);
  });

  it("stays in place when unpinned and lines up again when pinned again", () => {
    let next = setPinnedInList(setPinnedInList(list, "b", true), "c", true);
    next = setPinnedInList(next, "b", false);
    expect(ids(next)).toEqual(["b", "c", "a", "d"]);
    expect(pinnedIds(next)).toEqual(["c"]);
    next = setPinnedInList(next, "b", true);
    expect(ids(next)).toEqual(["c", "b", "a", "d"]);
  });

  it("ignores unknown ids and no-op changes", () => {
    expect(ids(setPinnedInList(list, "zz", true))).toEqual(["a", "b", "c", "d"]);
    expect(ids(setPinnedInList(list, "a", false))).toEqual(["a", "b", "c", "d"]);
  });
});

describe("reordering pinned accounts", () => {
  const list = [
    account("p1", { pinned: true }),
    account("x"),
    account("p2", { pinned: true }),
    account("p3", { pinned: true }),
    account("y"),
  ];

  it("orders only pinned accounts and moves them to the front, like the backend", () => {
    const next = reorderPinnedInList(list, ["p3", "x", "nope", "p1"]);
    expect(ids(next)).toEqual(["p3", "p1", "p2", "x", "y"]);
  });

  it("moves one place with Alt+arrows and stops at the ends", () => {
    const order = pinnedIds(list);
    expect(movePinned(order, "p2", -1)).toEqual(["p2", "p1", "p3"]);
    expect(movePinned(order, "p2", 1)).toEqual(["p1", "p3", "p2"]);
    expect(movePinned(order, "p1", -1)).toBeNull();
    expect(movePinned(order, "p3", 1)).toBeNull();
    expect(movePinned(order, "x", 1)).toBeNull();
  });

  it("shows a temporary order while dragging without touching the others", () => {
    const sorted = sortAccounts(list);
    expect(ids(withPinnedOrder(sorted, ["p2", "p3", "p1"]))).toEqual(["p2", "p3", "p1", "x", "y"]);
    expect(ids(withPinnedOrder(sorted, null))).toEqual(ids(sorted));
  });
});
