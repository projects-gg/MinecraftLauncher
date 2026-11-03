import { describe, expect, it } from "vitest";
import {
  MENTION_NOTIFY_MAX,
  memberChoices,
  mentionOf,
  mentionSpans,
  mentionsIn,
  putSuggestion,
  suggestToken,
} from "./mentions";

describe("suggestToken", () => {
  it("opens for a member after @ at a word's start", () => {
    expect(suggestToken("selam @Ka", 9)).toEqual({ kind: "member", query: "Ka", start: 6, end: 9 });
    expect(suggestToken("@", 1)).toEqual({ kind: "member", query: "", start: 0, end: 1 });
    // An address is no mention.
    expect(suggestToken("ali@site", 8)).toBeNull();
    expect(suggestToken("@Kaan\nyeni", 10)).toBeNull();
  });

  it("goes on over a space only while a member's name does", () => {
    expect(suggestToken("@Gece K", 7, ["Gece Kurdu"])?.query).toBe("Gece K");
    expect(suggestToken("@Gece K", 7, ["Kaan_07"])).toBeNull();
  });

  it("opens for an emoji after : and two letters, never after a digit", () => {
    expect(suggestToken("çok iyi :gü", 11)).toEqual({
      kind: "emoji",
      query: "gü",
      start: 8,
      end: 11,
    });
    expect(suggestToken(":fire", 5)?.kind).toBe("emoji");
    expect(suggestToken("saat 10:30", 10)).toBeNull();
    expect(suggestToken("10:30da", 7)).toBeNull();
    expect(suggestToken("hmm :g", 6)).toBeNull();
    expect(suggestToken("https://x.com", 13)).toBeNull();
  });
});

describe("memberChoices", () => {
  const members = [
    { id: "100", name: "Ben", online: true },
    { id: "101", name: "Kaan_07", online: false },
    { id: "102", name: "kartal", online: true },
    { id: "103", name: "Kuzenim", realName: "Zeyno_X", online: false },
    { id: "104", name: "İlker", online: false },
  ];

  it("filters by the name's start or a word's, online first, the player left out", () => {
    expect(memberChoices(members, "ka", "100").map((m) => m.id)).toEqual(["102", "101"]);
    expect(memberChoices(members, "07", "100").map((m) => m.id)).toEqual(["101"]);
    // A friend's nickname and real name both find them.
    expect(memberChoices(members, "zey", "100").map((m) => m.id)).toEqual(["103"]);
    expect(memberChoices(members, "ilk", "100").map((m) => m.id)).toEqual(["104"]);
    expect(memberChoices(members, "", "100", 2)).toHaveLength(2);
  });
});

describe("putSuggestion", () => {
  it("puts the choice in the token's place with a space after it", () => {
    expect(putSuggestion("selam @Ka nasılsın", { start: 6, end: 9 }, "@Kaan_07")).toEqual({
      value: "selam @Kaan_07 nasılsın",
      caret: 15,
    });
    expect(putSuggestion(":fi", { start: 0, end: 3 }, "🔥")).toEqual({ value: "🔥 ", caret: 3 });
  });
});

describe("mentions in a message", () => {
  const members = [
    { id: "101", name: "Kaan_07" },
    { id: "102", name: "Gece Kurdu" },
    { id: "103", name: "Gece" },
    { id: "104", name: "Işıl" },
  ];

  it("finds members at a word's edge, the longest name first, without case or marks", () => {
    expect(mentionsIn("@kaan_07 gel", members)).toEqual(["101"]);
    expect(mentionsIn("@Gece Kurdu ve @gece", members)).toEqual(["102", "103"]);
    expect(mentionsIn("selam @ISIL!", members)).toEqual(["104"]);
    expect(mentionsIn("mail@Kaan_07", members)).toEqual([]);
    expect(mentionsIn("@Kaan_07abc", members)).toEqual([]);
    expect(mentionsIn("bahsetme yok", members)).toEqual([]);
  });

  it("marks where each mention is", () => {
    expect(mentionSpans("hey @Kaan_07, bak", members)).toEqual([{ start: 4, end: 12, id: "101" }]);
  });

  it("tells the player unless more than five members are mentioned", () => {
    expect(mentionOf("@Kaan_07 bak", "101", members)).toBe("me");
    expect(mentionOf("@Gece bak", "101", members)).toBe("none");
    const crowd = Array.from({ length: MENTION_NOTIFY_MAX + 1 }, (_, i) => ({
      id: `20${i}`,
      name: `Oyuncu${i}`,
    }));
    const text = crowd.map((m) => `@${m.name}`).join(" ");
    expect(mentionOf(text, "200", crowd)).toBe("crowd");
    expect(mentionOf(text, undefined, crowd)).toBe("none");
  });
});
