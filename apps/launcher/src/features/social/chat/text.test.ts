import { describe, expect, it } from "vitest";
import {
  CHAT_TEXT_MAX,
  chatLength,
  cleanChatText,
  hasText,
  looksLikeLink,
  looksLikePhone,
  textProblem,
} from "./text";

describe("chat text", () => {
  it("is cleaned the way the service keeps it", () => {
    expect(cleanChatText("  selam    naber  ")).toBe("selam naber");
    expect(cleanChatText("a\r\n\r\nb\n\n\nc")).toBe("a\nb\nc");
    expect(cleanChatText("1\n2\n3\n4\n5\n6\n7")).toBe("1\n2\n3\n4\n5 6 7");
    // A zero width joiner inside a word goes; between two emoji it stays.
    const joiner = String.fromCharCode(0x200d);
    expect(cleanChatText(`ab${joiner}cd`)).toBe("abcd");
    expect(cleanChatText(`👩${joiner}💻`)).toBe(`👩${joiner}💻`);
    expect(chatLength("👍👍")).toBe(2);
    expect(hasText("   \n ")).toBe(false);
    expect(hasText("selam")).toBe(true);
  });

  it("finds links however they are written, but lets projects.gg through", () => {
    for (const text of [
      "discord.gg/abc",
      "gel discord gg/abc",
      "https://example.com",
      "www.bedava-elmas.net",
      "site(.)com'a gir",
      "youtube . com",
      "kanal nokta com",
      "185.12.34.56:25565",
      "ѕite.com",
    ]) {
      expect(looksLikeLink(text), text).toBe(true);
    }
    for (const text of [
      "play.projects.gg'ye gel",
      "projects.gg",
      "saat 12.30'da",
      "3.5 kalp kaldı",
      "Geliyorum",
      "sa as",
    ]) {
      expect(looksLikeLink(text), text).toBe(false);
    }
  });

  it("finds phone numbers but not game coordinates", () => {
    for (const text of ["0532 123 45 67", "+90 532 123 45 67", "5321234567", "0212 345 67 89"]) {
      expect(looksLikePhone(text), text).toBe(true);
    }
    for (const text of ["x 1234 y 64 z -5678", "skor 120 - 98", "2026"]) {
      expect(looksLikePhone(text), text).toBe(false);
    }
  });

  it("says why a text cannot go", () => {
    expect(textProblem("")).toBeNull();
    expect(textProblem("selam")).toBeNull();
    expect(textProblem("discord.gg/x")).toBe("link");
    expect(textProblem("0532 123 45 67")).toBe("phone");
    expect(textProblem("a".repeat(CHAT_TEXT_MAX))).toBeNull();
    expect(textProblem("a".repeat(CHAT_TEXT_MAX + 1))).toBe("long");
    // Spaces the service collapses do not count against the limit.
    expect(textProblem(`${"a ".repeat(150)}    `)).toBeNull();
  });
});
