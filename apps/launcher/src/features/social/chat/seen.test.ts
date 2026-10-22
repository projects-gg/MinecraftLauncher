import { describe, expect, it } from "vitest";
import type { ChatMessage } from "@/lib/api/social-types";
import { newestTheirs, seenKey } from "./seen";

const msg = (id: number, mine: boolean, extra: Partial<ChatMessage> = {}): ChatMessage => ({
  key: `i:${id}`,
  id,
  from: mine ? "100" : "301",
  mine,
  text: "x",
  at: id,
  edited: false,
  state: "sent",
  ...extra,
});

describe("seenKey", () => {
  it("marks the player's last message the friend has seen", () => {
    const messages = [msg(1, false), msg(2, true), msg(3, true)];
    expect(seenKey(messages, 3)).toBe("i:3");
    expect(seenKey(messages, 2)).toBe("i:2");
    expect(seenKey(messages, 1)).toBeNull();
  });

  it("says nothing once the friend answered", () => {
    expect(seenKey([msg(1, true), msg(2, false)], 2)).toBeNull();
    expect(seenKey([msg(1, true), msg(2, false, { deleted: { by: "301" } })], 2)).toBeNull();
  });

  it("skips what is on its way, failed or deleted, and needs a mark", () => {
    const messages = [
      msg(1, true),
      msg(2, true, { deleted: { by: "100" } }),
      { ...msg(3, true), id: undefined, key: "n:a", state: "sending" as const },
    ];
    expect(seenKey(messages, 5)).toBe("i:1");
    expect(seenKey(messages, undefined)).toBeNull();
  });
});

describe("newestTheirs", () => {
  it("is the friend's newest message while the player has not written after it", () => {
    expect(newestTheirs([msg(1, false), msg(2, false)])?.key).toBe("i:2");
    expect(newestTheirs([msg(1, false), msg(2, true)])).toBeNull();
    expect(newestTheirs([msg(1, false), msg(2, false, { deleted: { by: "301" } })])?.key).toBe(
      "i:1",
    );
  });
});
