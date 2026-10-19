import { describe, expect, it } from "vitest";
import type { ModCenterItem } from "@/lib/api";
import { awaitsPackMods, incomingMods, incomingNames } from "./incoming-mods";

function item(id: string, over: Partial<ModCenterItem> = {}): ModCenterItem {
  return {
    id,
    title: id,
    category: "performance",
    fileName: `${id}.jar`,
    folder: "mods",
    library: false,
    defaultEnabled: true,
    requires: [],
    selected: true,
    enabled: true,
    isNew: false,
    requiredBy: [],
    installed: false,
    updateAvailable: false,
    sizeBytes: 1000,
    modified: false,
    disabledCopy: false,
    ...over,
  };
}

const fresh = { managed: true, loader: "fabric" as const, state: "needsInstall" as const };

describe("the pack's mods on their way", () => {
  it("are the ones on and never installed, into the mods folder", () => {
    const items = [
      item("Sodium"),
      item("Zoomify", { selected: false, enabled: false }),
      item("Lithium", { installed: true }),
      item("Fresh Animations", { folder: "resourcepacks" }),
      item("Entity Culling"),
    ];
    expect(incomingMods(items).map((i) => i.id)).toEqual(["Sodium", "Entity Culling"]);
  });

  it("put the chosen mods before the libraries they need", () => {
    const items = [
      item("Fabric API", { library: true, selected: false, requiredBy: ["Sodium"] }),
      item("Sodium", { requires: ["Fabric API"] }),
      item("Cloth Config", { library: true, selected: false, enabled: false }),
    ];
    expect(incomingMods(items).map((i) => i.id)).toEqual(["Sodium", "Fabric API"]);
  });

  it("are asked for only while a pack instance has no game for its version yet", () => {
    expect(awaitsPackMods(fresh)).toBe(true);
    expect(awaitsPackMods({ ...fresh, state: "needsUpdate" })).toBe(true);
    // Installed: nothing on the way (a wiped mods folder is the mod center's to explain).
    expect(awaitsPackMods({ ...fresh, state: "ready" })).toBe(false);
    expect(awaitsPackMods({ ...fresh, managed: false })).toBe(false);
    expect(awaitsPackMods({ ...fresh, loader: "vanilla" })).toBe(false);
  });

  it("are named in one short line", () => {
    expect(incomingNames([item("Sodium"), item("Entity Culling")])).toEqual({
      names: ["Sodium", "Entity Culling"],
      more: 0,
    });
    expect(incomingNames([item("Sodium"), item("Entity Culling"), item("Fabric API")])).toEqual({
      names: ["Sodium"],
      more: 2,
    });
    expect(incomingNames([item("Sodium")])).toEqual({ names: ["Sodium"], more: 0 });
  });
});
