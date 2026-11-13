// SQOL §1.3: which chat the room panel shows, how the room's refusals read, the rows as chat
// messages, tombstones, and the previews' timing.
import { describe, expect, it } from "vitest";
import {
  asChatMessage,
  type ChatChoiceInput,
  chatChoice,
  failureOf,
  heldPeeks,
  livePeeks,
  nextPeekEnd,
  PEEK_MAX,
  PEEK_MS,
  type Peek,
  type ShownMsg,
  shortLine,
  tombstoneOf,
  withPeek,
} from "./rules";

const ME = "100";
const FRIEND = "302";
const OTHER = "909";

const person = (id: string) => ({ id }) as ChatChoiceInput["room"]["people"][number];

type InputChange = Omit<Partial<ChatChoiceInput>, "room"> & {
  room?: Partial<ChatChoiceInput["room"]>;
};

function input(extra: InputChange = {}) {
  const { room, ...rest } = extra;
  return {
    room: { kind: "friend" as const, people: [person(FRIEND)], chat: true, grew: false, ...room },
    calling: null,
    me: ME,
    friends: [FRIEND],
    chatOn: true,
    latched: false,
    ...rest,
  } satisfies ChatChoiceInput;
}

describe("chatChoice", () => {
  it("shows the friend's conversation in a call of two that never grew", () => {
    expect(chatChoice(input())).toEqual({ kind: "dm", conv: "d:100:302", friend: FRIEND });
  });

  it("shows it while the player's call still rings at the friend", () => {
    expect(chatChoice(input({ room: { people: [] }, calling: { friend: FRIEND } }))).toEqual({
      kind: "dm",
      conv: "d:100:302",
      friend: FRIEND,
    });
  });

  it("shows the room's chat once a third came, and keeps it when they left", () => {
    expect(chatChoice(input({ room: { people: [person(FRIEND), person(OTHER)] } }))).toEqual({
      kind: "room",
    });
    // Back to two: the room says it grew (`grew`), or this computer's latch holds it.
    expect(chatChoice(input({ room: { grew: true } }))).toEqual({ kind: "room" });
    expect(chatChoice(input({ latched: true }))).toEqual({ kind: "room" });
  });

  it("shows the room's chat with someone who is no friend, or without chat on", () => {
    expect(chatChoice(input({ friends: [] }))).toEqual({ kind: "room" });
    expect(chatChoice(input({ chatOn: false }))).toEqual({ kind: "room" });
  });

  it("shows the room's chat in a crew room, even with two", () => {
    expect(chatChoice(input({ room: { kind: "crew" } }))).toEqual({ kind: "room" });
  });

  it("shows nothing without the room's chat when no conversation fits", () => {
    expect(chatChoice(input({ room: { kind: "crew", chat: false } }))).toEqual({ kind: "none" });
    expect(chatChoice(input({ friends: [], room: { chat: undefined } }))).toEqual({
      kind: "none",
    });
    // The friend's conversation needs no room chat at all.
    expect(chatChoice(input({ room: { chat: false } })).kind).toBe("dm");
  });

  it("orders the conversation's ids as the service does", () => {
    const choice = chatChoice(
      input({ me: "1000", friends: ["99"], room: { people: [person("99")] } }),
    );
    expect(choice).toEqual({ kind: "dm", conv: "d:99:1000", friend: "99" });
  });
});

describe("failureOf", () => {
  it("reads the room's refusals as chat's own failures", () => {
    expect(failureOf({ code: "rejected", reason: "phone" })).toEqual({ failure: "phone" });
    expect(failureOf({ code: "rejected", reason: "link", rule: "shortener" })).toEqual({
      failure: "link",
      rule: "shortener",
    });
    expect(failureOf({ code: "rejected", reason: "word" })).toEqual({ failure: "words" });
    expect(failureOf({ code: "rejected", reason: "lines" })).toEqual({ failure: "invalid" });
    expect(failureOf({ code: "rateLimited" })).toEqual({ failure: "tooFast" });
    expect(failureOf({ code: "muted" })).toEqual({ failure: "muted" });
    expect(failureOf({ code: "limited" })).toEqual({ failure: "limitedLinks" });
    expect(failureOf({ code: "off" })).toEqual({ failure: "off" });
    expect(failureOf({ code: "failed" })).toEqual({ failure: "connection" });
  });
});

