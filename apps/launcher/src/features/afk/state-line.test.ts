import { describe, expect, it } from "vitest";
import { stateSignal } from "./state-line";

describe("stateSignal", () => {
  it("lights the bars in the game and crosses them when the connection dropped", () => {
    expect(stateSignal({ kind: "connected", since: "2026-10-18T10:00:00Z" })).toBe("online");
    expect(stateSignal({ kind: "error", reason: "Sunucu kapandı" })).toBe("offline");
  });

  it("sweeps only while a connection is being made", () => {
    expect(stateSignal({ kind: "starting" })).toBe("pending");
    expect(stateSignal({ kind: "connecting" })).toBe("pending");
    expect(stateSignal({ kind: "queued", inSecs: 15 })).toBe("idle");
    expect(
      stateSignal({ kind: "reconnecting", attempt: 2, nextAttemptAt: "2026-10-18T10:00:20Z" }),
    ).toBe("idle");
    expect(stateSignal({ kind: "stopping" })).toBe("idle");
    expect(stateSignal({ kind: "stopped" })).toBe("idle");
  });
});
