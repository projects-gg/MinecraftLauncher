import { describe, expect, it } from "vitest";
import type {
  ImportProfile,
  ImportReport,
  ImportScanView,
  ImportSource,
  ItemOutcome,
  SourceMeasure,
} from "@/lib/api";
import { enImport } from "@/locales/en-import";
import { trImport } from "@/locales/tr-import";
import {
  defaultTicks,
  FIRST_PROFILES,
  failureDetails,
  fits,
  LARGE_BYTES,
  LAUNCHER_NAMES,
  measureKeys,
  measuring,
  profileKey,
  profileMeta,
  requestOf,
  rowsKey,
  rowsOf,
  selectedBytes,
  sharedKey,
  suggestedName,
  summaryLines,
  summaryTitle,
  ticksOf,
} from "./model";

const MB = 1024 * 1024;
const GB = 1024 * MB;

function profile(id: string, extra: Partial<ImportProfile> = {}): ImportProfile {
  return {
    id,
    name: id,
    minecraft: "1.21.4",
    loader: "fabric",
    loaderVersion: "0.16.9",
    unsupported: null,
    withoutOptifine: false,
    ownFolder: false,
    modFiles: 0,
    lastUsedMs: null,
    importedAs: null,
    ...extra,
  };
}

function source(id: string, extra: Partial<ImportSource> = {}): ImportSource {
  return {
    id,
    kind: "tLauncher",
    path: "C:\\x",
    shared: null,
    profiles: [],
    offlineName: null,
    lastUsedMs: null,
    ...extra,
  };
}

const minecraft = source("minecraft", {
  shared: {
    options: true,
    servers: 4,
    worlds: 3,
    resourcePacks: 2,
    shaderPacks: 0,
    screenshots: 10,
  },
  profiles: [
    profile("version:Fabric 1.21.4", { name: "Fabric 1.21.4", modFiles: 20 }),
    profile("version:1.8.9", {
      name: "Minecraft 1.8.9",
      minecraft: "1.8.9",
      loader: "vanilla",
      withoutOptifine: true,
    }),
    profile("version:lite", {
      name: "LiteLoader 1.12.2",
      unsupported: { kind: "loader", name: "LiteLoader" },
    }),
    profile("version:done", { name: "Forge 1.20.1", loader: "forge", importedAs: "forge-1-20-1" }),
  ],
  offlineName: "Ejder_42",
});

const curseforge = source("curseforge", {
  kind: "curseForge",
  profiles: Array.from({ length: FIRST_PROFILES + 2 }, (_, i) =>
    profile(`instance:${i}`, { name: `Paket ${i}`, minecraft: "1.20.1", loader: "forge" }),
  ),
});

function scan(extra: Partial<ImportScanView> = {}): ImportScanView {
  return {
    sources: [minecraft, curseforge],
    freeBytes: 100 * GB,
    reserveBytes: 2 * GB,
    projectsHasOptions: false,
    ...extra,
  };
}

const measures = (worlds: number): Record<string, SourceMeasure> => ({
  minecraft: {
    sourceId: "minecraft",
    shared: { worlds, resourcePacks: 50 * MB, shaderPacks: 0, screenshots: 30 * MB },
    profiles: [
      { id: "version:Fabric 1.21.4", bytes: 300 * MB, mods: 17, leftMods: 3, worlds: 0 },
      { id: "version:1.8.9", bytes: 0, mods: 0, leftMods: 0, worlds: 0 },
    ],
  },
  curseforge: {
    sourceId: "curseforge",
    shared: null,
    profiles: curseforge.profiles.map((p) => ({
      id: p.id,
      bytes: 1 * GB,
      mods: 100,
      leftMods: 0,
      worlds: 1,
    })),
  },
});

