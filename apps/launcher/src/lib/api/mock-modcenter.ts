// Mock `api.modCenter` for the browser preview: a recommended-mods catalog with every state
// (new, update, required, adopted manual changes, replaced file), versions with kept sets and a
// migration plan.
import type { JobProgress } from "./content-types";
import { mockFreshOfficial } from "./mock-fresh";
import type {
  InstanceVersions,
  MigrationCheck,
  MigrationPlan,
  ModCenterApi,
  ModCenterItem,
  ModCenterPreview,
  ModSelection,
  SwitchPreview,
} from "./modcenter-types";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
/** Preview-only switch: `localStorage["pl.mock.modrinthOffline"] = "1"` shows the offline paths. */
const modrinthOffline = () => {
  try {
    return localStorage.getItem("pl.mock.modrinthOffline") === "1";
  } catch {
    return false;
  }
};
const daysAgo = (d: number) => new Date(Date.now() - d * 86_400_000).toISOString();
const MB = 1024 * 1024;

type Seed = Omit<
  ModCenterItem,
  "enabled" | "requiredBy" | "installed" | "updateAvailable" | "modified" | "disabledCopy"
> &
  Partial<Pick<ModCenterItem, "updateAvailable" | "modified" | "disabledCopy">>;

const icon = (id: string) => `https://cdn.modrinth.com/data/${id}/icon.png`;

const seeds: Seed[] = [
  {
    id: "AANobbMI",
    title: "Sodium",
    summary: "Daha yüksek FPS, daha akıcı oyun.",
    category: "performance",
    iconUrl: icon("AANobbMI"),
    projectId: "AANobbMI",
    fileName: "sodium-fabric-0.6.13+mc26.1.2.jar",
    folder: "mods",
    library: false,
    defaultEnabled: true,
    requires: [],
    selected: true,
    isNew: false,
    sizeBytes: 1.5 * MB,
  },
  {
    id: "gvQqBUqZ",
    title: "Lithium",
    summary: "Oyun mantığını hızlandırır, kalabalık yerlerde takılmayı azaltır.",
    category: "performance",
    iconUrl: icon("gvQqBUqZ"),
    projectId: "gvQqBUqZ",
    fileName: "lithium-fabric-0.15.4+mc26.1.2.jar",
    folder: "mods",
    library: false,
    defaultEnabled: true,
    requires: [],
    selected: true,
    isNew: false,
    updateAvailable: true,
    sizeBytes: 0.78 * MB,
  },
  {
    id: "NNAgCjsB",
    title: "Entity Culling",
    summary: "Görünmeyen varlıkları çizmez, FPS artar.",
    category: "performance",
    iconUrl: icon("NNAgCjsB"),
    projectId: "NNAgCjsB",
    fileName: "entityculling-fabric-1.11.3-mc26.1.2.jar",
    folder: "mods",
    library: false,
    defaultEnabled: true,
    requires: ["P7dR8mSH"],
    selected: true,
    isNew: false,
    sizeBytes: 1.4 * MB,
  },
  {
    id: "uXXizFIs",
    title: "FerriteCore",
    summary: "Bellek kullanımını azaltır.",
    category: "performance",
    iconUrl: icon("uXXizFIs"),
    projectId: "uXXizFIs",
    fileName: "ferritecore-7.1.3-fabric.jar",
    folder: "mods",
    library: false,
    defaultEnabled: true,
    requires: [],
    selected: false,
    isNew: false,
    manual: "disabled",
    disabledCopy: true,
    sizeBytes: 0.12 * MB,
  },
  {
    id: "YL57xq9U",
    title: "Iris",
    summary: "Shader paketlerini çalıştırır.",
    category: "visual",
    iconUrl: icon("YL57xq9U"),
    projectId: "YL57xq9U",
    fileName: "iris-fabric-1.8.11+mc26.1.2.jar",
    folder: "mods",
    library: false,
    defaultEnabled: false,
    requires: ["AANobbMI"],
    selected: false,
    isNew: false,
    sizeBytes: 2.6 * MB,
  },
  {
    id: "HVnmMxH1",
    title: "Complementary Reimagined",
    summary: "Gerçekçi ışık ve gölgeler; güçlü bir ekran kartı ister.",
    category: "visual",
    iconUrl: icon("HVnmMxH1"),
    projectId: "HVnmMxH1",
    fileName: "ComplementaryReimagined_r5.5.zip",
    folder: "shaderpacks",
    library: false,
    defaultEnabled: false,
    requires: ["YL57xq9U"],
    selected: false,
    isNew: false,
    sizeBytes: 7 * MB,
  },
  {
    id: "1bokaNcj",
    title: "Xaero's Minimap",
    summary: "Ekranın köşesinde mini harita ve yol işaretleri.",
    category: "interface",
    iconUrl: icon("1bokaNcj"),
    projectId: "1bokaNcj",
    fileName: "Xaeros_Minimap_25.2.10_Fabric_26.1.2.jar",
    folder: "mods",
    library: false,
    defaultEnabled: false,
    requires: [],
    selected: false,
    isNew: true,
    sizeBytes: 1.9 * MB,
  },
  {
    id: "EsAfCjCV",
    title: "AppleSkin",
    summary: "Açlık ve doygunluk değerlerini gösterir.",
    category: "interface",
    iconUrl: icon("EsAfCjCV"),
    projectId: "EsAfCjCV",
    fileName: "appleskin-fabric-mc26.1.2-3.0.6.jar",
    folder: "mods",
    library: false,
    defaultEnabled: true,
    requires: ["P7dR8mSH"],
    selected: false,
    isNew: false,
    manual: "removed",
    sizeBytes: 0.2 * MB,
  },
  {
    // Hosted by us (not on Modrinth): its page is set by hand in Studio.
    id: "wi-zoom",
    title: "WI Zoom",
    summary: "Yakınlaştırma tuşu; fare tekerleğiyle yakınlığı ayarlarsın.",
    category: "gameplay",
    iconUrl:
      "https://raw.githubusercontent.com/Wurst-Imperium/WI-Zoom/v1.7-MC26.1.2/src/main/resources/assets/wi_zoom/icon.png",
    pageUrl: "https://www.wimods.net/wi-zoom/",
    fileName: "WI-Zoom-1.7-MC26.1.2.jar",
    folder: "mods",
    library: false,
    defaultEnabled: true,
    requires: ["P7dR8mSH"],
    selected: true,
    isNew: false,
    sizeBytes: 0.15 * MB,
  },
  {
    id: "w7ThoJFB",
    title: "Zoomify",
    summary: "Yumuşak geçişli yakınlaştırma.",
    category: "gameplay",
    iconUrl: icon("w7ThoJFB"),
    projectId: "w7ThoJFB",
    fileName: "zoomify-2.14.2+26.1.2.jar",
    folder: "mods",
    library: false,
    defaultEnabled: true,
    requires: ["P7dR8mSH"],
    selected: true,
    isNew: false,
    modified: true,
    sizeBytes: 0.9 * MB,
  },
  {
    id: "P7dR8mSH",
    title: "Fabric API",
    summary: "Diğer modların kullandığı ortak kitaplık.",
    category: "library",
    iconUrl: icon("P7dR8mSH"),
    projectId: "P7dR8mSH",
    fileName: "fabric-api-0.119.2+26.1.2.jar",
    folder: "mods",
    library: true,
    defaultEnabled: false,
    requires: [],
    selected: false,
    isNew: false,
    sizeBytes: 2.3 * MB,
  },
];

