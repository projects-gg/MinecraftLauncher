// SQOL §1.3: the room chat's messages on the page: unread while out of sight, the previews, the
// player's own on their way, the room's last messages, deletions.
import { describe, expect, it } from "vitest";
import { PEEK_MS } from "./rules";
import {
  EMPTY_ROOM_CHAT,
  forRoom,
  KEPT_MAX,
  markDeleted,
  type RoomChatData,
  receive,
  receiveLog,
  retrySend,
  setShown,
  settleSend,
  startSend,
  undelete,
} from "./store";
import type { RoomChatMsg } from "./types";

const ME = "100";
const ROOM = "g0123";
const NOW = 1_700_000_000_000;

const msg = (id: string, from = "301", text = `m ${id}`, at = NOW): RoomChatMsg => ({
  id,
  from,
  at,
  text,
});

const start = (extra: Partial<RoomChatData> = {}): RoomChatData => ({
  ...forRoom(EMPTY_ROOM_CHAT, ROOM),
  loaded: true,
  ...extra,
});

const arrival = (peek = false) => ({ me: ME, peek, nowMs: NOW });

describe("unread", () => {
  it("counts others' messages while the chat is out of sight, not the player's own", () => {
    let data = start();
    data = receive(data, msg("a"), arrival());
    data = receive(data, msg("b"), arrival());
    data = receive(data, msg("c", ME), arrival());
    expect(data.unread).toBe(2);
    expect(data.msgs.map((m) => m.mine)).toEqual([false, false, true]);
  });

  it("counts nothing while the chat is on screen", () => {
    let data = setShown(start(), true);
    data = receive(data, msg("a"), arrival());
    expect(data.unread).toBe(0);
  });

  it("reads what waited when the chat comes on screen, the line over the first of it", () => {
    let data = receive(start(), msg("a"), arrival());
    data = setShown(data, true);
    data = setShown(data, false);
    data = receive(data, msg("b"), arrival());
    data = receive(data, msg("c", ME), arrival());
    data = receive(data, msg("d"), arrival());
    expect(data.unread).toBe(2);
    data = setShown(data, true);
    expect(data.unread).toBe(0);
    expect(data.divider).toBe("r:b");
    // Out of sight again, the line goes with it.
    expect(setShown(data, false).divider).toBeNull();
  });

  it("counts a message once, however often it comes", () => {
    let data = receive(start(), msg("a"), arrival());
    data = receive(data, msg("a"), arrival());
    expect(data.msgs).toHaveLength(1);
    expect(data.unread).toBe(1);
  });

  it("does not count the room's last messages (history)", () => {
    const data = receiveLog(start({ loaded: false }), [msg("a"), msg("b")], ME);
    expect(data.loaded).toBe(true);
    expect(data.unread).toBe(0);
    expect(data.msgs).toHaveLength(2);
  });

  it("starts again in another room", () => {
    const data = receive(start(), msg("a"), arrival());
    const next = forRoom(data, "other");
    expect(next.msgs).toEqual([]);
    expect(next.unread).toBe(0);
    expect(forRoom(data, ROOM)).toBe(data);
  });
});

describe("previews", () => {
  it("come only when asked for, out of sight, 6 s each", () => {
    let data = receive(start(), msg("a"), arrival(false));
    expect(data.peeks).toEqual([]);
    data = receive(data, msg("b"), arrival(true));
    expect(data.peeks).toEqual([{ key: "r:b", from: "301", text: "m b", until: NOW + PEEK_MS }]);
    // The player's own never.
    data = receive(data, msg("c", ME), arrival(true));
    expect(data.peeks).toHaveLength(1);
  });

  it("go when the chat comes on screen, or the message is taken away", () => {
    let data = receive(start(), msg("a"), arrival(true));
    data = receive(data, msg("b"), arrival(true));
    expect(markDeleted(data, "a", "mod").peeks.map((p) => p.key)).toEqual(["r:b"]);
    expect(setShown(data, true).peeks).toEqual([]);
  });
});