describe("import rows", () => {
  it("list what each launcher has, and say which profiles cannot be ticked", () => {
    const rows = rowsOf(scan(), {});
    const [mc] = rows;
    expect(mc?.shared.map((r) => r.kind)).toEqual([
      "options",
      "servers",
      "worlds",
      "resourcePacks",
      "screenshots",
    ]);
    // Sizes are unknown until measured, except the settings and the server list.
    expect(mc?.shared.map((r) => r.bytes)).toEqual([0, 0, undefined, undefined, undefined]);
    expect(mc?.profiles.map((r) => r.state)).toEqual(["ready", "ready", "unsupported", "imported"]);
    const kept = rowsOf(scan({ projectsHasOptions: true }), {})[0]?.shared[0];
    expect(kept?.state).toBe("kept");
  });

  it("start with everything that fits ticked, the big and the old ones left for the player", () => {
    const rows = rowsOf(scan(), measures(1 * GB));
    const ticks = defaultTicks(rows, 100 * GB, 2 * GB);
    expect(ticks.has(sharedKey("minecraft", "worlds"))).toBe(true);
    expect(ticks.has(profileKey("minecraft", "version:lite"))).toBe(false);
    expect(ticks.has(profileKey("minecraft", "version:done"))).toBe(false);
    const cf = curseforge.profiles.map((p) => ticks.has(profileKey("curseforge", p.id)));
    expect(cf.filter(Boolean)).toHaveLength(FIRST_PROFILES);
    expect(cf.slice(FIRST_PROFILES)).toEqual([false, false]);

    // Worlds over the limit start unticked.
    const huge = rowsOf(scan(), measures(LARGE_BYTES + 1));
    expect(defaultTicks(huge, 100 * GB, 2 * GB).has(sharedKey("minecraft", "worlds"))).toBe(false);
  });

  it("drop the biggest ticks until they fit the disk, never the settings", () => {
    const rows = rowsOf(scan(), measures(3 * GB));
    const ticks = defaultTicks(rows, 6 * GB, 2 * GB);
    const total = selectedBytes(rows, ticks);
    expect(total).toBeLessThanOrEqual(4 * GB);
    expect(fits(total, 6 * GB, 2 * GB)).toBe(true);
    expect(ticks.has(sharedKey("minecraft", "options"))).toBe(true);
    expect(ticks.has(sharedKey("minecraft", "servers"))).toBe(true);
    expect(ticks.has(sharedKey("minecraft", "worlds"))).toBe(false);
  });

  it("keep the player's own choices and send only what is ticked", () => {
    const rows = rowsOf(scan(), measures(1 * GB));
    const defaults = defaultTicks(rows, 100 * GB, 2 * GB);
    const choices = new Map([
      [sharedKey("minecraft", "screenshots"), false],
      [profileKey("curseforge", "instance:6"), true],
      // A row that cannot come over stays unticked whatever is chosen.
      [profileKey("minecraft", "version:lite"), true],
    ]);
    const ticks = ticksOf(rows, defaults, choices);
    expect(ticks.has(sharedKey("minecraft", "screenshots"))).toBe(false);
    expect(ticks.has(profileKey("curseforge", "instance:6"))).toBe(true);
    expect(ticks.has(profileKey("minecraft", "version:lite"))).toBe(false);
    const request = requestOf(rows, ticks);
    expect(request.sources.map((s) => s.sourceId)).toEqual(["minecraft", "curseforge"]);
    expect(request.sources[0]?.shared).toEqual(["options", "servers", "worlds", "resourcePacks"]);
    expect(request.sources[0]?.profiles).toEqual(["version:Fabric 1.21.4", "version:1.8.9"]);
    expect(requestOf(rows, new Set()).sources).toEqual([]);
  });

  it("know when sizes are still coming and when nothing is known about the disk", () => {
    const rows = rowsOf(scan(), {});
    const ticks = defaultTicks(rows, undefined, 0);
    expect(measuring(rows, ticks)).toBe(true);
    expect(fits(10 * GB, undefined, 2 * GB)).toBe(true);
    expect(fits(0, 1 * GB, 2 * GB)).toBe(true);
    expect(fits(5 * GB, 6 * GB, 2 * GB)).toBe(false);
  });

  it("describe a profile in its second line", () => {
    const rows = rowsOf(scan(), measures(1 * GB));
    const [fabric, vanilla, lite, done] = rows[0]?.profiles ?? [];
    // The name says the version; the measured mods replace the file count.
    expect(fabric && profileMeta(fabric)).toEqual([
      { key: "profile.mods", params: { count: 17 } },
      { key: "profile.leftMods", params: { count: 3 } },
    ]);
    expect(vanilla && profileMeta(vanilla)).toEqual([{ key: "profile.noOptifine" }]);
    expect(lite && profileMeta(lite)).toEqual([
      { key: "profile.loader", params: { name: "LiteLoader" } },
    ]);
    expect(done && profileMeta(done)).toEqual([{ key: "profile.imported" }]);
    // None of its mods fits: said in one piece.
    expect(
      fabric &&
        profileMeta({
          ...fabric,
          measure: { id: fabric.profile.id, bytes: 0, mods: 0, leftMods: 4, worlds: 0 },
        }),
    ).toEqual([{ key: "profile.noModsFit" }]);
    const pack = rows[1]?.profiles[0];
    expect(pack && profileMeta(pack)).toEqual([
      { key: "profile.version", params: { loader: "forge", version: "1.20.1" } },
      { key: "profile.mods", params: { count: 100 } },
      { key: "profile.worlds", params: { count: 1 } },
    ]);
  });

  it("offer the offline name a launcher remembered", () => {
    expect(suggestedName(scan())).toBe("Ejder_42");
    expect(suggestedName(scan({ sources: [curseforge] }))).toBeUndefined();
    expect(suggestedName(undefined)).toBeUndefined();
  });
});