/** Enabled = selected plus everything they need (transitively). */
function resolve(choices: Record<string, boolean>): ModCenterItem[] {
  const byId = new Map(seeds.map((s) => [s.id, s]));
  const enabled = new Set<string>();
  const visit = (id: string) => {
    if (enabled.has(id)) return;
    enabled.add(id);
    for (const r of byId.get(id)?.requires ?? []) visit(r);
  };
  for (const s of seeds) if (!s.library && choices[s.id]) visit(s.id);
  return seeds.map((s) => ({
    ...s,
    // Studio writes the Modrinth project page unless the item has its own.
    pageUrl: s.pageUrl ?? (s.projectId ? `https://modrinth.com/project/${s.projectId}` : undefined),
    selected: !s.library && Boolean(choices[s.id]),
    enabled: enabled.has(s.id),
    requiredBy: seeds
      .filter((o) => enabled.has(o.id) && o.requires.includes(s.id))
      .map((o) => o.id),
    installed: installed.has(s.id),
    updateAvailable: Boolean(s.updateAvailable) && installed.has(s.id),
    modified: Boolean(s.modified),
    disabledCopy: Boolean(s.disabledCopy) && !enabled.has(s.id),
  }));
}

const choices: Record<string, boolean> = Object.fromEntries(
  seeds.filter((s) => !s.library).map((s) => [s.id, s.selected]),
);
// `?fresh=1` (mock-fresh.ts): nothing installed until the first start.
const installed = new Set(
  mockFreshOfficial() ? [] : seeds.filter((s) => s.selected || s.library).map((s) => s.id),
);
let seen = false;

