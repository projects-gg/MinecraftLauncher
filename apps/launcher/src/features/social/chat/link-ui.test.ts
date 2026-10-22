import { describe, expect, it } from "vitest";
import type { ChatCard } from "@/lib/api/social-types";
import { cardShapeFor, domainOf, piecesOf, ruleWords, shapeOf } from "./link-ui";
import { messageLinks } from "./text";

const card = (extra: Partial<ChatCard>): ChatCard => ({
  state: "ready",
  url: "https://example.org/a",
  site: "example.org",
  icon: "globe",
  kind: "page",
  ...extra,
});

describe("domainOf", () => {
  it("names the site the warning speaks of", () => {
    expect(domainOf("www.youtube.com")).toBe("youtube.com");
    expect(domainOf("clips.twitch.tv")).toBe("twitch.tv");
    expect(domainOf("forum.projects.gg")).toBe("projects.gg");
    expect(domainOf("example.com.tr")).toBe("example.com.tr");
    expect(domainOf("forum.example.com.tr")).toBe("example.com.tr");
    expect(domainOf("shop.example.co.uk")).toBe("example.co.uk");
    expect(domainOf("kotu.blogspot.com")).toBe("blogspot.com");
    expect(domainOf("youtube.com.kotu.example")).toBe("kotu.example");
  });
});

describe("card shapes", () => {
  it("holds a wide place for a video, a clip and a projects.gg page", () => {
    expect(cardShapeFor("https://youtu.be/abc", "youtu.be")).toBe("banner");
    expect(cardShapeFor("https://www.youtube.com/watch?v=abc", "www.youtube.com")).toBe("banner");
    expect(cardShapeFor("https://www.youtube.com/@kanal", "www.youtube.com")).toBe("row");
    expect(cardShapeFor("https://clips.twitch.tv/abc", "clips.twitch.tv")).toBe("banner");
    expect(cardShapeFor("https://www.twitch.tv/kanal", "www.twitch.tv")).toBe("row");
    expect(cardShapeFor("https://projects.gg/haber/1", "projects.gg")).toBe("banner");
    expect(cardShapeFor("https://wiki.projects.gg/x", "wiki.projects.gg")).toBe("row");
    expect(cardShapeFor("https://modrinth.com/mod/sodium", "modrinth.com")).toBe("row");
  });

  it("draws a card by its picture's own proportions", () => {
    expect(shapeOf(card({ kind: "video", image: "data:x", w: 320, h: 180 }))).toBe("banner");
    expect(shapeOf(card({ kind: "page", image: "data:x", w: 1200, h: 630 }))).toBe("banner");
    expect(shapeOf(card({ kind: "page", image: "data:x", w: 900, h: 900 }))).toBe("row");
    expect(shapeOf(card({ kind: "project", image: "data:x", w: 96, h: 96 }))).toBe("row");
    expect(shapeOf(card({ kind: "stream", image: "data:x" }))).toBe("banner");
    expect(shapeOf(card({ kind: "video" }))).toBe("row");
    expect(shapeOf(card({ state: "plain", kind: "video" }))).toBe("row");
  });
});

describe("drawing a message's links", () => {
  const lists = { trusted: ["youtu.be"], blocked: [], invites: true };

  it("splits a text into its plain runs and links", () => {
    const text = "bak https://youtu.be/a ve gel";
    expect(piecesOf(text, messageLinks(text, lists)).map((p) => [p.kind, p.text])).toEqual([
      ["text", "bak "],
      ["link", "https://youtu.be/a"],
      ["text", " ve gel"],
    ]);
  });

  it("words each refusal by its own rule, or the common sentence", () => {
    expect(ruleWords("http")).toBe("http");
    expect(ruleWords("formHint")).toBe("formHint");
    expect(ruleWords("grabber")).toBe("other");
    expect(ruleWords("blocked")).toBe("other");
  });
});
