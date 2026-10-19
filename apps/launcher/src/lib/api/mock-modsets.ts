// Preview (no Tauri) implementation of `api.modSets` (MODSET1). Switches (query or
// localStorage `pl.mock.modsets`): `offline` (the sets cannot be loaded), `demo` (an instance
// "PvP 26.3" made from the PvP set: its files carry the set's badge, the team changed the set
// and one item is new), `empty` (the catalog has no sets).
import type { JobProgress } from "./content-types";
import type { MockSetCard, MockSetItem } from "./mock-modsets-data";
import type {
  CategoryCounts,
  ModCenterItem,
  ModCenterPreview,
  ModSelection,
  SyncSummary,
} from "./modcenter-types";
import type { InstanceSet, ModSetsApi, SetBadge, SetCard, SetItemKind } from "./modsets-types";
import type { InstanceSummary, ModEntry, NewInstance, PackEntry } from "./types";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function modSetsFlag(): string {
  try {
    return (
      new URLSearchParams(window.location.search).get("modsets") ??
      localStorage.getItem("pl.mock.modsets") ??
      ""
    );
  } catch {
    return "";
  }
}

/** The preview instance `?modsets=demo` adds to the library. */
export const DEMO_SET_INSTANCE: InstanceSummary = {
  id: "pvp-26-3",
  name: "PvP 26.3",
  icon: "swords",
  mcVersion: "26.3",
  loader: "fabric",
  loaderVersion: "0.19.5",
  managed: false,
  lastPlayedAt: new Date(Date.now() - 2 * 3600_000).toISOString(),
  playTimeSecs: 6 * 3600 + 20 * 60,
  state: "ready",
  createdAt: new Date(Date.now() - 4 * 86_400_000).toISOString(),
};

// The set data is a separate chunk: only the preview ever loads it.
let data: MockSetCard[] | null = null;
export async function loadMockSets(): Promise<MockSetCard[]> {
  if (!data) data = (await import("./mock-modsets-data")).mockSetCatalog;
  return data;
}

interface Added {
  setId: string;
  minecraft: string;
  /** Item id → the player's choice (missing: the default). */
  choices: Record<string, boolean>;
  /** Items the team added since the player last looked ("Yeni"). */
  unseen: Set<string>;
  updateAvailable: boolean;
}

/** Instance id → set id → what the preview pretends the set installed. */
const added = new Map<string, Map<string, Added>>();
const cancelled = new Set<string>();

function setsOf(instanceId: string): Map<string, Added> {
  let sets = added.get(instanceId);
  if (!sets) {
    sets = new Map();
    added.set(instanceId, sets);
  }
  return sets;
}

const folderOf: Record<SetItemKind, string> = {
  mod: "mods",
  resourcePack: "resourcepacks",
  shaderPack: "shaderpacks",
};

function variantOf(card: MockSetCard | undefined, minecraft: string) {
  return card?.variants.find((v) => v.minecraft === minecraft);
}

/** Items on after `choices`: chosen ones and, transitively, what they require. */
function enabledOf(items: MockSetItem[], choices: Record<string, boolean>): Set<string> {
  const byId = new Map(items.map((i) => [i.id, i]));
  const on = new Set<string>();
  const visit = (id: string) => {
    const item = byId.get(id);
    if (!item || on.has(id)) return;
    on.add(id);
    for (const required of item.requires) visit(required);
  };
  for (const item of items) {
    if (!item.library && (choices[item.id] ?? item.defaultEnabled)) visit(item.id);
  }
  return on;
}

function centerItem(
  item: MockSetItem,
  items: MockSetItem[],
  state: Added | undefined,
  enabled: Set<string>,
): ModCenterItem {
  const selected = !item.library && (state?.choices[item.id] ?? item.defaultEnabled);
  const isNew = Boolean(state?.unseen.has(item.id)) && !item.library;
  return {
    id: item.id,
    title: item.title,
    summary: item.summary,
    category: item.library ? "library" : item.category,
    iconUrl: item.iconUrl,
    pageUrl: item.pageUrl,
    projectId: item.projectId,
    fileName: item.fileName,
    folder: folderOf[item.kind],
    library: item.library,
    defaultEnabled: item.defaultEnabled,
    requires: item.requires,
    selected: isNew ? false : selected,
    enabled: enabled.has(item.id),
    isNew,
    requiredBy: items
      .filter((other) => enabled.has(other.id) && other.requires.includes(item.id))
      .map((other) => other.id),
    installed: Boolean(state) && enabled.has(item.id),
    updateAvailable: Boolean(state?.updateAvailable) && item.id === "w7ThoJFB",
    sizeBytes: item.sizeBytes,
    modified: false,
    disabledCopy: false,
  };
}