function preview(selection: ModSelection): ModCenterPreview {
  const next = { ...choices, ...selection.items };
  const before = resolve(choices);
  const after = resolve(next);
  let add = 0;
  let remove = 0;
  let downloadBytes = 0;
  let removeBytes = 0;
  for (const item of after) {
    const was = before.find((b) => b.id === item.id)?.enabled ?? false;
    if (item.enabled && !was) {
      add += 1;
      downloadBytes += item.sizeBytes;
    } else if (!item.enabled && was) {
      remove += 1;
      removeBytes += item.sizeBytes;
    }
  }
  const reverted = (selection.revert ?? []).length;
  const counts = (mods: number) => ({
    mods,
    resourcePacks: 0,
    shaderPacks: 0,
    configs: 0,
    other: 0,
  });
  return {
    items: after,
    summary: {
      added: counts(add),
      updated: counts(reverted),
      removed: counts(remove),
      kept: 0,
      unchanged: after.filter((i) => i.enabled).length,
      downloadBytes:
        downloadBytes +
        seeds.filter((s) => selection.revert?.includes(s.id)).reduce((a, s) => a + s.sizeBytes, 0),
      removeBytes,
    },
    changesFiles: add + remove + reverted > 0,
    recordsChoices: !seen,
  };
}

async function progress(onProgress: (p: JobProgress) => void, steps = 8) {
  for (let i = 1; i <= steps; i++) {
    await delay(160);
    onProgress({
      stage: "downloading",
      percent: (i / steps) * 100,
      doneFiles: i,
      totalFiles: steps,
      bytesPerSec: 4.2 * MB,
    });
  }
}

const versionOptions = (includeSnapshots = false): InstanceVersions => ({
  current: "26.1.2",
  managed: true,
  loader: "fabric",
  partial: false,
  gameRunning: false,
  options: [
    ...(includeSnapshots
      ? [
          {
            minecraft: "26w41a",
            current: false,
            recommended: false,
            available: true,
            hasSet: false,
            prepared: false,
            snapshot: true,
            sizeBytes: 0,
          },
        ]
      : []),
    {
      minecraft: "26.3",
      current: false,
      recommended: false,
      available: true,
      hasSet: false,
      prepared: false,
      snapshot: false,
      sizeBytes: 0,
    },
    {
      minecraft: "26.1.2",
      current: true,
      recommended: true,
      available: false,
      hasSet: true,
      prepared: false,
      snapshot: false,
      lastPlayedAt: daysAgo(1),
      sizeBytes: 186 * MB,
    },
    {
      minecraft: "1.21.11",
      current: false,
      recommended: false,
      available: true,
      hasSet: true,
      prepared: false,
      snapshot: false,
      lastPlayedAt: daysAgo(12),
      sizeBytes: 164 * MB,
    },
    {
      minecraft: "1.21.4",
      current: false,
      recommended: false,
      available: true,
      hasSet: false,
      prepared: true,
      snapshot: false,
      sizeBytes: 9 * MB,
    },
    {
      minecraft: "1.21.1",
      current: false,
      recommended: false,
      available: true,
      hasSet: true,
      prepared: false,
      snapshot: false,
      lastPlayedAt: daysAgo(74),
      sizeBytes: 142 * MB,
    },
  ],
});

