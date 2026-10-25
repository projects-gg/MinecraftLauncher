import { describe, expect, it } from "vitest";
import type { ModEntry } from "@/lib/api";
import { baseName, ownMods } from "./own-mods";

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
