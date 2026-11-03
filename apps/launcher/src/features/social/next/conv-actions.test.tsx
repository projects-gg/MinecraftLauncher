import type { TFunction } from "i18next";
import { afterEach, describe, expect, it } from "vitest";
import type { Conversation } from "@/lib/api/social-types";
import { useSocialPrefs } from "../store";
import { conversationItems } from "./conv-actions";
import { useSocNextPrefs } from "./prefs";

const t = ((key: string) => key) as unknown as TFunction;

const conv = (id: string, unread: number, at = 100): Conversation => ({
  id,
  unread,
  writable: true,
  failed: false,
  last: { from: "101", mine: false, text: "selam", at },
});

afterEach(() => {
  useSocNextPrefs.setState({ unreadMarks: {}, pinnedChats: [] });
  useSocialPrefs.setState({ chatMutes: {} });
});

describe("conversationItems", () => {
  const list = [conv("d:100:101", 0), conv("d:100:102", 2), conv("c:ab", 0), conv("d:100:103", 0)];

  it("words the rows in order, with a separator before the mute", () => {
    const rows = conversationItems(list[0] as Conversation, list, t);
    expect(rows.map((row) => (row ? row.label : "|"))).toEqual([
      "socNext.conv.markUnread",
      "socNext.conv.pin",
      "|",
      "chat.actions.mute",
      "chat.actions.clear",
    ]);
    // "Sessize al" offers the crew mute's four choices.
    expect(rows[3]?.items).toHaveLength(4);
  });

  it("marks a conversation unread here, and read again", () => {
    const first = list[0] as Conversation;
    conversationItems(first, list, t)[0]?.onSelect();
    expect(useSocNextPrefs.getState().unreadMarks[first.id]).toBe(100);
    const marked = conversationItems(first, list, t);
    expect(marked[0]?.label).toBe("socNext.conv.markRead");
    marked[0]?.onSelect();
    expect(useSocNextPrefs.getState().unreadMarks[first.id]).toBeUndefined();
  });

  it("pins up to three, the fourth says the rule and pins nothing", () => {
    for (const one of list.slice(0, 3)) {
      conversationItems(one, list, t)
        .find((row) => row?.label === "socNext.conv.pin")
        ?.onSelect();
    }
    expect(useSocNextPrefs.getState().pinnedChats).toEqual(["d:100:101", "d:100:102", "c:ab"]);
    conversationItems(list[3] as Conversation, list, t)
      .find((row) => row?.label === "socNext.conv.pin")
      ?.onSelect();
    expect(useSocNextPrefs.getState().pinnedChats).toHaveLength(3);
    // A pinned one offers "Sabitlemeyi kaldır".
    const pinned = conversationItems(list[1] as Conversation, list, t);
    expect(pinned.some((row) => row?.label === "socNext.conv.unpin")).toBe(true);
  });
});
