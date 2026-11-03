import { describe, expect, it } from "vitest";
import type { ChatMessage } from "@/lib/api/social-types";
import {
  foldWithMap,
  matchRanges,
  mergeHistory,
  searchMessages,
  searchQuery,
  stepResult,
} from "./chat-search";

const message = (key: string, text: string, extra: Partial<ChatMessage> = {}): ChatMessage => ({
  key,
  id: Number(key.slice(2)),
  from: "101",
  mine: false,
  text,
  at: Number(key.slice(2)),
  edited: false,
  state: "sent",
  ...extra,
});

describe("searchQuery", () => {
  it("folds the query and waits for two characters", () => {
    expect(searchQuery("İ")).toBeNull();
    expect(searchQuery("  Şİ ")).toBe("si");
    expect(searchQuery("Iğ")).toBe("ig");
  });
});

describe("matchRanges", () => {
  it("finds every place without case or Turkish marks, as ranges of the text", () => {
    expect(matchRanges("Işık ışık ISIK", "isik")).toEqual([
      [0, 4],
      [5, 9],
      [10, 14],
    ]);
    expect(matchRanges("Saat 21:00'de sunucuda", "sunucu")).toEqual([[14, 20]]);
    expect(matchRanges("👍🏽 güzel", "guzel")).toEqual([[5, 10]]);
    expect(matchRanges("abc", "")).toEqual([]);
  });

  it("maps folded characters back to the original ones", () => {
    const map = foldWithMap("Ağ");
    expect(map.folded).toBe("ag");
    expect(map.from).toEqual([0, 1, 2]);
  });
});

describe("searchMessages", () => {
  const messages = [
    message("i:1", "sunucu adresi play.projects.gg"),
    message("i:2", "Çıkartma", { sticker: "pl1/gg", text: "" }),
    message("i:3", "Sunucuya gel", { deleted: { by: "101" } }),
    message("i:4", "SUNUCU açık"),
  ];

  it("finds messages the newest first, never tombstones or stickers", () => {
    expect(searchMessages(messages, "sunucu")).toEqual(["i:4", "i:1"]);
    expect(searchMessages(messages, "cikartma")).toEqual([]);
  });

  it("reads the words a message shows (an edit on its way)", () => {
    const shown = (m: ChatMessage) => (m.key === "i:1" ? "yeni metin" : m.text);
    expect(searchMessages(messages, "yeni", shown)).toEqual(["i:1"]);
  });
});

describe("mergeHistory", () => {
  it("joins the history and the shown messages once each, oldest first", () => {
    const history = [message("i:1", "a"), message("i:2", "b")];
    const shown = [message("i:2", "b (edited)"), message("i:3", "c")];
    expect(mergeHistory(history, shown).map((m) => m.text)).toEqual(["a", "b (edited)", "c"]);
  });
});

describe("stepResult", () => {
  it("goes older and newer, round at the ends", () => {
    expect(stepResult(-1, 3, 1)).toBe(0);
    expect(stepResult(0, 3, 1)).toBe(1);
    expect(stepResult(2, 3, 1)).toBe(0);
    expect(stepResult(0, 3, -1)).toBe(2);
    expect(stepResult(0, 0, 1)).toBe(-1);
  });
});