const noCounts = (): CategoryCounts => ({
  mods: 0,
  resourcePacks: 0,
  shaderPacks: 0,
  configs: 0,
  other: 0,
});

function countOf(counts: CategoryCounts, kind: SetItemKind) {
  if (kind === "mod") counts.mods += 1;
  else if (kind === "resourcePack") counts.resourcePacks += 1;
  else counts.shaderPacks += 1;
}

function badgeOf(card: MockSetCard): SetBadge {
  return { id: card.id, name: card.name, purpose: card.purpose };
}

/**
 * Files the preview's sets put into an instance, for the mods and packs tabs (with the set's
 * badge). Empty until the set data is loaded (`loadMockSets`).
 */
export function mockSetFiles(instanceId: string): {
  mods: ModEntry[];
  resourcePacks: PackEntry[];
  shaderPacks: PackEntry[];
} {
  const files = {
    mods: [] as ModEntry[],
    resourcePacks: [] as PackEntry[],
    shaderPacks: [] as PackEntry[],
  };
  if (!data) return files;
  for (const state of added.get(instanceId)?.values() ?? []) {
    const card = data.find((c) => c.id === state.setId);
    const variant = variantOf(card, state.minecraft);
    if (!card || !variant) continue;
    const enabled = enabledOf(variant.items, state.choices);
    for (const item of variant.items.filter((i) => enabled.has(i.id))) {
      const modSet = badgeOf(card);
      if (item.kind === "mod") {
        files.mods.push({
          fileName: item.fileName,
          name: item.title,
          version: "",
          authors: [],
          description: item.summary ?? "",
          enabled: true,
          iconUrl: item.iconUrl,
          fromPack: false,
          sizeBytes: item.sizeBytes,
          projectId: item.projectId,
          modSet,
        });
      } else {
        const entry: PackEntry = {
          fileName: item.fileName,
          name: item.title,
          description: item.summary ?? "",
          enabled: true,
          iconUrl: item.iconUrl,
          sizeBytes: item.sizeBytes,
          projectId: item.projectId,
          modSet,
        };
        if (item.kind === "resourcePack") files.resourcePacks.push(entry);
        else files.shaderPacks.push(entry);
      }
    }
  }
  return files;
}

