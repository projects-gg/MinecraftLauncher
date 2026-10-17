import { describe, expect, it } from "vitest";
import type { DroppedFile, DroppedKind, DropResult } from "@/lib/api";
import { dropNotices, planDrop, stem, tabFor } from "./model";

const f = (name: string, kind: DroppedKind): DroppedFile => ({ path: `C:/x/${name}`, name, kind });

const empty: DropResult = {
  added: [],
  existing: [],
  unknown: [],
  needsLoader: [],
  busy: [],
  modpacks: [],
  failed: [],
};

describe("planDrop", () => {
  it("groups files by where they go, in a fixed order", () => {
    const plan = planDrop(
      [
        f("readme.txt", "unknown"),
        f("BSL.zip", "shaderPack"),
        f("sodium.jar", "mod"),
        f("lithium.jar", "mod"),
        f("Faithful.zip", "resourcePack"),
      ],
      { id: "projects", modded: true },
    );
    expect(plan.rows).toEqual([
      { kind: "mod", count: 2 },
      { kind: "resourcePack", count: 1 },
      { kind: "shaderPack", count: 1 },
      { kind: "unknown", count: 1 },
    ]);
    expect(plan.acceptable).toBe(true);
    expect(plan.importOnly).toBe(false);
  });

  it("marks mods on a vanilla instance and copies without an instance", () => {
    const vanilla = planDrop([f("sodium.jar", "mod")], { id: "v", modded: false });
    expect(vanilla.rows).toEqual([{ kind: "mod", count: 1, blocked: "needsLoader" }]);
    expect(vanilla.acceptable).toBe(false);
    // Resource packs still go into a vanilla instance.
    const packs = planDrop([f("sodium.jar", "mod"), f("F.zip", "resourcePack")], {
      id: "v",
      modded: false,
    });
    expect(packs.acceptable).toBe(true);
    const none = planDrop([f("F.zip", "resourcePack")], null);
    expect(none.rows[0]?.blocked).toBe("noInstance");
    expect(none.acceptable).toBe(false);
  });

  it("imports modpacks without a target and refuses unknown files", () => {
    const pack = planDrop([f("FO.mrpack", "modpack")], null);
    expect(pack.importOnly).toBe(true);
    expect(pack.acceptable).toBe(true);
    expect(pack.modpack?.name).toBe("FO.mrpack");
    const unknown = planDrop([f("a.txt", "unknown"), f("b.png", "unknown")], null);
    expect(unknown.rows).toEqual([{ kind: "unknown", count: 2 }]);
    expect(unknown.acceptable).toBe(false);
    expect(planDrop([], null).importOnly).toBe(false);
  });
});

describe("dropNotices", () => {
  it("names a single file and counts several", () => {
    expect(dropNotices({ ...empty, added: [f("Sodium-0.6.jar", "mod")] })).toEqual([
      { tone: "success", key: "addedOne", values: { name: "Sodium-0.6" }, show: "mods" },
    ]);
    expect(
      dropNotices({ ...empty, added: [f("a.zip", "resourcePack"), f("b.zip", "resourcePack")] }),
    ).toEqual([
      { tone: "success", key: "added.resourcePack", values: { count: 2 }, show: "resourcePacks" },
    ]);
    expect(
      dropNotices({
        ...empty,
        added: [f("a.jar", "mod"), f("b.zip", "shaderPack"), f("c.zip", "shaderPack")],
      }),
    ).toEqual([{ tone: "success", key: "addedMany", values: { count: 3 }, show: "shaders" }]);
  });

  it("reports what was left out, success first", () => {
    const notices = dropNotices({
      ...empty,
      added: [f("a.jar", "mod")],
      existing: [f("Faithful.zip", "resourcePack")],
      needsLoader: [f("b.jar", "mod")],
      busy: [f("c.jar", "mod")],
      unknown: [f("x.txt", "unknown"), f("y.txt", "unknown")],
      failed: [f("d.zip", "shaderPack")],
    });
    expect(notices.map((n) => n.key)).toEqual([
      "addedOne",
      "existingOne",
      "needsLoader",
      "busy",
      "unknownMany",
      "failedOne",
    ]);
    expect(notices.find((n) => n.key === "unknownMany")?.values).toEqual({ count: 2 });
    expect(notices.at(-1)?.tone).toBe("error");
    expect(dropNotices(empty)).toEqual([]);
  });

  it("names an unknown file in full", () => {
    expect(dropNotices({ ...empty, unknown: [f("notes.txt", "unknown")] })).toEqual([
      { tone: "info", key: "unknownOne", values: { name: "notes.txt" } },
    ]);
  });
});

describe("helpers", () => {
  it("strips one extension", () => {
    expect(stem("Sodium-0.6.jar")).toBe("Sodium-0.6");
    expect(stem("Faithful")).toBe("Faithful");
    expect(stem(".hidden")).toBe(".hidden");
  });

  it("opens the tab of the most common kind", () => {
    expect(tabFor([f("a.zip", "shaderPack")])).toBe("shaders");
    expect(
      tabFor([f("a.jar", "mod"), f("b.zip", "resourcePack"), f("c.zip", "resourcePack")]),
    ).toBe("resourcePacks");
    expect(tabFor([f("a.jar", "mod"), f("b.zip", "resourcePack")])).toBe("mods");
  });
});
