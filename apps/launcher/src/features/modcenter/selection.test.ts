import { describe, expect, it } from "vitest";
import type { ModCenterItem } from "@/lib/api";
import {
  bringsAlong,
  choicesOf,
  enabledSet,
  matches,
  neededBy,
  neededFor,
  pendingSelection,
  safePageUrl,
  totals,
} from "./selection";

function item(id: string, extra: Partial<ModCenterItem> = {}): ModCenterItem {
  return {
    id,
    title: id.toUpperCase(),
    category: "performance",
    fileName: `${id}.jar`,
    folder: "mods",
    library: false,
    defaultEnabled: true,
    requires: [],
    selected: false,
    enabled: false,
    isNew: false,
    requiredBy: [],
    installed: false,
    updateAvailable: false,
    sizeBytes: 1,
    modified: false,
    disabledCopy: false,
    ...extra,
  };
}

// shaders → iris → sodium → api (library); minimap stands alone.
const catalog = [
  item("api", { library: true, category: "library" }),
  item("sodium", { selected: true, requires: ["api"] }),
  item("iris", { requires: ["sodium"] }),
  item("shaders", { requires: ["iris"], summary: "Işık ve gölge" }),
  item("minimap", { selected: true }),
];

describe("mod center selection", () => {
  it("closes over requirements and ignores libraries as choices", () => {
    const choices = choicesOf(catalog);
    expect(choices).toEqual({ sodium: true, iris: false, shaders: false, minimap: true });
    expect([...enabledSet(catalog, choices)].sort()).toEqual(["api", "minimap", "sodium"]);
    const withShaders = enabledSet(catalog, { ...choices, shaders: true, sodium: false });
    expect([...withShaders].sort()).toEqual(["api", "iris", "minimap", "shaders", "sodium"]);
    expect(neededBy(catalog, withShaders, "sodium")).toEqual(["IRIS"]);
    expect(bringsAlong(catalog, "shaders")).toEqual(["IRIS", "SODIUM", "API"]);
  });

  it("keeps what the launcher's own overlay needs and says so", () => {
    const api = item("api", { library: true, category: "library", launcherNeeds: true });
    const withOverlay = [api, ...catalog.filter((i) => i.id !== "api")];
    const lean = enabledSet(withOverlay, { ...choicesOf(withOverlay), sodium: false });
    expect([...lean].sort()).toEqual(["api", "minimap"]);
    expect(neededFor(withOverlay, lean, api, "Oyun içi katman")).toEqual(["Oyun içi katman"]);
    const full = enabledSet(withOverlay, choicesOf(withOverlay));
    expect(neededFor(withOverlay, full, api, "Oyun içi katman")).toEqual([
      "SODIUM",
      "Oyun içi katman",
    ]);
    expect(neededFor(catalog, full, item("minimap"), "Oyun içi katman")).toEqual([]);
  });

  it("sends only what changed", () => {
    expect(pendingSelection(catalog, choicesOf(catalog), new Set())).toBeNull();
    expect(pendingSelection(catalog, { ...choicesOf(catalog), iris: true }, new Set())).toEqual({
      items: { iris: true },
      revert: [],
    });
    expect(pendingSelection(catalog, choicesOf(catalog), new Set(["minimap"]))).toEqual({
      items: {},
      revert: ["minimap"],
    });
  });

  it("searches Turkish text and sums summaries", () => {
    expect(matches(catalog[3] as ModCenterItem, "IŞIK")).toBe(true);
    expect(matches(catalog[4] as ModCenterItem, "ışık")).toBe(false);
    const counts = (mods: number, configs = 0) => ({
      mods,
      resourcePacks: 0,
      shaderPacks: 1,
      configs,
      other: 0,
    });
    const summary = (added: ReturnType<typeof counts>) => ({
      added,
      updated: counts(0),
      removed: counts(1),
      kept: 0,
      unchanged: 0,
      downloadBytes: 0,
      removeBytes: 0,
    });
    expect(totals(summary(counts(2)))).toEqual({
      added: 3,
      updated: 1,
      removed: 2,
      otherFiles: false,
    });
    expect(safePageUrl("https://www.wimods.net/wi-zoom/")).toBe("https://www.wimods.net/wi-zoom/");
    for (const unsafe of [
      undefined,
      "",
      "http://www.wimods.net/",
      "javascript:alert(1)",
      "file:///C:/Windows",
      "wimods.net",
    ]) {
      expect(safePageUrl(unsafe)).toBeUndefined();
    }
    // A first install also writes the pack's settings files: they are not counted as mods.
    expect(totals(summary(counts(2, 2)))).toEqual({
      added: 3,
      updated: 1,
      removed: 2,
      otherFiles: true,
    });
  });
});