export function createMockModCenter(): ModCenterApi {
  return {
    async view(instanceId) {
      await delay(260);
      const managed = instanceId === "projects";
      const fresh = managed && mockFreshOfficial();
      return {
        instanceId,
        mcVersion: managed ? "26.1.2" : "1.21.1",
        loader: "fabric",
        managed,
        pack: managed
          ? {
              id: "projects",
              name: "Projects",
              version: "2.4.0",
              installedVersion: fresh ? undefined : "2.3.0",
              minecraft: "26.1.2",
            }
          : undefined,
        items: managed ? resolve(choices).map((i) => (seen ? { ...i, isNew: false } : i)) : [],
        reset: false,
        issues:
          managed && !fresh
            ? [
                {
                  fileName: "bozuk-mod.jar",
                  folder: "mods",
                  kind: "unreadable",
                  detail: "mods/bozuk-mod.jar: Erişim engellendi. (os error 5)",
                },
              ]
            : [],
        unavailable: false,
        gameRunning: false,
      };
    },
    async preview(_instanceId, selection) {
      await delay(180);
      return preview(selection);
    },
    async apply(_instanceId, selection, _jobId, onProgress) {
      const plan = preview(selection);
      await progress(onProgress, Math.max(2, plan.summary.added.mods * 3));
      Object.assign(choices, selection.items);
      for (const item of resolve(choices)) {
        if (item.enabled) installed.add(item.id);
        else installed.delete(item.id);
      }
      seen = true;
      return {
        downloaded: plan.summary.added.mods + plan.summary.updated.mods,
        restored: 0,
        removed: plan.summary.removed.mods,
        issues: 0,
      };
    },
    async markSeen() {
      await delay(120);
      seen = true;
    },
    async versions(instanceId, includeSnapshots) {
      await delay(240);
      const list = versionOptions(includeSnapshots);
      if (instanceId === "projects") return list;
      // Other instances follow Mojang's list: here on 1.21.1, the recommended 26.1.2 one switch
      // away.
      return {
        ...list,
        current: "1.21.1",
        managed: false,
        options: list.options.map((o) => ({
          ...o,
          current: o.minecraft === "1.21.1",
          available: o.minecraft !== "1.21.1",
        })),
      };
    },
    async switchPreview(_instanceId, minecraft): Promise<SwitchPreview> {
      await delay(320);
      const option = versionOptions().options.find((o) => o.minecraft === minecraft);
      const returning = Boolean(option?.hasSet);
      return {
        from: "26.1.2",
        to: minecraft,
        firstVisit: !returning,
        parkedMods: 14,
        parkedBytes: 186 * MB,
        hasConfig: true,
        restoredMods: returning ? 12 : option?.prepared ? 3 : 0,
        restoredLastPlayedAt: returning ? option?.lastPlayedAt : undefined,
        downloadFiles: returning ? 2 : 9,
        downloadBytes: returning ? 3.1 * MB : 21.4 * MB,
        carriedChoices: returning ? 0 : 6,
        newItems: returning ? 0 : 2,
        ownMods: 4,
        gameDownload: !returning,
        loader: "fabric",
        loaderVersion: "0.17.3",
        // Preview of "Eski seti geri getir": cleanup moved this version's set to the trash.
        trashedSet:
          !returning && minecraft === "26.3"
            ? {
                trashId: "mock-trashed-set",
                mods: 18,
                sizeBytes: 212 * MB,
                lastPlayedAt: new Date(Date.now() - 74 * 24 * 3600 * 1000).toISOString(),
                downloadFiles: 3,
                downloadBytes: 4.2 * MB,
              }
            : undefined,
      };
    },
    async restoreTrashedSet(_instanceId, _trashId) {
      await delay(260);
    },
    async switchVersion(_instanceId, minecraft, _jobId, onProgress) {
      onProgress({ stage: "switching", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
      await delay(500);
      await progress(onProgress, 9);
      const firstVisit = !versionOptions().options.find((o) => o.minecraft === minecraft)?.hasSet;
      return {
        from: "26.1.2",
        to: minecraft,
        firstVisit,
        carried: firstVisit ? ["Sodium", "Lithium", "Entity Culling", "Zoomify"] : [],
        newItems: firstVisit ? ["ImmediatelyFast", "Xaero's Minimap"] : [],
        ownModsLeft: 4,
        synced: true,
      };
    },
    async migrationCheck(_instanceId, from, to): Promise<MigrationCheck> {
      await delay(700);
      const offline = modrinthOffline();
      return {
        from,
        to,
        own: 4,
        compatible: offline
          ? []
          : [
              { title: "Zoomify", versionId: "mock-zoomify" },
              { title: "Continuity", versionId: "mock-continuity" },
            ],
        alreadyThere: 0,
        offline,
      };
    },
    async migrationPlan(_instanceId, from, to): Promise<MigrationPlan> {
      await delay(600);
      return {
        from: from ?? "26.1.2",
        to,
        offline: false,
        mods: [
          {
            fileName: "zoomify-2.14.2+26.1.2.jar",
            title: "Zoomify",
            projectId: "w7ThoJFB",
            currentVersion: "2.14.2",
            targetVersionId: "mock-zoomify",
            targetVersionNumber: "2.13.5",
            sizeBytes: 0.9 * MB,
            status: "compatible",
          },
          {
            fileName: "continuity-3.0.0.jar.disabled",
            title: "Continuity",
            projectId: "1IjD5062",
            currentVersion: "3.0.0",
            targetVersionId: "mock-continuity",
            targetVersionNumber: "3.0.0-beta.5",
            sizeBytes: 0.4 * MB,
            status: "compatible",
          },
          {
            fileName: "eski-harita-eklentisi.jar",
            title: "Eski harita eklentisi",
            currentVersion: "1.0",
            sizeBytes: 0.2 * MB,
            status: "unknown",
          },
          {
            fileName: "betterf3-11.0.jar",
            title: "BetterF3",
            projectId: "8shC1gFX",
            currentVersion: "11.0",
            sizeBytes: 0.5 * MB,
            status: "noVersion",
          },
        ],
      };
    },
    async migrate(_instanceId, _to, versionIds, _jobId, onProgress) {
      await progress(onProgress, Math.max(2, versionIds.length * 2));
      return { installed: versionIds.length, alreadyInstalled: 0, unresolved: 0 };
    },
  };
}
