import { describe, expect, it } from "vitest";
import type { SocialNotice } from "@/lib/api/social-types";
import { directConversation } from "../chat/conversations";
import { foldRuns, fromIgnored, ignoredDirectMutes, noticeFrom } from "./ignore";

const message = (from: string) => ({
  kind: "chatMessage" as const,
  conv: "c:ab",
  message: {
    key: `i:${from}`,
    from,
    mine: false,
    text: "selam",
    at: 1,
    edited: false,
    state: "sent" as const,
  },
});

describe("notices from someone ignored", () => {
  const ignored = new Set(["101"]);

  it("are told apart by who they come from", () => {
    expect(fromIgnored(message("101"), ignored)).toBe(true);
    expect(fromIgnored(message("102"), ignored)).toBe(false);
    expect(fromIgnored({ kind: "friendOnline", id: "101" }, ignored)).toBe(true);
    expect(
      fromIgnored({ kind: "crewCall", call: { from: "101" } } as unknown as SocialNotice, ignored),
    ).toBe(true);
    expect(
      fromIgnored({ kind: "crewPlan", plan: { by: "101" } } as unknown as SocialNotice, ignored),
    ).toBe(true);
    expect(
      fromIgnored(
        { kind: "invite", invite: { id: "i", from: "101", at: 1, quiet: false } },
        ignored,
      ),
    ).toBe(true);
    // Notices about the player's own account always come.
    expect(noticeFrom({ kind: "signedOutRemotely" })).toBeNull();
    expect(fromIgnored({ kind: "restricted" }, ignored)).toBe(false);
  });
});

describe("foldRuns", () => {
  const ignored = new Set(["101"]);
  const items = [
    { key: "a", from: "102" },
    { key: "b", from: "101" },
    { key: "c", from: "101" },
    { key: "plan", from: undefined },
    { key: "d", from: "101" },
    { key: "e", from: "102" },
    { key: "f", from: "101" },
  ];

  it("folds each run of an ignored person's messages into its first", () => {
    const folded = foldRuns(items, ignored, new Set());
    expect([...folded.keys()]).toEqual(["b", "c", "d", "f"]);
    expect(folded.get("b")).toEqual({ run: "b", lead: true, count: 2 });
    expect(folded.get("c")).toEqual({ run: "b", lead: false, count: 2 });
    // A local line between breaks the run.
    expect(folded.get("d")).toEqual({ run: "d", lead: true, count: 1 });
    expect(folded.get("f")).toEqual({ run: "f", lead: true, count: 1 });
  });

  it("leaves a run the player opened, and everything without anyone ignored", () => {
    expect([...foldRuns(items, ignored, new Set(["b"])).keys()]).toEqual(["d", "f"]);
    expect(foldRuns(items, new Set(), new Set()).size).toBe(0);
  });
});

describe("ignoredDirectMutes", () => {
  it("names the direct conversation with each ignored friend", () => {
    expect(ignoredDirectMutes("100", ["101", "9"], directConversation)).toEqual({
      [directConversation("100", "101")]: null,
      [directConversation("100", "9")]: null,
    });
    expect(ignoredDirectMutes(undefined, ["101"], directConversation)).toEqual({});
  });
});
