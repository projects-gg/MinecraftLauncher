import { describe, expect, it } from "vitest";
import type { ResumeTarget, SavedServer } from "@/lib/api";
import {
  featuredServer,
  instanceBytes,
  leadCard,
  neverPlayed,
  recentShots,
  recentWorlds,
  SHOT_TILES,
  WORLD_ROWS,
} from "./model";

const hoursAgo = (hours: number) => new Date(Date.UTC(2026, 9, 17, 12) - hours * 3_600_000);
const world = (folder: string, hours?: number) => ({
  folder,
  lastPlayedAt: hours === undefined ? undefined : hoursAgo(hours).toISOString(),
});
const resumeWorld = (folder: string): ResumeTarget => ({
  kind: "world",
  name: folder,
  quickPlay: { kind: "singleplayer", world: folder },
});
const resumeServer = (host: string, port: number | null = null): ResumeTarget => ({
  kind: "server",
  name: host,
  quickPlay: { kind: "multiplayer", host, port },
});
const server = (name: string, address: string): SavedServer => ({ name, address });

describe("neverPlayed", () => {
  it("is true only without a last session and without play time", () => {
    expect(neverPlayed({ playTimeSecs: 0 })).toBe(true);
    expect(neverPlayed({ playTimeSecs: 0, lastPlayedAt: hoursAgo(1).toISOString() })).toBe(false);
    expect(neverPlayed({ playTimeSecs: 30 })).toBe(false);
  });
});

describe("recentWorlds", () => {
  const worlds = [world("Eski", 900), world("Yeni", 2), world("Bozuk"), world("Orta", 50)];

  it("lists the most recently played first, unreadable dates last", () => {
    const { shown, more, others } = recentWorlds(worlds, null);
    expect(shown.map((w) => w.folder)).toEqual(["Yeni", "Orta", "Eski"]);
    expect(shown).toHaveLength(WORLD_ROWS + 1);
    expect(more).toBe(true);
    expect(others).toBe(false);
  });

  it("leaves out the world the card opens and keeps one row less under it", () => {
    const { shown, more, others } = recentWorlds(worlds, resumeWorld("Yeni"));
    expect(shown.map((w) => w.folder)).toEqual(["Orta", "Eski"]);
    expect(more).toBe(true);
    expect(others).toBe(true);
  });

  it("says nothing is left out when every world is on screen", () => {
    const two = [world("A", 1), world("B", 3)];
    expect(recentWorlds(two, null).more).toBe(false);
    expect(recentWorlds([...two, world("C", 5)], resumeWorld("A")).more).toBe(false);
  });

  it("keeps a server card from taking a world's place in the title", () => {
    const { shown, others } = recentWorlds(worlds, resumeServer("mc.hypixel.net"));
    expect(shown).toHaveLength(WORLD_ROWS);
    expect(others).toBe(false);
  });

  it("does not reorder the list it was given", () => {
    const list = [world("B", 9), world("A", 1)];
    recentWorlds(list, null);
    expect(list.map((w) => w.folder)).toEqual(["B", "A"]);
  });
});

describe("featuredServer", () => {
  const list = [
    server("Hypixel", "mc.hypixel.net"),
    server("Projects", "Play.Projects.GG"),
    server("Kale", "kale.ornek.net:25570"),
  ];

  it("features the community server wherever the player keeps it", () => {
    expect(featuredServer(list, null)).toEqual({ server: list[1], more: true });
  });

  it("falls back to the top of the player's list", () => {
    expect(featuredServer([list[2] as SavedServer], null)).toEqual({
      server: list[2],
      more: false,
    });
  });

  it("skips the server the card already joins, default port or not", () => {
    expect(featuredServer(list, resumeServer("play.projects.gg")).server).toBe(list[0]);
    expect(featuredServer(list, resumeServer("play.projects.gg", 25565)).server).toBe(list[0]);
    expect(featuredServer(list, resumeServer("kale.ornek.net", 25570)).server).toBe(list[1]);
    // Another port is another server.
    expect(featuredServer(list, resumeServer("play.projects.gg", 25566)).server).toBe(list[1]);
  });

  it("has nothing to feature when the card joins the only server", () => {
    const only = [server("Projects", "play.projects.gg")];
    expect(featuredServer(only, resumeServer("play.projects.gg"))).toEqual({
      server: null,
      more: false,
    });
    expect(featuredServer([], null)).toEqual({ server: null, more: false });
  });

  it("does not let a world card hide a server", () => {
    expect(featuredServer(list, resumeWorld("Yeni")).server).toBe(list[1]);
  });
});

describe("recentShots", () => {
  it("shows the newest few and says when there are more", () => {
    const shots = ["a", "b", "c", "d"];
    expect(recentShots(shots)).toEqual({ shown: shots.slice(0, SHOT_TILES), more: true });
    expect(recentShots(["a"])).toEqual({ shown: ["a"], more: false });
    expect(recentShots([])).toEqual({ shown: [], more: false });
  });
});

describe("leadCard", () => {
  const base = { played: true, resume: false, worlds: 0, shots: 0, server: false };

  it("offers Devam et whenever there is somewhere to go back to", () => {
    expect(leadCard({ ...base, resume: true, worlds: 3 })).toBe("resume");
    expect(leadCard({ ...base, played: false, resume: true })).toBe("resume");
  });

  it("shows one empty state for an instance never played", () => {
    expect(leadCard({ ...base, played: false })).toBe("neverPlayed");
    // The server the launcher listed at creation still shows under it.
    expect(leadCard({ ...base, played: false, server: true })).toBe("neverPlayed");
  });

  it("lets brought-along worlds and screenshots speak for themselves", () => {
    expect(leadCard({ ...base, played: false, worlds: 2 })).toBeNull();
    expect(leadCard({ ...base, played: false, shots: 1 })).toBeNull();
    expect(leadCard({ ...base, worlds: 1 })).toBeNull();
  });

  it("uses the empty card for a played instance only when nothing at all is there", () => {
    expect(leadCard(base)).toBe("nothing");
    expect(leadCard({ ...base, server: true })).toBeNull();
  });
});

describe("instanceBytes", () => {
  const sized = (...sizes: number[]) => sizes.map((sizeBytes) => ({ sizeBytes }));

  it("adds up the instance's own files", () => {
    expect(
      instanceBytes({
        mods: sized(10, 20),
        resourcePacks: sized(5),
        shaderPacks: sized(3),
        worlds: sized(100, 200),
        screenshots: sized(1, 1),
        backups: sized(50),
      }),
    ).toBe(390);
  });

  it("counts what is known when some lists are missing", () => {
    expect(instanceBytes({ mods: [], resourcePacks: [], shaderPacks: [], worlds: sized(7) })).toBe(
      7,
    );
  });

  it("ignores negative sizes", () => {
    expect(
      instanceBytes({ mods: sized(-5, 5), resourcePacks: [], shaderPacks: [], worlds: [] }),
    ).toBe(5);
  });
});
