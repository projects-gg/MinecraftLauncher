import { describe, expect, it } from "vitest";
import table from "./chat-links.json";
import {
  CHAT_TEXT_MAX,
  chatLength,
  checkForm,
  cleanChatText,
  DEFAULT_LINK_LISTS,
  findLinks,
  hasText,
  LINK_RULES_VERSION,
  type LinkLists,
  lookalike,
  looksLikeLink,
  looksLikePhone,
  messageLinks,
  messageProblem,
  skeleton,
  textProblem,
  typoDistance,
} from "./text";
import shared from "./text-vectors.json";

describe("chat text", () => {
  it("is cleaned the way the service keeps it", () => {
    expect(cleanChatText("  selam    naber  ")).toBe("selam naber");
    expect(cleanChatText("a\r\n\r\nb\n\n\nc")).toBe("a\nb\nc");
    expect(cleanChatText("1\n2\n3\n4\n5\n6\n7")).toBe("1\n2\n3\n4\n5 6 7");
    // A zero width joiner inside a word goes; between two emoji it stays.
    const joiner = String.fromCharCode(0x200d);
    expect(cleanChatText(`ab${joiner}cd`)).toBe("abcd");
    expect(cleanChatText(`👩${joiner}💻`)).toBe(`👩${joiner}💻`);
    // Variation selectors nobody needs go; VS16 stays, one at a time (CHAT2 §3.4).
    const vs1 = String.fromCodePoint(0xfe00);
    const vs16 = String.fromCodePoint(0xfe0f);
    expect(cleanChatText(`a${vs1}b`)).toBe("ab");
    expect(cleanChatText(`❤${vs16}${vs16}${vs16}`)).toBe(`❤${vs16}`);
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
    // CHAT2: with Studio's lists a plain https link that passes the rules goes.
    expect(textProblem("bak https://youtu.be/dQw4w9WgXcQ")).toBe("link");
    expect(textProblem("bak https://youtu.be/dQw4w9WgXcQ", DEFAULT_LINK_LISTS)).toBeNull();
    expect(textProblem("https://bit.ly/x", DEFAULT_LINK_LISTS)).toBe("link");
    expect(messageProblem("https://bit.ly/x", DEFAULT_LINK_LISTS)).toEqual({
      reason: "link",
      rule: "shortener",
    });
    // 🖕 is the one listed word the launcher knows; the rest of the list is the service's.
    expect(messageProblem("🖕", null)).toEqual({ reason: "words" });
    expect(textProblem("🖕")).toBeNull();
  });

  it("reads symbols that spell as what they spell (CHAT2 §3.4)", () => {
    expect(checkForm("🇩🇮🇸🇨🇴🇷🇩.🇬🇬")).toBe("discord.gg");
    expect(checkForm("🅳🅸🆂🅲🅾🆁🅳")).toBe("discord");
    expect(checkForm("ᴅɪꜱᴄᴏʀᴅ")).toBe("discord");
    expect(checkForm("❶❷⓫🔟")).toBe("121110");
    // A flag whose letters spell a listed word reads as a flag in the word check only.
    expect(checkForm("🇦🇶", true)).toBe(" ");
    expect(checkForm("🇦🇶")).toBe("aq");
    expect(checkForm("🇸🇮🇰", true)).toBe("sik");
  });
});

interface TextVector {
  name: string;
  text: string;
  reason: string | null;
}

interface LinkVector {
  name: string;
  text: string;
  reason: string | null;
  rule?: string;
  links?: { url: string; trusted: boolean }[];
  off?: boolean;
  invites?: boolean;
}

describe("the service's shared cases", () => {
  it("are the ones the service tests, with the rule table they were made for", () => {
    expect(shared.text.vectors.length).toBeGreaterThanOrEqual(45);
    expect(shared.links.vectors.length).toBeGreaterThan(100);
    expect(LINK_RULES_VERSION).toBe(table.version);
    expect(shared.links.about).toContain(`rule table version ${LINK_RULES_VERSION}`);
  });

  it("text: tricks turned away with their reason, ordinary text through (links off)", () => {
    const failures: string[] = [];
    for (const vector of shared.text.vectors as TextVector[]) {
      const got = messageProblem(vector.text, null);
      let want: unknown;
      if (vector.reason === "link") want = { reason: "link", rule: "off" };
      else if (vector.reason === "phone") want = { reason: "phone" };
      else if (vector.reason === "words") {
        // The word list is the service's; the launcher knows only 🖕 and says nothing else.
        want = vector.text.includes("\u{1F595}") ? { reason: "words" } : null;
      } else want = null;
      if (JSON.stringify(got) !== JSON.stringify(want)) {
        failures.push(`${vector.name}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
      }
    }
    expect(failures).toEqual([]);
  });

  it("links: every case with Studio's lists as given", () => {
    const failures: string[] = [];
    for (const vector of shared.links.vectors as LinkVector[]) {
      const lists: LinkLists = {
        trusted: shared.links.lists.trusted,
        blocked: shared.links.lists.blocked,
        invites: vector.invites ?? shared.links.lists.invites,
      };
      const on = vector.off === true ? null : lists;
      const problem = messageProblem(vector.text, on);
      const want: Record<string, unknown> = { reason: vector.reason };
      if (vector.rule !== undefined) want.rule = vector.rule;
      const got: Record<string, unknown> = { reason: problem?.reason ?? null };
      if (problem?.reason === "link") got.rule = problem.rule;
      if (vector.reason === null) {
        want.links = vector.links ?? [];
        got.links =
          on === null
            ? []
            : messageLinks(cleanChatText(vector.text), on).map((link) => ({
                url: link.url,
                trusted: link.trusted,
              }));
      }
      if (JSON.stringify(got) !== JSON.stringify(want)) {
        failures.push(`${vector.name}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
      }
    }
    expect(failures).toEqual([]);
  });
});

describe("chat links", () => {
  it("are found as the service finds them", () => {
    const [link] = findLinks("(https://youtu.be/x'i izle)");
    expect(link?.url).toBe("https://youtu.be/x");
    expect(findLinks("xhttps://a.com")).toEqual([]);
    expect(findLinks("HTTPS://Modrinth.com/mod/sodium🔥").map((l) => l.url)).toEqual([
      "HTTPS://Modrinth.com/mod/sodium",
    ]);
  });

  it("may be pressed only while they pass the rules as they are now", () => {
    const text = "https://youtu.be/a https://www.planetminecraft.com/x https://bit.ly/y";
    const links = messageLinks(text, DEFAULT_LINK_LISTS);
    expect(links.map((l) => [l.ok, l.trusted])).toEqual([
      [true, true],
      [true, false],
      [false, false],
    ]);
    expect(links[2]?.rule).toBe("shortener");
    // A site Studio blocks later turns back into plain text in older messages.
    const blocked = messageLinks(text, { ...DEFAULT_LINK_LISTS, blocked: ["youtu.be"] });
    expect(blocked[0]?.ok).toBe(false);
    expect(blocked[0]?.rule).toBe("blocked");
    // Links off: nothing is pressed.
    expect(messageLinks(text, null).every((l) => !l.ok && l.rule === "off")).toBe(true);
  });

  it("read brand names as the eye does", () => {
    expect(skeleton("d1scord")).toBe(skeleton("discord"));
    expect(typoDistance("dslcord", "dlscord")).toBe(1);
    expect(lookalike("discordapp.xyz")).toBe(true);
    expect(lookalike("cdn.discordapp.com")).toBe(false);
  });
});