// E2E3: a profile that turned up within a minute of the last measurement was never measured, so
// its row kept its skeleton and "Aktar" stayed off.
describe("import measurements", () => {
  const later = profile("version:Fabric 1.21.5", { name: "Fabric 1.21.5", minecraft: "1.21.5" });
  const grown = { ...minecraft, profiles: [...minecraft.profiles, later] };

  it("are made again when a profile turns up, once its rows hold still", () => {
    expect(rowsKey(grown)).not.toBe(rowsKey(minecraft));
    // A world that turns up changes what the worlds weigh.
    const shared = minecraft.shared;
    expect(shared && rowsKey({ ...minecraft, shared: { ...shared, worlds: 4 } })).not.toBe(
      rowsKey(minecraft),
    );
    // The same profiles in another order are the same rows.
    expect(rowsKey({ ...minecraft, profiles: [...minecraft.profiles].reverse() })).toBe(
      rowsKey(minecraft),
    );
    const first = { minecraft: rowsKey(minecraft), curseforge: rowsKey(curseforge) };
    // A source seen for the first time is measured at once.
    expect(measureKeys(first, {})).toEqual(first);
    // A changed one keeps its last rows until the change settles, then gets its new ones.
    const changed = { ...first, minecraft: rowsKey(grown) };
    expect(measureKeys(changed, first)).toEqual(first);
    expect(measureKeys(changed, changed)).toEqual(changed);
  });

  it("let the new row wait for its size, never for ever", () => {
    const grownScan = scan({ sources: [grown, curseforge] });
    const sizes = measures(1 * GB);
    const newRow = (upToDate?: Set<string>) =>
      rowsOf(grownScan, sizes, upToDate)[0]?.profiles.find((r) => r.profile.id === later.id);
    // Under the last measurement it waits: "Aktar" holds while it is ticked.
    expect(newRow()?.bytes).toBeUndefined();
    const rows = rowsOf(grownScan, sizes);
    const ticks = defaultTicks(rows, 100 * GB, 2 * GB);
    expect(ticks.has(profileKey("minecraft", later.id))).toBe(true);
    expect(measuring(rows, ticks)).toBe(true);
    // A measurement of the rows it has now that does not know it: it is gone, it weighs nothing.
    const upToDate = new Set(["minecraft", "curseforge"]);
    expect(newRow(upToDate)?.bytes).toBe(0);
    const settled = rowsOf(grownScan, sizes, upToDate);
    expect(measuring(settled, defaultTicks(settled, 100 * GB, 2 * GB))).toBe(false);
  });
});

function outcome(extra: Partial<ItemOutcome>): ItemOutcome {
  return {
    sourceId: "minecraft",
    sourceKind: "tLauncher",
    item: { type: "shared", kind: "options" },
    label: "",
    status: "done",
    copied: 1,
    left: 0,
    reason: null,
    destinations: [],
    detail: null,
    ...extra,
  };
}

function report(outcomes: ItemOutcome[], cancelled = false): ImportReport {
  return { outcomes, cancelled, created: [], officialId: "projects" };
}

