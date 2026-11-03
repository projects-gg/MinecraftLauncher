import { describe, expect, it } from "vitest";
import type { ChatState, Conversation } from "@/lib/api/social-types";
import {
  conversationMenu,
  liveMarks,
  livePins,
  shownUnread,
  unreadShownAll,
  withPinsFirst,
} from "./conv-menu";

const conv = (id: string, unread: number, at?: number): Conversation => ({
  id,
  unread,
  writable: true,
  failed: false,
  last: at === undefined ? undefined : { from: "101", mine: false, text: "selam", at },
});

describe("conversationMenu", () => {
  it("offers read or unread, pinning, then the mute and the history", () => {
    const actions = (input: Parameters<typeof conversationMenu>[0]) =>
      conversationMenu(input).map((row) => (row ? row.action : "|"));
    expect(
      actions({
        unread: true,
        hasMessages: true,
        theirsNewest: true,
        pinned: false,
        pinsFull: false,
        muted: false,
      }),
    ).toEqual(["markRead", "pin", "|", "mute", "clear"]);
    expect(
      actions({
        unread: false,
        hasMessages: true,
        theirsNewest: true,
        pinned: true,
        pinsFull: true,
        muted: true,
      }),
    ).toEqual(["markUnread", "unpin", "|", "unmute", "clear"]);
    // The player's own last word is never "new": nothing to mark.
    expect(
      actions({
        unread: false,
        hasMessages: true,
        theirsNewest: false,
        pinned: false,
        pinsFull: false,
        muted: false,
      }),
    ).toEqual(["pin", "|", "mute", "clear"]);
    // Nothing to mark or clear in an empty conversation.
    expect(
      actions({
        unread: false,
        hasMessages: false,
        theirsNewest: false,
        pinned: false,
        pinsFull: false,
        muted: false,
      }),
    ).toEqual(["pin", "|", "mute"]);
    // Three pinned: the row stays and says the rule when pressed (a toast).
    expect(
      conversationMenu({
        unread: false,
        hasMessages: true,
        theirsNewest: true,
        pinned: false,
        pinsFull: true,
        muted: false,
      })[1],
    ).toEqual({ action: "pin" });
  });
});

describe("unread with the player's own marks", () => {
  it("counts the service's count, else one while the marked message is the newest", () => {
    expect(shownUnread(conv("d:1:2", 3, 100), { "d:1:2": 50 })).toBe(3);
    expect(shownUnread(conv("d:1:2", 0, 100), { "d:1:2": 100 })).toBe(1);
    // A newer message came (the player wrote, or it was read): the mark says nothing.
    expect(shownUnread(conv("d:1:2", 0, 120), { "d:1:2": 100 })).toBe(0);
    expect(shownUnread(conv("d:1:2", 0), { "d:1:2": 100 })).toBe(0);
    expect(shownUnread(conv("d:1:2", 0, 100), {})).toBe(0);
  });

  it("adds the marks to every conversation's count", () => {
    const chat = {
      conversations: [conv("d:1:2", 2, 10), conv("c:ab", 0, 20), conv("d:1:3", 0, 30)],
    } as ChatState;
    expect(unreadShownAll(chat, { "c:ab": 20 })).toBe(3);
    expect(unreadShownAll(undefined, {})).toBe(0);
  });

  it("keeps only the marks that still say something", () => {
    const list = [conv("d:1:2", 0, 10), conv("c:ab", 4, 20), conv("d:1:3", 0, 40)];
    expect(liveMarks({ "d:1:2": 10, "c:ab": 20, "d:1:3": 30, "d:9:9": 5 }, list)).toEqual({
      "d:1:2": 10,
    });
  });
});

describe("pinned conversations", () => {
  const list = [conv("d:1:2", 0), conv("c:ab", 0), conv("d:1:3", 0), conv("d:1:4", 0)];

  it("puts the pinned ones first, in the order they were pinned", () => {
    expect(withPinsFirst(list, ["d:1:3", "c:ab"]).map((c) => c.id)).toEqual([
      "d:1:3",
      "c:ab",
      "d:1:2",
      "d:1:4",
    ]);
    expect(withPinsFirst(list, [])).toBe(list);
    // A pin of a conversation no longer listed is left out (and frees its place).
    expect(withPinsFirst(list, ["d:9:9"])).toBe(list);
    expect(livePins(["d:9:9", "d:1:4"], list)).toEqual(["d:1:4"]);
  });
});
