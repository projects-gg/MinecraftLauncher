// SQOL §2.2, §3: the launcher's room chat events and answers, read field by field (the room's own
// wire names too).
import { describe, expect, it } from "vitest";
import { eventOf, msgOf, sendOf } from "./types";

describe("msgOf", () => {
  it("reads words and stickers, under either name", () => {
    expect(msgOf({ id: "a", from: "301", at: 5, text: "selam" })).toEqual({
      id: "a",
      from: "301",
      at: 5,
      text: "selam",
    });
    expect(msgOf({ id: "b", u: "301", at: 6, st: "pl1/gg" })).toEqual({
      id: "b",
      from: "301",
      at: 6,
      text: "",
      sticker: "pl1/gg",
    });
  });

  it("is nothing without an id, a sender, a time or a body", () => {
    expect(msgOf({ from: "301", at: 5, text: "x" })).toBeNull();
    expect(msgOf({ id: "a", at: 5, text: "x" })).toBeNull();
    expect(msgOf({ id: "a", from: "301", text: "x" })).toBeNull();
    expect(msgOf({ id: "a", from: "301", at: 5, text: "" })).toBeNull();
    expect(msgOf(null)).toBeNull();
  });
});

describe("eventOf", () => {
  it("reads a message, a deletion and the last messages", () => {
    expect(
      eventOf({ room: "r", kind: "msg", msg: { id: "a", from: "1", at: 1, text: "x" } }),
    ).toEqual({ room: "r", kind: "msg", msg: { id: "a", from: "1", at: 1, text: "x" } });
    expect(eventOf({ room: "r", kind: "del", id: "a", by: "mod" })).toEqual({
      room: "r",
      kind: "del",
      id: "a",
      by: "mod",
    });
    expect(
      eventOf({ room: "r", kind: "log", ms: [{ id: "a", u: "1", at: 1, text: "x" }, { id: "b" }] }),
    ).toEqual({ room: "r", kind: "log", msgs: [{ id: "a", from: "1", at: 1, text: "x" }] });
  });

  it("takes an unknown deleter as the sender, and leaves unknown events alone", () => {
    expect(eventOf({ room: "r", kind: "del", id: "a", by: "?" })).toMatchObject({ by: "self" });
    expect(eventOf({ room: "r", kind: "typing" })).toBeNull();
    expect(eventOf({ kind: "msg" })).toBeNull();
  });
});

describe("sendOf", () => {
  it("reads what the room took (SQOL-R's answer)", () => {
    expect(sendOf({ kind: "sent", id: "x", at: 9 })).toEqual({ sent: { id: "x", at: 9 } });
    expect(sendOf({ id: "x" }, 42)).toEqual({ sent: { id: "x", at: 42 } });
  });

  it("reads SQOL-R's refusals", () => {
    expect(
      sendOf({
        kind: "refused",
        error: "rejected",
        reason: "phone",
        rule: null,
        retryAfter: null,
        until: null,
      }),
    ).toEqual({ refused: { code: "rejected", reason: "phone" } });
    expect(
      sendOf({ kind: "refused", error: "rateLimited", reason: null, rule: null, retryAfter: 3 }),
    ).toEqual({ refused: { code: "rateLimited" } });
    expect(sendOf({ kind: "refused", error: "rejected", reason: "link", rule: "ip" })).toEqual({
      refused: { code: "rejected", reason: "link", rule: "ip" },
    });
  });

  it("reads the room's own codes, flat or nested", () => {
    expect(sendOf({ refused: { code: "chat_rejected", reason: "word" } })).toEqual({
      refused: { code: "rejected", reason: "word" },
    });
    expect(sendOf({ code: "rate_limited" })).toEqual({ refused: { code: "rateLimited" } });
    expect(sendOf({ code: "chat_muted", until: 5 })).toEqual({ refused: { code: "muted" } });
    expect(sendOf({ refused: "something_new" })).toEqual({ refused: { code: "failed" } });
    expect(sendOf({ kind: "refused" })).toEqual({ refused: { code: "failed" } });
    expect(sendOf(null)).toEqual({ refused: { code: "failed" } });
  });
});