describe("import summary", () => {
  it("says what came over, where worlds went and what stayed behind", () => {
    const lines = summaryLines(
      report([
        outcome({}),
        outcome({ item: { type: "shared", kind: "servers" }, copied: 3 }),
        outcome({
          item: { type: "shared", kind: "worlds" },
          status: "partial",
          copied: 4,
          left: 1,
          reason: "noSpace",
          destinations: [
            { instanceId: "projects", instanceName: "Projects", count: 3 },
            { instanceId: "forge", instanceName: "Forge 1.20.1", count: 1 },
          ],
        }),
        outcome({
          item: { type: "profile", id: "p" },
          label: "Fabric 1.21.4",
          status: "partial",
          copied: 17,
          left: 3,
          reason: "incompatibleMods",
        }),
        outcome({
          item: { type: "profile", id: "lite" },
          label: "LiteLoader 1.12.2",
          status: "skipped",
          copied: 0,
          reason: "unsupported",
        }),
        outcome({
          item: { type: "shared", kind: "screenshots" },
          status: "skipped",
          copied: 0,
          reason: "alreadyThere",
        }),
      ]),
    );
    expect(lines.map((l) => [l.tone, l.key])).toEqual([
      ["done", "outcome.options"],
      ["done", "outcome.servers"],
      ["partial", "outcome.worlds"],
      ["done", "outcome.profileMods"],
      ["failed", "outcome.profileUnsupported"],
      ["skipped", "outcome.already"],
    ]);
    expect(lines[2]?.more.map((m) => m.key)).toEqual([
      "outcome.worldsIn",
      "outcome.worldsIn",
      "outcome.leftBecause.noSpace",
    ]);
    expect(lines[3]?.params).toEqual({ name: "Fabric 1.21.4", count: 17 });
    expect(lines[3]?.more).toEqual([{ key: "outcome.profileLeftMods", params: { count: 3 } }]);
    // Shared lines say which launcher their things came from; a profile is named by itself.
    expect(lines.map((l) => l.source)).toEqual([
      "tLauncher",
      "tLauncher",
      "tLauncher",
      undefined,
      undefined,
      "tLauncher",
    ]);
  });

  it("say where worlds went and why some stayed in the other launcher", () => {
    const worlds = (extra: Partial<ItemOutcome>) =>
      summaryLines(report([outcome({ item: { type: "shared", kind: "worlds" }, ...extra })]))[0];
    // One instance: named without a count.
    const one = worlds({
      sourceId: "tlauncher-folder",
      copied: 1,
      destinations: [{ instanceId: "forge", instanceName: "Forge 1.20.1", count: 1 }],
    });
    expect(one?.more).toEqual([
      { key: "outcome.worldsInOne", params: { name: "Forge 1.20.1", count: 1 } },
    ]);
    // Worlds whose version or loader no instance here plays stay there.
    const some = worlds({
      status: "partial",
      copied: 2,
      left: 1,
      reason: "needsInstance",
      destinations: [
        { instanceId: "fabric", instanceName: "Benim modlarım", count: 1 },
        { instanceId: "vanilla", instanceName: "Minecraft 1.21.1", count: 1 },
      ],
    });
    expect([some?.tone, some?.more.map((m) => m.key)]).toEqual([
      "partial",
      ["outcome.worldsIn", "outcome.worldsIn", "outcome.leftBecause.needsInstance"],
    ]);
    const none = worlds({ status: "failed", copied: 0, left: 2, reason: "needsInstance" });
    expect([none?.tone, none?.key]).toEqual(["failed", "outcome.needsInstance"]);
  });

  it("keep two launchers' alike lines apart", () => {
    const lines = summaryLines(
      report([
        outcome({ item: { type: "shared", kind: "servers" }, copied: 1 }),
        outcome({
          sourceId: "tlauncher-folder",
          item: { type: "shared", kind: "servers" },
          copied: 1,
        }),
        outcome({ sourceId: "prism", sourceKind: "prism", item: { type: "profile", id: "x" } }),
      ]),
    );
    expect(new Set(lines.map((l) => l.id)).size).toBe(3);
  });

  it("folds what was cancelled into one line and names the run", () => {
    const cancelled = report(
      [
        outcome({}),
        outcome({
          item: { type: "shared", kind: "worlds" },
          status: "skipped",
          copied: 0,
          reason: "cancelled",
        }),
        outcome({
          item: { type: "profile", id: "x" },
          label: "X",
          status: "skipped",
          copied: 0,
          reason: "cancelled",
        }),
      ],
      true,
    );
    expect(summaryLines(cancelled).map((l) => l.key)).toEqual([
      "outcome.options",
      "outcome.cancelled",
    ]);
    expect(summaryTitle(cancelled)).toBe("titleCancelled");
    expect(summaryTitle(report([outcome({})]))).toBe("title");
    // Mods that fit no version here stay behind by design.
    expect(
      summaryTitle(
        report([
          outcome({
            item: { type: "profile", id: "p" },
            status: "partial",
            left: 3,
            reason: "incompatibleMods",
          }),
        ]),
      ),
    ).toBe("title");
    expect(
      summaryTitle(report([outcome({}), outcome({ status: "failed", reason: "noTarget" })])),
    ).toBe("titleSome");
    expect(summaryTitle(report([outcome({ status: "failed", reason: "noTarget" })]))).toBe(
      "titleFailed",
    );
  });

  it("collects the technical details of what failed", () => {
    const details = failureDetails(
      report([
        outcome({}),
        outcome({
          item: { type: "profile", id: "instance:RLCraft" },
          label: "RLCraft",
          status: "failed",
          reason: "noSpace",
          detail: "not enough free space",
        }),
      ]),
    );
    expect(details).toContain("RLCraft (instance:RLCraft): failed noSpace");
    expect(details).toContain("not enough free space");
    expect(failureDetails(report([outcome({})]))).toBe("");
  });
});

