// SQOL §2.2, §3: the launcher's room chat events and answers, by SQOL-R's names (the generated
// bindings' `RoomChatMsg`, `VoiceChatEvent`, `RoomChatSend`, `RoomChatError`, `RoomChatReason`).
import { describe, expect, it } from "vitest";
import type { RoomChatError, RoomChatReason } from "@/bindings";
import { codeOf, eventOf, msgOf, reasonOf, sendOf } from "./types";

describe("msgOf", () => {
  it("reads words and stickers as SQOL-R sends them (the other one null)", () => {
    expect(msgOf({ id: "a", from: "301", at: 5, text: "selam", sticker: null })).toEqual({
      id: "a",
      from: "301",
      at: 5,
      text: "selam",
    });
    expect(msgOf({ id: "b", from: "301", at: 6, text: null, sticker: "pl1/gg" })).toEqual({
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
    expect(msgOf({ id: "a", from: "301", at: 5, text: null, sticker: null })).toBeNull();
    expect(msgOf(null)).toBeNull();
  });
});

describe("eventOf", () => {
  it("reads a message, a deletion and the whole log", () => {
    const msg = { id: "a", from: "1", at: 1, text: "x", sticker: null };
    expect(eventOf({ kind: "msg", room: "r", msg })).toEqual({
      room: "r",
      kind: "msg",
      msg: { id: "a", from: "1", at: 1, text: "x" },
    });
    expect(eventOf({ kind: "del", room: "r", id: "a", by: "mod" })).toEqual({
      room: "r",
      kind: "del",
      id: "a",
      by: "mod",
    });
    expect(eventOf({ kind: "log", room: "r", msgs: [msg, { id: "b" }] })).toEqual({
      room: "r",
      kind: "log",
      msgs: [{ id: "a", from: "1", at: 1, text: "x" }],
    });
  });

  it("takes an unknown deleter as the sender, and leaves unknown events alone", () => {
    expect(eventOf({ room: "r", kind: "del", id: "a", by: "?" })).toMatchObject({ by: "self" });
    expect(eventOf({ room: "r", kind: "typing" })).toBeNull();
    expect(eventOf({ kind: "msg" })).toBeNull();
  });
});

describe("a refusal's names", () => {
  it("keeps every RoomChatError and RoomChatReason as SQOL-R names it", () => {
    const codes: RoomChatError[] = [
      "off",
      "stickersOff",
      "rejected",
      "rateLimited",
      "muted",
      "limited",
      "notAllowed",
      "gone",
      "failed",
    ];
    for (const code of codes) expect(codeOf(code)).toBe(code);
    const reasons: RoomChatReason[] = ["phone", "link", "words", "length"];
    for (const reason of reasons) expect(reasonOf(reason)).toBe(reason);
  });

  it("reads a name it does not know as a failure, and no reason", () => {
    expect(codeOf("chat_feature_off")).toBe("failed");
    expect(codeOf(undefined)).toBe("failed");
    // The friends' chat's old singular is not SQOL-R's.
    expect(reasonOf("word")).toBeUndefined();
    expect(reasonOf("lines")).toBeUndefined();
    expect(reasonOf(null)).toBeUndefined();
  });
});

describe("sendOf", () => {
  const refused = (error: string, extra: Record<string, unknown> = {}) => ({
    kind: "refused",
    error,
    reason: null,
    rule: null,
    retryAfter: null,
    until: null,
    ...extra,
  });

  it("reads what the room took", () => {
    expect(sendOf({ kind: "sent", id: "x", at: 9 })).toEqual({ sent: { id: "x", at: 9 } });
    expect(sendOf({ kind: "sent", id: "x" }, 42)).toEqual({ sent: { id: "x", at: 42 } });
  });

  it("reads each of SQOL-R's refusals with its reason and rule", () => {
    expect(sendOf(refused("rejected", { reason: "phone" }))).toEqual({
      refused: { code: "rejected", reason: "phone" },
    });
    expect(sendOf(refused("rejected", { reason: "link", rule: "shortener" }))).toEqual({
      refused: { code: "rejected", reason: "link", rule: "shortener" },
    });
    expect(sendOf(refused("rejected", { reason: "words" }))).toEqual({
      refused: { code: "rejected", reason: "words" },
    });
    expect(sendOf(refused("rejected", { reason: "length" }))).toEqual({
      refused: { code: "rejected", reason: "length" },
    });
    expect(sendOf(refused("stickersOff"))).toEqual({ refused: { code: "stickersOff" } });
    expect(sendOf(refused("rateLimited", { retryAfter: 3 }))).toEqual({
      refused: { code: "rateLimited" },
    });
    expect(sendOf(refused("muted", { until: 5 }))).toEqual({ refused: { code: "muted" } });
    expect(sendOf(refused("limited", { until: 5 }))).toEqual({ refused: { code: "limited" } });
    expect(sendOf(refused("notAllowed"))).toEqual({ refused: { code: "notAllowed" } });
    expect(sendOf(refused("off"))).toEqual({ refused: { code: "off" } });
    expect(sendOf(refused("gone"))).toEqual({ refused: { code: "gone" } });
  });

  it("reads anything else as not sent", () => {
    expect(sendOf(refused("something_new"))).toEqual({ refused: { code: "failed" } });
    expect(sendOf({ kind: "refused" })).toEqual({ refused: { code: "failed" } });
    expect(sendOf({ id: "x" })).toEqual({ refused: { code: "failed" } });
    expect(sendOf(null)).toEqual({ refused: { code: "failed" } });
  });
});
