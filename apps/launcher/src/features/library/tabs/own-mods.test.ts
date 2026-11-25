import { describe, expect, it } from "vitest";
import type { InstalledContent, ModEntry } from "@/lib/api";
import { baseName, bulkUpdates, ownMods } from "./own-mods";

const mod = (fileName: string, extra: Partial<ModEntry> = {}): ModEntry => ({
  fileName,
  name: fileName,
  version: "1.0",
  authors: [],
  description: "",
  enabled: !fileName.endsWith(".disabled"),
  fromPack: false,
  sizeBytes: 1,
  ...extra,
});

const names = (mods: ModEntry[]) => mods.map((m) => m.fileName);

describe("Tümünü Güncelle", () => {
  const entry = (fileName: string, extra: Partial<InstalledContent> = {}): InstalledContent => ({
    fileName,
    enabled: true,
    sizeBytes: 1,
    fromPack: false,
    update: { versionId: "v2", versionNumber: "2.0", versionType: "release", datePublished: "" },
    ...extra,
  });

  it("leaves out held mods (REPAIR-RB), the pack's files and files no longer listed", () => {
    const entries = [
      entry("sodium.jar"),
      entry("lithium.jar", { heldFrom: "0.16.0" }),
      entry("fabric-api.jar", { fromPack: true }),
      entry("gone.jar"),
      entry("zoomify.jar.disabled"),
      entry("iris.jar", { update: undefined }),
    ];
    const present = new Set([
      "sodium.jar",
      "lithium.jar",
      "fabric-api.jar",
      "zoomify.jar",
      "iris.jar",
    ]);
    expect(bulkUpdates(entries, present).map((e) => e.fileName)).toEqual([
      "sodium.jar",
      "zoomify.jar.disabled",
    ]);
  });
});

describe("the player's own mods (Kendi modların)", () => {
  const folder = [
    mod("sodium.jar", { fromPack: true, itemId: "AANobbMI" }),
    mod("zoomify.jar"),
    mod("appleskin.jar", { modSet: { id: "pvp", name: "PvP", purpose: "pvp" } }),
    mod("continuity.jar.disabled"),
    mod("lithium.jar"),
  ];

  it("leaves the official pack's mods to Önerilenler and a set's to Setler", () => {
    expect(names(ownMods(folder, true))).toEqual([
      "zoomify.jar",
      "continuity.jar.disabled",
      "lithium.jar",
    ]);
  });

  it("knows the pack's files the scan saw, also switched off", () => {
    const scanned = new Set(["lithium.jar", "continuity.jar"]);
    expect(names(ownMods(folder, true, (name) => scanned.has(name)))).toEqual(["zoomify.jar"]);
  });

  it("lists a pack's file where no recommended section holds it", () => {
    // Outside the official pack's instances a pack file (a copy, an older launcher) stays listed.
    expect(names(ownMods(folder, false))).toEqual([
      "sodium.jar",
      "zoomify.jar",
      "continuity.jar.disabled",
      "lithium.jar",
    ]);
  });

  it("drops only the switched-off mark from a name", () => {
    expect(baseName("a.jar.disabled")).toBe("a.jar");
    expect(baseName("a.disabled.jar")).toBe("a.disabled.jar");
  });
});