const shown = (extra: Partial<ShownMsg> = {}): ShownMsg => ({
  key: "r:a",
  id: "a",
  seq: 4,
  from: OTHER,
  mine: false,
  at: 1_000,
  text: "selam",
  state: "sent",
  ...extra,
});

describe("asChatMessage", () => {
  it("gives a row its number, and a way to take it away only to those who may", () => {
    const message = asChatMessage(shown(), true);
    expect(message).toMatchObject({ key: "r:a", id: 4, from: OTHER, text: "selam", edited: false });
    expect(message.deleteUntil).toBe(Number.MAX_SAFE_INTEGER);
    expect(asChatMessage(shown(), false).deleteUntil).toBeUndefined();
  });

  it("keeps the player's own on its way without a number (no menu yet)", () => {
    const message = asChatMessage(
      shown({ key: "n:x", id: undefined, mine: true, state: "sending" }),
      true,
    );
    expect(message.id).toBeUndefined();
    expect(message.deleteUntil).toBeUndefined();
    expect(message.state).toBe("sending");
  });

  it("carries a refusal and a tombstone", () => {
    expect(
      asChatMessage(shown({ state: "failed", failure: "link", failureRule: "ip" }), false),
    ).toMatchObject({ state: "failed", failure: "link", failureRule: "ip" });
    const gone = asChatMessage(shown({ deleted: { by: "mod" } }), true);
    expect(gone.deleted).toEqual({ by: "mod" });
    expect(gone.deleteUntil).toBeUndefined();
  });
});

describe("tombstoneOf", () => {
  it("says who took it away", () => {
    expect(tombstoneOf({ by: "me" }, false)).toBe("mine");
    expect(tombstoneOf({ by: "self" }, true)).toBe("mine");
    expect(tombstoneOf({ by: "self" }, false)).toBe("sender");
    expect(tombstoneOf({ by: "mod" }, true)).toBe("mod");
    expect(tombstoneOf({ by: "staff" }, false)).toBe("staff");
  });
});

const peek = (key: string, until: number): Peek => ({ key, from: OTHER, text: key, until });

describe("previews", () => {
  it("hold at most three, the newest last", () => {
    let list: Peek[] = [];
    for (const key of ["a", "b", "c", "d"]) list = withPeek(list, peek(key, 10));
    expect(list.map((p) => p.key)).toEqual(["b", "c", "d"]);
    expect(PEEK_MAX).toBe(3);
  });

  it("each go 6 s after they came, one timer for the next", () => {
    expect(PEEK_MS).toBe(6_000);
    const list = [peek("a", 6_000), peek("b", 7_500)];
    expect(nextPeekEnd(list, 0)).toBe(6_000);
    expect(livePeeks(list, 6_000).map((p) => p.key)).toEqual(["b"]);
    expect(nextPeekEnd(list, 6_000)).toBe(7_500);
    expect(nextPeekEnd(list, 7_500)).toBeNull();
    expect(livePeeks(list, 7_500)).toEqual([]);
  });

  it("wait while the pointer rests on them", () => {
    const list = [peek("a", 6_000)];
    expect(heldPeeks(list, 2_000)[0]?.until).toBe(8_000);
    expect(heldPeeks(list, 0)[0]?.until).toBe(6_000);
  });
});

describe("shortLine", () => {
  it("is one line, cut with an ellipsis", () => {
    expect(shortLine("a\n b   c")).toBe("a b c");
    expect(shortLine("x".repeat(200), 10)).toBe(`${"x".repeat(9)}…`);
  });
});
