// Browser preview of `api.importer`: a TLauncher `.minecraft`, two CurseForge instances and a
// Modrinth profile, measured with a short delay and copied with live-looking progress.
// `?import=` (or localStorage `pl.mock.import`): `none` (no other launcher), `one` (TLauncher
// only), `big` (worlds too big for the disk), `kept` (Projects has game settings of its own),
// `fail` (some items do not come over).
import type {
  ImportApi,
  ImportProfile,
  ImportReport,
  ImportRequest,
  ImportScanView,
  ImportShared,
  ImportSource,
  ItemOutcome,
  SourceMeasure,
} from "./import-types";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const MB = 1024 * 1024;
const GB = 1024 * MB;

function scenario(): string {
  try {
    return (
      new URLSearchParams(window.location.search).get("import") ??
      localStorage.getItem("pl.mock.import") ??
      ""
    );
  } catch {
    return "";
  }
}

const day = 86_400_000;

function profile(
  id: string,
  name: string,
  minecraft: string | null,
  loader: ImportProfile["loader"],
  extra: Partial<ImportProfile> = {},
): ImportProfile {
  return {
    id,
    name,
    minecraft,
    loader,
    loaderVersion: null,
    unsupported: null,
    withoutOptifine: false,
    ownFolder: false,
    modFiles: 0,
    lastUsedMs: null,
    importedAs: null,
    ...extra,
  };
}

function sources(kind: string): ImportSource[] {
  if (kind === "none") return [];
  const now = Date.now();
  const tlauncher: ImportSource = {
    id: "minecraft",
    kind: "tLauncher",
    path: "C:\\Users\\oyuncu\\AppData\\Roaming\\.minecraft",
    shared: {
      options: true,
      servers: 9,
      worlds: 6,
      resourcePacks: 4,
      shaderPacks: 2,
      screenshots: 148,
    },
    profiles: [
      profile("version:Fabric 1.21.4", "Fabric 1.21.4", "1.21.4", "fabric", {
        loaderVersion: "0.16.10",
        modFiles: 24,
        lastUsedMs: now - 2 * day,
      }),
      profile("version:Forge 1.20.1", "Forge 1.20.1", "1.20.1", "forge", {
        loaderVersion: "47.3.0",
        modFiles: 24,
        lastUsedMs: now - 9 * day,
      }),
      profile("version:1.8.9-OptiFine_HD_U_M5", "Minecraft 1.8.9", "1.8.9", "vanilla", {
        withoutOptifine: true,
        lastUsedMs: now - 30 * day,
      }),
      profile("version:1.16.5", "Minecraft 1.16.5", "1.16.5", "vanilla", {
        lastUsedMs: now - 120 * day,
      }),
      profile("version:1.12.2-LiteLoader1.12.2", "LiteLoader 1.12.2", "1.12.2", "vanilla", {
        unsupported: { kind: "loader", name: "LiteLoader" },
      }),
    ],
    offlineName: "EjderAvcisi",
    lastUsedMs: now - 2 * day,
  };
  if (kind === "one" || kind === "big" || kind === "kept") return [tlauncher];
  return [
    tlauncher,
    {
      id: "curseforge",
      kind: "curseForge",
      path: "C:\\Users\\oyuncu\\curseforge\\minecraft\\Instances",
      shared: null,
      profiles: [
        profile("instance:All the Mods 9", "All the Mods 9", "1.20.1", "forge", {
          loaderVersion: "47.2.0",
          ownFolder: true,
          modFiles: 402,
          lastUsedMs: now - 5 * day,
        }),
        profile("instance:RLCraft", "RLCraft", "1.12.2", "forge", {
          loaderVersion: "14.23.5.2860",
          ownFolder: true,
          modFiles: 180,
          lastUsedMs: now - 60 * day,
        }),
      ],
      offlineName: null,
      lastUsedMs: now - 5 * day,
    },
    {
      id: "modrinth",
      kind: "modrinth",
      path: "C:\\Users\\oyuncu\\AppData\\Roaming\\ModrinthApp\\profiles",
      shared: null,
      profiles: [
        profile("instance:Fabulously Optimized", "Fabulously Optimized", "1.21.1", "fabric", {
          loaderVersion: "0.16.5",
          ownFolder: true,
          modFiles: 92,
          lastUsedMs: now - 14 * day,
        }),
      ],
      offlineName: null,
      lastUsedMs: now - 14 * day,
    },
  ];
}

