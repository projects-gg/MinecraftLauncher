import { describe, expect, it } from "vitest";
import type { ChatLinkRules } from "@/lib/api/social-types";
import { findLinks, hostIn, piecesOf } from "./links";

const RULES: ChatLinkRules = {
  rules: 2,
  trusted: ["projects.gg", "youtube.com", "youtu.be", "twitch.tv", "modrinth.com"],
  blocked: ["youtube.com/@kotu", "kotu.example"],
  invites: true,
};

const urls = (text: string) => findLinks(text, RULES).map((l) => l.url);

describe("findLinks", () => {
  it("finds links as the contract reads them, without what ends a sentence", () => {
    expect(urls("bak https://youtu.be/abc.")).toEqual(["https://youtu.be/abc"]);
    expect(urls("(https://modrinth.com/mod/sodium)")).toEqual(["https://modrinth.com/mod/sodium"]);
    expect(urls("https://youtu.be/x'i izle")).toEqual(["https://youtu.be/x"]);
    expect(urls("https://tr.wikipedia.org/wiki/Elma_(meyve)")).toEqual([
      "https://tr.wikipedia.org/wiki/Elma_(meyve)",
    ]);
    expect(urls("xhttps://youtu.be/a")).toEqual([]);
    expect(urls("HTTPS://YOUTU.BE/a 🔥")).toEqual(["HTTPS://YOUTU.BE/a"]);
  });

  it("lets trusted sites open at once and other good links after the warning", () => {
    const [trusted, other] = findLinks(
      "https://www.youtube.com/watch?v=1 https://example.org/a",
      RULES,
    );
    expect(trusted).toMatchObject({ ok: true, trusted: true, host: "www.youtube.com" });
    expect(other).toMatchObject({ ok: true, trusted: false, host: "example.org" });
  });

  it("keeps links the rules refuse as plain text", () => {
    const refused = [
      "http://example.org",
      "https://youtube.com@kotu.com/x",
      "https://192.168.1.4/x",
      "https://example.org:8080/x",
      "https://xn--youtube-r2a.com/x",
      "https://localhost/x",
      "https://youtube.com/@kotu/videos",
      "https://www.kotu.example/x",
      `https://example.org/${"a".repeat(200)}`,
    ];
    for (const text of refused) expect(findLinks(text, RULES)[0]?.ok, text).toBe(false);
    expect(findLinks("https://youtube.com/@kotular", RULES)[0]?.ok).toBe(true);
    expect(findLinks("https://youtube.com/x", undefined)[0]?.ok).toBe(false);
  });

  it("names the first rule a link fails, in the contract's order", () => {
    const rule = (text: string) => findLinks(text, RULES)[0]?.rule;
    expect(rule("http://youtube.com@kotu.com")).toBe("http");
    expect(rule("https://youtube.com@kotu.com/x")).toBe("credentials");
    expect(rule("https://192.168.1.4/x")).toBe("ip");
    expect(rule("https://[::1]/x")).toBe("ip");
    expect(rule("https://example.org:8080/x")).toBe("port");
    expect(rule("https://xn--youtube-r2a.com/x")).toBe("lookalike");
    expect(rule("https://localhost/x")).toBe("form");
    expect(rule(`https://example.org/${"a".repeat(200)}`)).toBe("long");
    expect(rule("https://youtube.com/@kotu/videos")).toBe("blocked");
    expect(rule("https://youtube.com/x")).toBeNull();
  });

  it("takes Discord invites only while Studio allows them", () => {
    const off = { ...RULES, invites: false };
    for (const text of ["https://discord.gg/abc", "https://discord.com/invite/abc"]) {
      expect(findLinks(text, RULES)[0]).toMatchObject({ ok: true, rule: null });
      expect(findLinks(text, off)[0], text).toMatchObject({ ok: false, rule: "invite" });
    }
    expect(findLinks("https://discord.com/channels/1/2", off)[0]?.ok).toBe(true);
  });

  it("presses at most three links of a message", () => {
    const four = findLinks(
      "https://a.com/1 https://b.com/2 https://c.com/3 https://d.com/4",
      RULES,
    ).map((l) => l.ok);
    expect(four).toEqual([true, true, true, false]);
  });
});

describe("hostIn and piecesOf", () => {
  it("reads a site with its subdomains", () => {
    expect(hostIn("clips.twitch.tv", RULES.trusted)).toBe(true);
    expect(hostIn("nottwitch.tv", RULES.trusted)).toBe(false);
  });

  it("splits a text into its plain runs and links", () => {
    const text = "bak https://youtu.be/a ve gel";
    expect(piecesOf(text, findLinks(text, RULES)).map((p) => [p.kind, p.text])).toEqual([
      ["text", "bak "],
      ["link", "https://youtu.be/a"],
      ["text", " ve gel"],
    ]);
  });
});