async function simulate(jobId: string, onProgress: (p: JobProgress) => void, totalFiles: number) {
  onProgress({ stage: "resolving", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
  await delay(420);
  const steps = 14;
  for (let i = 1; i <= steps; i++) {
    if (cancelled.delete(jobId)) throw new Error("İşlem iptal edildi.");
    await delay(110);
    onProgress({
      stage: "downloading",
      percent: (i / steps) * 100,
      doneFiles: Math.round((i / steps) * totalFiles),
      totalFiles,
      bytesPerSec: 4_200_000 + Math.random() * 1_600_000,
    });
  }
  onProgress({ stage: "finishing", doneFiles: totalFiles, totalFiles, bytesPerSec: 0 });
  await delay(260);
}

export function createMockModSets(host: {
  instanceOf: (id: string) => InstanceSummary | undefined;
  createInstance: (spec: NewInstance) => Promise<InstanceSummary>;
}): ModSetsApi {
  if (modSetsFlag() === "demo") {
    setsOf(DEMO_SET_INSTANCE.id).set("pvp", {
      setId: "pvp",
      minecraft: "26.3",
      choices: {},
      // HitColor came with the team's last change: it is new for this player.
      unseen: new Set(["VXFg68YU"]),
      updateAvailable: true,
    });
  }

  const catalogCards = async (): Promise<MockSetCard[]> =>
    modSetsFlag() === "empty" ? [] : loadMockSets();

  const preview = async (
    instanceId: string,
    setId: string,
    selection?: ModSelection,
  ): Promise<ModCenterPreview> => {
    await delay(160);
    const card = (await loadMockSets()).find((c) => c.id === setId);
    const instance = host.instanceOf(instanceId);
    const state = added.get(instanceId)?.get(setId);
    const variant = variantOf(card, state?.minecraft ?? instance?.mcVersion ?? "");
    const summary: SyncSummary = {
      added: noCounts(),
      updated: noCounts(),
      removed: noCounts(),
      kept: 0,
      unchanged: 0,
      downloadBytes: 0,
      removeBytes: 0,
    };
    if (!variant) return { items: [], summary, changesFiles: false, recordsChoices: false };
    const choices = { ...(state?.choices ?? {}), ...(selection?.items ?? {}) };
    const before = state ? enabledOf(variant.items, state.choices) : new Set<string>();
    const after = enabledOf(variant.items, choices);
    for (const item of variant.items) {
      if (after.has(item.id) && !before.has(item.id)) {
        countOf(summary.added, item.kind);
        summary.downloadBytes += item.sizeBytes;
      } else if (!after.has(item.id) && before.has(item.id)) {
        countOf(summary.removed, item.kind);
        summary.removeBytes += item.sizeBytes;
      } else if (after.has(item.id)) {
        summary.unchanged += 1;
      }
    }
    if (state?.updateAvailable) {
      summary.updated.mods += 1;
      summary.downloadBytes += 579_146;
    }
    const changesFiles =
      summary.downloadBytes > 0 || summary.removeBytes > 0 || summary.updated.mods > 0;
    return {
      items: variant.items.map((item) => centerItem(item, variant.items, state, after)),
      summary,
      changesFiles,
      recordsChoices: true,
    };
  };

  return {
    async catalog() {
      await delay(280);
      if (modSetsFlag() === "offline") return { sets: [], unavailable: true };
      const cards: SetCard[] = await catalogCards();
      return { sets: cards, unavailable: false };
    },
    async instance(instanceId) {
      await delay(200);
      const instance = host.instanceOf(instanceId);
      const cards = await loadMockSets();
      const sets: InstanceSet[] = [];
      for (const state of added.get(instanceId)?.values() ?? []) {
        const card = cards.find((c) => c.id === state.setId);
        const variant = variantOf(card, state.minecraft);
        if (!card || !variant) continue;
        const enabled = enabledOf(variant.items, state.choices);
        const items = variant.items.map((item) => centerItem(item, variant.items, state, enabled));
        sets.push({
          id: card.id,
          name: card.name,
          nameEn: card.nameEn,
          summary: card.summary,
          summaryEn: card.summaryEn,
          purpose: card.purpose,
          installed: true,
          available: true,
          updateAvailable: state.updateAvailable,
          retired: false,
          items,
          present: [],
          pending: false,
          issues: [],
          sizeBytes: variant.items
            .filter((item) => enabled.has(item.id))
            .reduce((sum, item) => sum + item.sizeBytes, 0),
        });
      }
      return {
        instanceId,
        mcVersion: instance?.mcVersion ?? "",
        loader: instance?.loader ?? "vanilla",
        sets,
        gameRunning: false,
        unavailable: modSetsFlag() === "offline" && sets.length > 0,
      };
    },
    preview,
    async apply(instanceId, setId, selection, jobId, onProgress) {
      const plan = await preview(instanceId, setId, selection);
      const files =
        plan.summary.added.mods +
        plan.summary.added.resourcePacks +
        plan.summary.added.shaderPacks +
        plan.summary.updated.mods;
      await simulate(jobId, onProgress, Math.max(files, 1));
      const instance = host.instanceOf(instanceId);
      const sets = setsOf(instanceId);
      const state = sets.get(setId) ?? {
        setId,
        minecraft: instance?.mcVersion ?? "",
        choices: {},
        unseen: new Set<string>(),
        updateAvailable: false,
      };
      state.choices = { ...state.choices, ...(selection?.items ?? {}) };
      state.unseen = new Set();
      state.updateAvailable = false;
      sets.set(setId, state);
      return { downloaded: files, restored: 0, removed: 0, issues: 0 };
    },
    async remove(instanceId, setId, jobId, onProgress) {
      await simulate(jobId, onProgress, 4);
      added.get(instanceId)?.delete(setId);
      return { downloaded: 0, restored: 0, removed: 4, issues: 0 };
    },
    async markSeen(instanceId, setId) {
      const state = added.get(instanceId)?.get(setId);
      const card = (await loadMockSets()).find((c) => c.id === setId);
      const variant = variantOf(card, state?.minecraft ?? "");
      if (state && variant) state.unseen = new Set();
    },
    async createInstance(request, jobId, onProgress) {
      const card = (await loadMockSets()).find((c) => c.id === request.setId);
      const variant = variantOf(card, request.minecraft);
      if (!card || !variant) throw new Error("Bu set, seçilen sürüm için hazır değil.");
      const created = await host.createInstance({
        name: request.name,
        icon: request.icon as NewInstance["icon"],
        mcVersion: variant.minecraft,
        loader: variant.loader ?? "vanilla",
        loaderVersion: variant.loaderVersion,
      });
      const enabled = enabledOf(variant.items, request.selection?.items ?? {});
      await simulate(jobId, onProgress, enabled.size);
      setsOf(created.id).set(card.id, {
        setId: card.id,
        minecraft: variant.minecraft,
        choices: request.selection?.items ?? {},
        unseen: new Set(),
        updateAvailable: false,
      });
      return created;
    },
    async cancel(jobId) {
      cancelled.add(jobId);
    },
  };
}