function measureOf(kind: string, source: ImportSource): SourceMeasure {
  const big = kind === "big";
  const sizes: Record<string, { bytes: number; mods: number; leftMods: number; worlds: number }> = {
    "version:Fabric 1.21.4": { bytes: 312 * MB, mods: 21, leftMods: 3, worlds: 0 },
    "version:Forge 1.20.1": { bytes: 486 * MB, mods: 18, leftMods: 6, worlds: 0 },
    "instance:All the Mods 9": { bytes: 1.9 * GB, mods: 402, leftMods: 0, worlds: 2 },
    "instance:RLCraft": { bytes: 780 * MB, mods: 180, leftMods: 0, worlds: 1 },
    "instance:Fabulously Optimized": { bytes: 210 * MB, mods: 92, leftMods: 0, worlds: 0 },
  };
  return {
    sourceId: source.id,
    shared: source.shared
      ? {
          worlds: big ? 38.6 * GB : 2.4 * GB,
          resourcePacks: 186 * MB,
          shaderPacks: 41 * MB,
          screenshots: 352 * MB,
        }
      : null,
    profiles: source.profiles.map((p) => ({
      id: p.id,
      ...(sizes[p.id] ?? { bytes: 0, mods: 0, leftMods: 0, worlds: 0 }),
    })),
  };
}

const WORLD_NAMES = ["Survival", "Ada", "Köy", "Create Dünyası", "Skyblock", "Yaratıcı"];

export function createMockImport(): ImportApi {
  let cancelled = false;
  return {
    async scan(): Promise<ImportScanView> {
      await delay(260);
      const kind = scenario();
      return {
        sources: sources(kind),
        freeBytes: kind === "big" ? 21.4 * GB : 118.2 * GB,
        reserveBytes: 2 * GB,
        projectsHasOptions: kind === "kept",
      };
    },
    async measure(sourceId) {
      const kind = scenario();
      const source = sources(kind).find((s) => s.id === sourceId);
      await delay(sourceId === "minecraft" ? 900 : 1300);
      if (!source) throw new Error("no source");
      return measureOf(kind, source);
    },
    async start(request, _jobId, onProgress): Promise<ImportReport> {
      cancelled = false;
      const kind = scenario();
      const all = sources(kind);
      const items = plan(request, all);
      const totalBytes = items.reduce((sum, item) => sum + item.bytes, 0);
      let doneBytes = 0;
      const send = (stage: "preparing" | "copying" | "finishing", done: number, current?: string) =>
        onProgress({
          stage,
          doneBytes,
          totalBytes,
          doneItems: done,
          totalItems: items.length,
          current: current ?? null,
        });
      send("preparing", 0);
      await delay(500);
      const outcomes: ItemOutcome[] = [];
      for (const [index, item] of items.entries()) {
        if (cancelled) {
          outcomes.push({ ...item.outcome, status: "skipped", reason: "cancelled", copied: 0 });
          continue;
        }
        const steps = Math.max(1, Math.round(item.bytes / (180 * MB)));
        for (let step = 1; step <= steps && !cancelled; step++) {
          await delay(110);
          doneBytes += item.bytes / steps;
          send("copying", index, item.current?.[step % (item.current?.length ?? 1)]);
        }
        outcomes.push(
          cancelled
            ? { ...item.outcome, status: "skipped", reason: "cancelled", copied: 0 }
            : failed(kind, item.outcome),
        );
        send("copying", index + 1);
      }
      send("finishing", items.length);
      await delay(300);
      return {
        outcomes,
        cancelled,
        created: outcomes
          .filter(
            (o) => o.item.type === "profile" && (o.status === "done" || o.status === "partial"),
          )
          .map((o) => ({ id: o.label.toLowerCase().replace(/\W+/g, "-"), name: o.label })),
        officialId: "projects",
      };
    },
    async cancel() {
      cancelled = true;
    },
  };
}