/** Every string key of a locale tree (`a.b.c`), plural forms folded into their key. */
function keys(tree: unknown, prefix = ""): string[] {
  if (typeof tree === "string") return [prefix.replace(/_(one|other)$/, "")];
  if (!tree || typeof tree !== "object") return [];
  return Object.entries(tree).flatMap(([name, child]) =>
    keys(child, prefix ? `${prefix}.${name}` : name),
  );
}

describe("import words", () => {
  it("exist in both languages", () => {
    const tr = new Set(keys(trImport));
    const en = new Set(keys(enImport));
    expect([...tr].filter((k) => !en.has(k))).toEqual([]);
    expect([...en].filter((k) => !tr.has(k))).toEqual([]);
  });

  it("cover every line the model asks for", () => {
    const tr = new Set(keys(trImport));
    const used = [
      ...summaryLines(
        report(
          [
            outcome({}),
            outcome({ status: "skipped", reason: "keptExisting" }),
            outcome({ status: "failed", reason: "unreadable" }),
            outcome({ status: "failed", reason: "noTarget" }),
            outcome({ status: "failed", reason: "inUse" }),
            outcome({ status: "failed", reason: "failed" }),
            outcome({
              item: { type: "shared", kind: "worlds" },
              status: "failed",
              reason: "needsInstance",
            }),
            outcome({
              item: { type: "shared", kind: "worlds" },
              status: "partial",
              left: 1,
              reason: "needsInstance",
              destinations: [{ instanceId: "a", instanceName: "A", count: 1 }],
            }),
            outcome({
              item: { type: "profile", id: "a" },
              label: "A",
              status: "partial",
              copied: 0,
              left: 1,
              reason: "failed",
            }),
            outcome({ item: { type: "profile", id: "b" }, label: "B", status: "failed" }),
            outcome({
              item: { type: "profile", id: "c" },
              label: "C",
              status: "failed",
              reason: "noSpace",
            }),
            outcome({
              item: { type: "profile", id: "d" },
              label: "D",
              status: "skipped",
              reason: "alreadyThere",
            }),
          ],
          true,
        ),
      ).flatMap((l) => [l.key, ...l.more.map((m) => m.key)]),
      ...rowsOf(scan(), measures(1 * GB))
        .flatMap((s) => s.profiles)
        .flatMap((row) => profileMeta(row).map((p) => p.key)),
    ];
    expect(used.filter((key) => !tr.has(key))).toEqual([]);
    // Every launcher can be named as where things came from, in both languages.
    for (const kind of Object.keys(LAUNCHER_NAMES)) {
      expect(tr.has(`from.${kind}`) && tr.has(`at.${kind}`)).toBe(true);
    }
  });

  it("name each launcher with the suffixes its name takes in Turkish", () => {
    for (const [kind, name] of Object.entries(LAUNCHER_NAMES)) {
      const from = trImport.from[kind as keyof typeof trImport.from];
      const at = trImport.at[kind as keyof typeof trImport.at];
      expect(from.startsWith(`${name}'`)).toBe(true);
      expect(from).toMatch(/'(dan|den|tan|ten)$/);
      // "TLauncher'dan" → "TLauncher'daki", "Modrinth App'ten" → "Modrinth App'teki".
      expect(at).toBe(from.replace(/n$/, "ki"));
      expect(enImport.from[kind as keyof typeof enImport.from]).toContain(name);
    }
  });

  it("speak to the player informally and never show technical words", () => {
    const strings = keys(trImport).map((k) =>
      k.split(".").reduce<unknown>((o, part) => (o as Record<string, unknown>)[part], trImport),
    );
    for (const text of strings) {
      expect(typeof text).toBe("string");
      expect(String(text).toLocaleLowerCase("tr")).not.toMatch(/lütfen|deneyin|\.minecraft|json/);
    }
  });
});
