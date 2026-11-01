import { describe, expect, it } from "vitest";
import type { LegacyImportReport, LegacyScan } from "@/lib/api";
import { broughtThings } from "./summary";

const scan: LegacyScan = {
  found: true,
  hasPremium: false,
  nickname: "EjderAvcisi",
  afkAccounts: 3,
  hasAfkClient: true,
  worlds: 4,
  worldsBytes: 812_000_000,
  screenshots: 37,
  resourcePacks: 2,
  shaderPacks: 1,
  freeBytes: 6_700_000_000,
};

function report(notes: string[] = []): LegacyImportReport {
  return {
    premiumImported: false,
    nickname: null,
    afkAccounts: 0,
    afkClient: false,
    instanceId: "projects",
    copiedFiles: 214,
    linkedFiles: 3912,
    settings: null,
    notes,
  };
}

describe("v1 summary", () => {
  it("names what the game data brought, worlds only when they were picked", () => {
    expect(broughtThings(report(), scan, true)).toEqual(["worlds", "screenshots", "packs"]);
    expect(broughtThings(report(), scan, false)).toEqual(["screenshots", "packs"]);
    expect(
      broughtThings(report(), { ...scan, worlds: 0, screenshots: 0, shaderPacks: 0 }, true),
    ).toEqual(["packs"]);
  });

  it("names nothing it cannot be sure of", () => {
    // A note says something stayed behind (here the worlds): no kind is named, the note says it.
    const left = report(["Dünyalar diskte yeterli yer olmadığı için kopyalanmadı."]);
    expect(broughtThings(left, scan, true)).toBeNull();
    expect(broughtThings(report(), undefined, true)).toBeNull();
    const none = { ...scan, worlds: 0, screenshots: 0, resourcePacks: 0, shaderPacks: 0 };
    expect(broughtThings(report(), none, true)).toBeNull();
  });
});