describe("the player's own messages", () => {
  it("show at once and settle with the room's answer", () => {
    let data = startSend(start(), "n1", { text: "selam" }, ME, NOW);
    expect(data.msgs[0]).toMatchObject({ key: "n:n1", state: "sending", mine: true });
    data = settleSend(data, "n1", { sent: { id: "x1", at: NOW + 5 } });
    expect(data.msgs[0]).toMatchObject({ key: "n:n1", id: "x1", at: NOW + 5, state: "sent" });
    // The room's copy coming after the answer adds nothing.
    data = receive(data, msg("x1", ME, "selam"), arrival());
    expect(data.msgs).toHaveLength(1);
  });

  it("settle with the room's copy when it comes before the answer", () => {
    let data = startSend(start(), "n1", { text: "selam" }, ME, NOW);
    data = receive(data, msg("x1", ME, "selam", NOW + 3), arrival());
    expect(data.msgs).toHaveLength(1);
    expect(data.msgs[0]).toMatchObject({ key: "n:n1", id: "x1", state: "sent" });
    data = settleSend(data, "n1", { sent: { id: "x1", at: NOW + 9 } });
    expect(data.msgs).toHaveLength(1);
    expect(data.msgs[0]?.at).toBe(NOW + 3);
  });

  it("take the words the room keeps from its copy", () => {
    let data = startSend(start(), "n1", { text: "selam  dünya" }, ME, NOW);
    data = settleSend(data, "n1", { sent: { id: "x1", at: NOW } });
    data = receive(data, msg("x1", ME, "selam dünya"), arrival());
    expect(data.msgs).toHaveLength(1);
    expect(data.msgs[0]).toMatchObject({ key: "n:n1", text: "selam dünya" });
  });

  it("drop a copy the room changed (its words) once the answer names it", () => {
    let data = startSend(start(), "n1", { text: "selam  " }, ME, NOW);
    data = receive(data, msg("x1", ME, "selam"), arrival());
    expect(data.msgs).toHaveLength(2);
    data = settleSend(data, "n1", { sent: { id: "x1", at: NOW } });
    expect(data.msgs.map((m) => m.key)).toEqual(["n:n1"]);
  });

  it("stay sent once the room's copy came, whatever a late answer says", () => {
    let data = startSend(start(), "n1", { text: "selam" }, ME, NOW);
    data = receive(data, msg("x1", ME, "selam", NOW + 3), arrival());
    // The answer timed out after the room took it.
    data = settleSend(data, "n1", { refused: { code: "failed" } });
    expect(data.msgs).toHaveLength(1);
    expect(data.msgs[0]).toMatchObject({ id: "x1", state: "sent" });
    expect(data.msgs[0]?.failure).toBeUndefined();
  });

  it("stay with the room's refusal in chat's words, and go again on a retry", () => {
    let data = startSend(start(), "n1", { text: "https://bit.ly/x" }, ME, NOW);
    data = settleSend(data, "n1", {
      refused: { code: "rejected", reason: "link", rule: "shortener" },
    });
    expect(data.msgs[0]).toMatchObject({
      state: "failed",
      failure: "link",
      failureRule: "shortener",
    });
    data = retrySend(data, "n:n1");
    expect(data.msgs[0]).toMatchObject({ state: "sending", failure: undefined });
  });
});

describe("the room's last messages", () => {
  it("replace what the page shows, in their order, the player's own on their way last", () => {
    let data = receive(start(), msg("old", "301", "old", NOW), arrival());
    data = receive(data, msg("b", "301", "b", NOW + 20), arrival());
    data = startSend(data, "n1", { text: "yolda" }, ME, NOW + 30);
    data = receiveLog(
      data,
      [
        msg("c", "303", "c", NOW + 25),
        msg("b", "301", "b", NOW + 20),
        msg("a", "301", "a", NOW + 10),
      ],
      ME,
    );
    // SQOL-R's log is the whole log: what the room no longer holds goes, older ones too.
    expect(data.msgs.map((m) => m.key)).toEqual(["r:a", "r:b", "r:c", "n:n1"]);
    // The message the page had keeps its number.
    expect(data.msgs[1]?.seq).toBe(2);
  });

  it("empty the list when the log is empty, keeping only the player's own on their way", () => {
    let data = receive(start(), msg("a", "301", "a", NOW + 10), arrival());
    data = startSend(data, "n1", { text: "yolda" }, ME, NOW + 30);
    data = receiveLog(data, [], ME);
    expect(data.msgs.map((m) => m.key)).toEqual(["n:n1"]);
    expect(data.loaded).toBe(true);
  });

  it("take away the unread count, the line and the previews of what went (someone blocked)", () => {
    let data = start({ shown: false });
    data = receive(data, msg("a", "301", "a", NOW + 10), arrival(true));
    data = receive(data, msg("b", "302", "b", NOW + 20), arrival(true));
    expect(data.unread).toBe(2);
    data = receiveLog(data, [msg("a", "301", "a", NOW + 10)], ME);
    expect(data.unread).toBe(1);
    expect(data.peeks.map((p) => p.key)).toEqual(["r:a"]);
  });

  it("keep at most KEPT_MAX messages", () => {
    const many = Array.from({ length: KEPT_MAX + 5 }, (_, i) => msg(`m${i}`, "301", "x", NOW + i));
    const data = receiveLog(start(), many, ME);
    expect(data.msgs).toHaveLength(KEPT_MAX);
    expect(data.msgs[0]?.id).toBe("m5");
  });
});

describe("deletions", () => {
  it("turn a message into its tombstone, the player's own saying so", () => {
    let data = receive(start(), msg("a"), arrival());
    data = markDeleted(data, "a", "me");
    expect(data.msgs[0]?.deleted).toEqual({ by: "me" });
    // The room's word afterwards does not change the player's own.
    expect(markDeleted(data, "a", "mod").msgs[0]?.deleted).toEqual({ by: "me" });
    expect(undelete(data, "a").msgs[0]?.deleted).toBeUndefined();
    // A message the page does not hold changes nothing.
    expect(markDeleted(data, "zzz", "staff")).toBe(data);
  });
});