interface PlannedItem {
  bytes: number;
  current?: string[];
  outcome: ItemOutcome;
}

function outcomeFor(sourceId: string, item: ItemOutcome["item"], label = ""): ItemOutcome {
  return {
    sourceId,
    item,
    label,
    status: "done",
    copied: 0,
    left: 0,
    reason: null,
    destinations: [],
    detail: null,
  };
}

const projects = (count: number) => [{ instanceId: "projects", instanceName: "Projects", count }];

function plan(request: ImportRequest, all: ImportSource[]): PlannedItem[] {
  const items: PlannedItem[] = [];
  const order: ImportShared[] = ["options", "servers"];
  const later: ImportShared[] = ["worlds", "resourcePacks", "shaderPacks", "screenshots"];
  const shared = (kinds: ImportShared[]) => {
    for (const req of request.sources) {
      const source = all.find((s) => s.id === req.sourceId);
      if (!source?.shared) continue;
      const sizes = measureOf(scenario(), source).shared;
      for (const kind of kinds) {
        if (!req.shared?.includes(kind)) continue;
        const outcome = outcomeFor(source.id, { type: "shared", kind });
        const counts: Record<ImportShared, number> = {
          options: 1,
          servers: source.shared.servers,
          worlds: source.shared.worlds,
          resourcePacks: source.shared.resourcePacks,
          shaderPacks: source.shared.shaderPacks,
          screenshots: source.shared.screenshots,
        };
        outcome.copied = counts[kind];
        outcome.destinations =
          kind === "worlds"
            ? [
                ...projects(counts[kind] - 2),
                { instanceId: "forge-1-20-1", instanceName: "Forge 1.20.1", count: 2 },
              ]
            : projects(counts[kind]);
        items.push({
          bytes: kind === "options" || kind === "servers" ? 0 : (sizes?.[kind] ?? 0),
          current: kind === "worlds" ? WORLD_NAMES : undefined,
          outcome,
        });
      }
    }
  };
  shared(order);
  for (const req of request.sources) {
    const source = all.find((s) => s.id === req.sourceId);
    if (!source) continue;
    const measured = measureOf(scenario(), source);
    for (const id of req.profiles ?? []) {
      const p = source.profiles.find((x) => x.id === id);
      const m = measured.profiles.find((x) => x.id === id);
      if (!p || !m) continue;
      const outcome = outcomeFor(source.id, { type: "profile", id }, p.name);
      outcome.copied = m.mods;
      outcome.left = m.leftMods;
      if (m.leftMods > 0) {
        outcome.status = "partial";
        outcome.reason = "incompatibleMods";
      }
      outcome.destinations = [{ instanceId: id, instanceName: p.name, count: 1 }];
      items.push({ bytes: m.bytes, current: [p.name], outcome });
    }
  }
  shared(later);
  return items;
}

/** `?import=fail`: the worlds and RLCraft do not all come over. */
function failed(kind: string, outcome: ItemOutcome): ItemOutcome {
  if (kind !== "fail") return outcome;
  if (outcome.item.type === "shared" && outcome.item.kind === "worlds") {
    return {
      ...outcome,
      status: "partial",
      copied: outcome.copied - 2,
      left: 2,
      reason: "noSpace",
      destinations: outcome.destinations.filter((d) => d.instanceId === "projects"),
    };
  }
  if (outcome.label === "RLCraft") {
    return {
      ...outcome,
      status: "failed",
      copied: 0,
      reason: "noSpace",
      detail: "mock: not enough space",
    };
  }
  return outcome;
}
