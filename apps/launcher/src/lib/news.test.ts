import { describe, expect, it } from "vitest";
import { highlights, isWebLink, newsHighlights, parseProjectsLink } from "./news";

describe("news text helpers", () => {
  it("takes plain bullet lines", () => {
    const body =
      "Giriş\n\n- **Yeni** zindan\n* [Kılıç](https://projects.gg/k) dengelendi\n1. `Market` hızlandı\n- \n+ Dördüncü\n- Beşinci";
    expect(highlights(body)).toEqual([
      "Yeni zindan",
      "Kılıç dengelendi",
      "Market hızlandı",
      "Dördüncü",
    ]);
  });

  it("falls back to the summary for patch notes without bullets", () => {
    expect(newsHighlights("Sadece bir paragraf.", "Kısa özet")).toEqual(["Kısa özet"]);
    expect(newsHighlights("- bir\n- iki\n- üç", "özet")).toEqual(["bir", "iki"]);
    expect(newsHighlights("", "")).toEqual([]);
  });

  it("opens only https pages in the browser", () => {
    expect(isWebLink("https://projects.gg/kis")).toBe(true);
    for (const href of [
      undefined,
      "",
      "http://projects.gg",
      "javascript:alert(1)",
      "mailto:a@b.c",
      "https://user:pw@projects.gg",
      " https://projects.gg",
      "projects://news/x",
      "file:///C:/Windows",
      "/relative",
    ]) {
      expect(isWebLink(href)).toBe(false);
    }
  });
});

describe("projects:// links from content", () => {
  // The system opener refuses the launcher's own scheme, so a news CTA, a banner or an event
  // pointing at "projects://news/x" did nothing (CR1-F); the launcher reads them itself.
  it("reads the routes the launcher knows", () => {
    expect(parseProjectsLink("projects://news/sezon-5")).toEqual({ kind: "news", id: "sezon-5" });
    expect(parseProjectsLink("projects://haber/sezon-5/")).toEqual({
      kind: "news",
      id: "sezon-5",
    });
    expect(parseProjectsLink("projects:news/x")).toEqual({ kind: "news", id: "x" });
    expect(parseProjectsLink("projects://")).toEqual({ kind: "home" });
    expect(parseProjectsLink("projects://open")).toEqual({ kind: "home" });
    expect(parseProjectsLink("projects://PLAY")).toEqual({ kind: "play" });
    expect(parseProjectsLink("projects://join")).toEqual({ kind: "play" });
    expect(parseProjectsLink("projects://oyna/projects")).toEqual({
      kind: "playInstance",
      id: "projects",
    });
    expect(parseProjectsLink("projects://afk")).toEqual({ kind: "afk" });
    expect(parseProjectsLink("projects://kurulum/kk-test")).toEqual({
      kind: "instance",
      id: "kk-test",
    });
    expect(parseProjectsLink("projects://friend/prj-7k3m-q9td")).toEqual({
      kind: "friend",
      code: "PRJ-7K3M-Q9TD",
    });
    expect(parseProjectsLink("projects://crew/abcd-efgh-jkmn")).toEqual({
      kind: "crew",
      link: "ABCD-EFGH-JKMN",
    });
    expect(parseProjectsLink("projects://ekip/o0il12345678")).toEqual({
      kind: "crew",
      link: "0011-1234-5678",
    });
  });

  it("ignores anything it does not know", () => {
    for (const raw of [
      "https://projects.gg",
      "projects://pack/projects",
      "projects://news",
      "projects://news/a/b",
      "projects://news/..",
      "projects://news/%2E%2E",
      "projects://afk/x",
      "projects://friend/nope",
      "projects://crew/EKP-7Q2X-9KDA",
      "projects://crew/ABCD-EFGH",
      `projects://news/${"a".repeat(65)}`,
      "projects://a b",
      "not a link",
    ]) {
      expect(parseProjectsLink(raw)).toBeNull();
    }
  });
});
