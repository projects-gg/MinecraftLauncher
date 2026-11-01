import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  CHAT_GATHER_MS,
  CHAT_REPOP_MS,
  type ChatNoticeContext,
  type ChatNoticeSink,
  ChatNotices,
  type ChatNoticeWay,
  chatNoticeWay,
  type GatheredChatNotice,
  preview,
} from "./notify";

const base: ChatNoticeContext = {
  kind: "direct",
  status: "online",
  gameBusy: false,
  inFront: true,
  onScreen: false,
  mutedHere: false,
  crewMuted: false,
  crewChatOn: false,
  toMe: false,
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

  it("tell a reply to the player in a quiet crew, as a mention would be (CHAT-CALM)", () => {
    const reply = { ...base, kind: "crew" as const, toMe: true };
    expect(chatNoticeWay(reply)).toBe("toast");
    expect(chatNoticeWay({ ...reply, crewMuted: true, inFront: false })).toBe("system");
    // Never past the player's own quiet: this chat muted here, do-not-disturb, a game.
    expect(chatNoticeWay({ ...reply, mutedHere: true })).toBe("none");
    expect(chatNoticeWay({ ...reply, status: "dnd" })).toBe("none");
    expect(chatNoticeWay({ ...reply, gameBusy: true })).toBe("none");
    expect(chatNoticeWay({ ...reply, onScreen: true })).toBe("none");
  });

  it("shorten long messages to one line", () => {
    expect(preview("  selam\n  naber ")).toBe("selam naber");
    const long = preview("a".repeat(200));
    expect([...long].length).toBe(90);
    expect(long.endsWith("…")).toBe(true);
  });
});

describe("gathered chat notices (CHAT-CALM)", () => {
  let way: ChatNoticeWay;
  let shown: { notice: GatheredChatNotice; way: "toast" | "system" }[];
  let cleared: string[];
  let notices: ChatNotices;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(1_800_000_000_000);
    way = "system";
    shown = [];
    cleared = [];
    const sink: ChatNoticeSink = {
      way: () => way,
      show: (notice, how) => shown.push({ notice, way: how }),
      clear: (conv) => cleared.push(conv),
    };
    notices = new ChatNotices(sink);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const say = (conv: string, text: string) =>
    notices.add(conv, { name: "Kaan_07", text, crew: conv.startsWith("c:") ? "GKR" : undefined });

  it("wait a moment, then tell a burst once", () => {
    for (let i = 1; i <= 6; i++) say("d:1:2", `mesaj ${i}`);
    expect(shown).toHaveLength(0);
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    expect(shown).toHaveLength(1);
    expect(shown[0]?.notice).toMatchObject({ conv: "d:1:2", count: 6, popup: true });
    expect(shown[0]?.notice.last.text).toBe("mesaj 6");
  });

  it("keep one notice per conversation and change it in place between pop-ups", () => {
    say("d:1:2", "selam");
    say("c:t1", "toplanın");
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    expect(shown.map((s) => [s.notice.conv, s.notice.count, s.notice.popup])).toEqual([
      ["d:1:2", 1, true],
      ["c:t1", 1, true],
    ]);
    // More a minute later: the same notice says the new count, without popping up.
    vi.advanceTimersByTime(60_000);
    say("d:1:2", "naber");
    say("d:1:2", "orada mısın");
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    expect(shown.at(-1)?.notice).toMatchObject({ conv: "d:1:2", count: 3, popup: false });
    // Two minutes after the last pop-up it may pop up again.
    vi.advanceTimersByTime(CHAT_REPOP_MS);
    say("d:1:2", "hey");
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    expect(shown.at(-1)?.notice).toMatchObject({ count: 4, popup: true });
  });

  it("say nothing while the conversation is on screen and start over once it is seen", () => {
    say("d:1:2", "selam");
    way = "none";
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    expect(shown).toHaveLength(0);
    notices.seen("d:1:2");
    expect(cleared).toEqual(["d:1:2"]);
    expect(notices.open()).toEqual([]);
    way = "system";
    say("d:1:2", "yeni");
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    expect(shown[0]?.notice).toMatchObject({ count: 1, popup: true });
  });

  it("change an in-app toast in place and never pop it as a system notification", () => {
    way = "toast";
    say("d:1:2", "selam");
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    say("d:1:2", "naber");
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    expect(shown.map((s) => [s.way, s.notice.count, s.notice.popup])).toEqual([
      ["toast", 1, false],
      ["toast", 2, false],
    ]);
    // The window goes away: the first system notification pops up.
    way = "system";
    say("d:1:2", "hey");
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    expect(shown.at(-1)).toMatchObject({ way: "system", notice: { count: 3, popup: true } });
  });

  it("forget everything for another account", () => {
    say("d:1:2", "selam");
    notices.forget();
    vi.advanceTimersByTime(CHAT_GATHER_MS);
    expect(shown).toHaveLength(0);
    expect(cleared).toEqual(["d:1:2"]);
  });
});
