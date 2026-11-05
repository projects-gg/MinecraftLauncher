import { describe, expect, it } from "vitest";
import type { DiscoverCrew, DiscoverStatus } from "@/lib/api/discover-types";
import {
  checksHold,
  daysUntil,
  filterCrews,
  filtering,
  freshness,
  maySend,
  menuFilters,
  NO_FILTER,
  sizeOf,
  textLength,
  textReady,
  waveEnding,
  waveWhen,
} from "./model";

const crew = (id: string, change: Partial<DiscoverCrew> = {}): DiscoverCrew => ({
  id,
  name: `Ekip ${id}`,
  tag: id.toUpperCase().slice(0, 3),
  emblem: "sword",
  color: "gold",
  frame: "cut",
  motto: "",
  text: "Akşamları SkyBlock oynuyoruz.",
  tags: ["skyblock"],
  join: "open",
  members: 5,
  active: true,
  ...change,
});

const words = (tag: string) => ({ skyblock: "SkyBlock", pvp: "PvP", build: "Yapı" })[tag] ?? tag;

describe("filtering the listing", () => {
  const crews = [
    crew("kurt", { name: "Demir Kurtlar", tags: ["skyblock", "build"], members: 12 }),
    crew("tilki", { name: "Kızıl Tilkiler", join: "approval", tags: ["pvp"], members: 22 }),
    crew("ada", { name: "Ada Kurucuları", active: false, text: "Yeni başlayanlara yardım." }),
  ];

  it("keeps the day's order and finds by name, tag, text or a tag's words, Turkish-insensitive", () => {
    expect(filterCrews(crews, NO_FILTER, words).map((c) => c.id)).toEqual(["kurt", "tilki", "ada"]);
    const by = (query: string) =>
      filterCrews(crews, { ...NO_FILTER, query }, words).map((c) => c.id);
    expect(by("kizil")).toEqual(["tilki"]);
    expect(by("YARDIM")).toEqual(["ada"]);
    expect(by("yapı")).toEqual(["kurt"]);
    expect(by("ADA")).toEqual(["ada"]);
  });

  it("takes every chosen tag, the menu's switches and the size", () => {
    const ids = (change: Partial<typeof NO_FILTER>) =>
      filterCrews(crews, { ...NO_FILTER, ...change }, words).map((c) => c.id);
    expect(ids({ tags: ["skyblock", "build"] })).toEqual(["kurt"]);
    expect(ids({ open: true })).toEqual(["kurt", "ada"]);
    expect(ids({ active: true })).toEqual(["kurt", "tilki"]);
    expect(ids({ size: "big" })).toEqual(["tilki"]);
    expect(ids({ size: "small" })).toEqual(["ada"]);
    expect(menuFilters({ ...NO_FILTER, open: true, size: "mid" })).toBe(2);
    expect(filtering(NO_FILTER)).toBe(false);
    expect(filtering({ ...NO_FILTER, query: " " })).toBe(false);
    expect(filtering({ ...NO_FILTER, tags: ["pvp"] })).toBe(true);
  });

  it("groups sizes as the menu says", () => {
    expect([2, 9, 10, 19, 20, 30].map(sizeOf)).toEqual([
      "small",
      "small",
      "mid",
      "mid",
      "big",
      "big",
    ]);
  });
});

describe("the kept listing", () => {
  const now = Date.UTC(2026, 10, 5, 14, 0);
  const kept = (fetchedAgoMin: number, nextInH = 4) => ({
    listing: { ver: 1, at: 0, next: now / 1000 + nextInH * 3600, crews: [] },
    etag: '"d1.1"',
    fetchedAt: now - fetchedAgoMin * 60_000,
  });

  it("fetches without a copy or after the wave, checks it after 15 minutes, keeps it before", () => {
    expect(freshness(null, now)).toBe("fetch");
    expect(freshness(kept(1, -1), now)).toBe("fetch");
    expect(freshness(kept(16), now)).toBe("revalidate");
    expect(freshness(kept(3), now)).toBe("keep");
  });
});

describe("the leader's text and block", () => {
  it("counts and checks the text as the service does", () => {
    expect(textLength("  Akşamları\n  SkyBlock  ")).toBe(18);
    expect(textReady("kısa yazı")).toBe(false);
    expect(textReady("1234567890 12")).toBe(false);
    expect(textReady("Akşamları SkyBlock oynuyoruz.")).toBe(true);
    expect(textReady("ş".repeat(161))).toBe(false);
  });

  const status = (change: Partial<DiscoverStatus> = {}): DiscoverStatus => ({
    state: "listed",
    listed: { text: "Akşamları SkyBlock oynuyoruz.", tags: ["skyblock"] },
    draft: { text: "Akşamları SkyBlock oynuyoruz.", tags: ["skyblock"] },
    changeWaits: false,
    waveAt: 0,
    retryAt: null,
    checks: {
      members: { need: 3, have: 5 },
      crewDays: { need: 3, have: 9 },
      leaderDays: { need: 14, have: 100 },
      visible: true,
    },
    editsLeft: 3,
    editsReset: 0,
    ...change,
  });

  it("sends only something new, while changes are left", () => {
    expect(maySend(status(), "Akşamları SkyBlock oynuyoruz.", ["skyblock"])).toBe(false);
    expect(maySend(status(), "Akşamları  SkyBlock oynuyoruz. ", ["skyblock"])).toBe(false);
    expect(maySend(status(), "Akşamları SkyBlock oynuyoruz.", ["skyblock", "pvp"])).toBe(true);
    expect(maySend(status({ editsLeft: 0 }), "Yepyeni bir yazı burada.", [])).toBe(false);
    expect(
      maySend(status({ state: "off", listed: null, draft: null }), "Yepyeni bir yazı.", []),
    ).toBe(true);
  });

  it("holds the requirements only when all hold", () => {
    expect(checksHold(status().checks)).toBe(true);
    expect(checksHold({ ...status().checks, visible: false })).toBe(false);
    expect(checksHold({ ...status().checks, members: { need: 3, have: 2 } })).toBe(false);
  });

  it("says the wave's day and the time's Turkish ending", () => {
    const now = new Date(2026, 10, 5, 12, 0).getTime();
    const today = new Date(2026, 10, 5, 18, 0).getTime() / 1000;
    const tomorrow = new Date(2026, 10, 6, 18, 0).getTime() / 1000;
    expect(waveWhen(today, now)).toEqual({ day: "today", hour: 18, minute: 0 });
    expect(waveWhen(tomorrow, now).day).toBe("tomorrow");
    expect(waveEnding(18, 0)).toBe("de");
    expect(waveEnding(20, 0)).toBe("de");
    expect(waveEnding(19, 30)).toBe("da");
    expect(daysUntil(now / 1000 + 6.2 * 86_400, now)).toBe(7);
    expect(daysUntil(now / 1000 + 60, now)).toBe(1);
  });
});
