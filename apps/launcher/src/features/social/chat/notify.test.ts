import { describe, expect, it } from "vitest";
import { type ChatNoticeContext, chatNoticeWay, preview } from "./notify";

const base: ChatNoticeContext = {
  kind: "direct",
  status: "online",
  gameBusy: false,
  inFront: true,
  onScreen: false,
  mutedHere: false,
  crewMuted: false,
  crewChatOn: false,
};

describe("chat notices", () => {
  it("tell a direct message as a toast, or outside the launcher as a system notification", () => {
    expect(chatNoticeWay(base)).toBe("toast");
    expect(chatNoticeWay({ ...base, inFront: false })).toBe("system");
    // Already on screen: nothing to tell.
    expect(chatNoticeWay({ ...base, onScreen: true })).toBe("none");
    // On screen but the window is away: the player is not looking.
    expect(chatNoticeWay({ ...base, onScreen: true, inFront: false })).toBe("system");
  });

  it("stay quiet in game, on do-not-disturb and where muted", () => {
    expect(chatNoticeWay({ ...base, gameBusy: true, inFront: false })).toBe("none");
    expect(chatNoticeWay({ ...base, status: "dnd" })).toBe("none");
    expect(chatNoticeWay({ ...base, mutedHere: true })).toBe("none");
  });

  it("count crew chat only, unless its notifications are on and the crew is not muted", () => {
    const crew = { ...base, kind: "crew" as const };
    expect(chatNoticeWay(crew)).toBe("none");
    expect(chatNoticeWay({ ...crew, crewChatOn: true })).toBe("toast");
    expect(chatNoticeWay({ ...crew, crewChatOn: true, crewMuted: true })).toBe("none");
  });

  it("shorten long messages to one line", () => {
    expect(preview("  selam\n  naber ")).toBe("selam naber");
    const long = preview("a".repeat(200));
    expect([...long].length).toBe(90);
    expect(long.endsWith("…")).toBe(true);
  });
});
