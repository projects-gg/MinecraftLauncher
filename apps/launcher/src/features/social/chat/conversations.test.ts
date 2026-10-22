import { describe, expect, it } from "vitest";
import type { ChatMessage, ChatState, Conversation, SocialTeam } from "@/lib/api/social-types";
import {
  chatMutedNow,
  conversationFace,
  crewConversation,
  crewOf,
  deletionOf,
  directConversation,
  newDay,
  peerOf,
  readable,
  sameGroup,
  sortMessages,
  tombstoneKind,
  unreadAll,
  unreadDirect,
} from "./conversations";

const ME = "100000000000000001";
const AYSE = "100000000000000002";

function conversation(id: string, unread: number, extra: Partial<Conversation> = {}): Conversation {
  return { id, unread, writable: true, failed: false, ...extra };
}

function message(key: string, at: number, extra: Partial<ChatMessage> = {}): ChatMessage {
  return { key, from: AYSE, mine: false, text: key, at, edited: false, state: "sent", ...extra };
}

describe("conversations", () => {
  it("are named the way the service names them", () => {
    expect(directConversation(AYSE, ME)).toBe(`d:${ME}:${AYSE}`);
    // A shorter id is the smaller one, whatever its digits.
    expect(directConversation("900000000000000000", "10000000000000000")).toBe(
      "d:10000000000000000:900000000000000000",
    );
    expect(crewConversation("0123456789abcdef")).toBe("c:0123456789abcdef");
    expect(crewOf("c:0123456789abcdef")).toBe("0123456789abcdef");
    expect(crewOf(`d:${ME}:${AYSE}`)).toBeNull();
    expect(peerOf(`d:${ME}:${AYSE}`, ME)).toBe(AYSE);
    expect(peerOf(`d:${ME}:${AYSE}`, AYSE)).toBe(ME);
    expect(peerOf("c:0123456789abcdef", ME)).toBeNull();
  });

  it("count unread direct messages unless muted here; crew chat only in the list", () => {
    const chat: ChatState = {
      on: true,
      mailbox: true,
      perDay: 500,
      conversations: [
        conversation(`d:${ME}:${AYSE}`, 2, { peer: AYSE }),
        conversation("d:1:2", 1, { peer: "2" }),
        conversation("c:0123456789abcdef", 5, { crew: "0123456789abcdef" }),
      ],
    };
    expect(unreadAll(chat)).toBe(8);
    expect(unreadDirect(chat, {}, 0)).toBe(3);
    expect(unreadDirect(chat, { "d:1:2": null }, 0)).toBe(2);
    // A timed mute ends by itself.
    expect(chatMutedNow("d:1:2", { "d:1:2": 1_000 }, 999)).toBe(true);
    expect(chatMutedNow("d:1:2", { "d:1:2": 1_000 }, 1_000)).toBe(false);
    expect(unreadDirect(undefined, {}, 0)).toBe(0);
  });

  it("say who they are with, from the lists or as last known", () => {
    const view = {
      friends: [
        {
          id: AYSE,
          name: "Ayşe",
          username: "ayse",
          avatar: "discord",
          state: "launcher" as const,
          dnd: false,
          discordUrl: "",
        },
      ],
      crews: [],
    };
    expect(conversationFace({ id: "d", peer: AYSE, title: "Eski ad" }, view).name).toBe("Ayşe");
    expect(conversationFace({ id: "d", peer: "9", title: "Bora" }, view).name).toBe("Bora");
    expect(conversationFace({ id: "c", crew: "x", title: "Ekip" }, view).name).toBe("Ekip");
  });

  it("say who deleted a message in its tombstone", () => {
    const crew = {
      members: [
        { id: ME, role: "founder" as const },
        { id: AYSE, role: "member" as const },
        { id: "3", role: "member" as const },
      ],
    } as unknown as Pick<SocialTeam, "members">;
    // The player's own deletion, of their message or (leading the crew) of anyone's.
    expect(tombstoneKind(ME, ME, ME)).toBe("mine");
    expect(tombstoneKind(ME, AYSE, ME, crew)).toBe("mine");
    // The sender's own, or nobody known (a deleted account): "Bu mesaj silindi".
    expect(tombstoneKind(AYSE, AYSE, ME)).toBe("sender");
    expect(tombstoneKind("", AYSE, ME, crew)).toBe("sender");
    expect(tombstoneKind("staff", AYSE, ME, crew)).toBe("staff");
    // Someone else in crew chat: its leader, else a moderator (one who left included).
    const led = { members: [{ id: "3", role: "founder" as const }] } as unknown as Pick<
      SocialTeam,
      "members"
    >;
    expect(tombstoneKind("3", AYSE, ME, led)).toBe("leader");
    expect(tombstoneKind("3", AYSE, ME, crew)).toBe("moderator");
    expect(tombstoneKind("9", AYSE, ME, crew)).toBe("moderator");
    // A direct message is deleted only by its sender or moderators.
    expect(tombstoneKind("9", AYSE, ME)).toBe("sender");
  });

  it("show the player's own deletion at once and never count a tombstone", () => {
    const gone = message("i:3", 30, { id: 3, text: "", deleted: { by: AYSE, at: 31 } });
    expect(deletionOf(gone, {}, ME)).toEqual({ by: AYSE, at: 31 });
    const asked = message("i:4", 40, { id: 4 });
    expect(deletionOf(asked, { "i:4": true }, ME)).toEqual({ by: ME });
    expect(deletionOf(asked, {}, ME)).toBeUndefined();
    expect(readable(gone)).toBe(false);
    expect(readable(asked)).toBe(true);
  });

  it("keep their order and group a sender's messages", () => {
    const sent = [
      message("i:2", 20, { id: 2 }),
      message("n:a", 25, { state: "sending", mine: true, from: ME }),
      message("i:1", 10, { id: 1 }),
    ];
    expect(sortMessages(sent).map((m) => m.key)).toEqual(["i:1", "i:2", "n:a"]);
    const first = message("i:1", 1_000);
    expect(sameGroup(first, message("i:2", 1_000 + 60_000))).toBe(true);
    expect(sameGroup(first, message("i:3", 1_000 + 10 * 60_000))).toBe(false);
    expect(sameGroup(first, message("i:4", 2_000, { from: ME }))).toBe(false);
    expect(newDay(undefined, first)).toBe(true);
    expect(newDay(first, message("i:5", 1_000 + 86_400_000))).toBe(true);
  });
});
