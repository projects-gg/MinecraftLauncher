// Browser preview of `api.cleanup`: a pretend data folder with old sets, unused game files and
// leftovers that really go away when cleaned (until reload).
// `localStorage["pl.mock.cleanup"] = "empty"` shows a tidy folder; `"blocked"` pretends a game
// runs; `"slow"` stretches every step (screenshots of progress).
import type {
  CleanupApi,
  CleanupBlock,
  Leftover,
  OldSet,
  RemovedSets,
  RestoredSets,
  SharedGroup,
  SharedItem,
  TrashedFile,
  TrashedSet,
} from "./cleanup-types";
import { pick } from "./mock-lang";
import { mockTrashedInstances } from "./mock-trash";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const MB = 1024 * 1024;
const GB = 1024 * MB;
const DAY = 24 * 60 * 60 * 1000;

const mode = () => {
  try {
    return localStorage.getItem("pl.mock.cleanup") ?? "";
  } catch {
    return "";
  }
};
const slow = () => (mode() === "slow" ? 3 : 1);

interface MockSet extends OldSet {
  daysAgo: number;
}

function sets(): MockSet[] {
  const set = (
    instanceId: string,
    instanceName: string,
    minecraft: string,
    daysAgo: number,
    sizeBytes: number,
    loader: OldSet["loader"] = "fabric",
  ): MockSet => ({
    key: `${instanceId}/${minecraft}`,
    instanceId,
    instanceName,
    minecraft,
    loader,
    daysAgo,
    lastUsedAt: new Date(Date.now() - daysAgo * DAY).toISOString(),
    sizeBytes,
  });
  return [
    set("projects", "Projects", "1.19.4", 214, Math.round(366.4 * MB)),
    set("projects", "Projects", "1.20.1", 143, Math.round(842.7 * MB)),
    set("skyblock", "Skyblock macerası", "1.18.2", 96, Math.round(1.38 * GB), "forge"),
    set("yaratici", "Yaratıcı dünya", "1.20.4", 71, Math.round(127.9 * MB)),
    set("yaratici", "Yaratıcı dünya", "1.21.1", 38, Math.round(212.3 * MB)),
  ];
}

function shared(): { items: SharedItem[]; groups: SharedGroup[] } {
  return {
    items: [
      {
        id: "version:1.16.5",
        kind: "gameVersion",
        name: "1.16.5",
        minecraft: "1.16.5",
        bytes: Math.round(311.2 * MB),
        files: 3,
      },
      {
        id: "version:1.20.1-forge-47.1.0",
        kind: "loaderProfile",
        name: "1.20.1-forge-47.1.0",
        minecraft: "1.20.1",
        loader: "forge",
        loaderVersion: "47.1.0",
        bytes: Math.round(46.8 * MB),
        files: 4,
      },
      {
        id: "version:fabric-loader-0.14.21-1.20.1",
        kind: "loaderProfile",
        name: "fabric-loader-0.14.21-1.20.1",
        minecraft: "1.20.1",
        loader: "fabric",
        loaderVersion: "0.14.21",
        bytes: Math.round(2.1 * MB),
        files: 2,
      },
      {
        id: "runtime:jre-legacy",
        kind: "javaRuntime",
        name: "jre-legacy",
        javaMajor: 8,
        bytes: Math.round(196.4 * MB),
        files: 211,
      },
      { id: "libraries", kind: "libraries", name: "", bytes: Math.round(238.6 * MB), files: 412 },
      { id: "assets", kind: "assets", name: "", bytes: Math.round(690.3 * MB), files: 1893 },
    ],
    groups: [],
  };
}

function leftovers(): Leftover[] {
  return [
    { kind: "cache", bytes: Math.round(18.4 * MB), files: 312 },
    { kind: "downloads", bytes: Math.round(6.1 * MB), files: 9 },
    { kind: "logs", bytes: Math.round(41.7 * MB), files: 23 },
    { kind: "crashReports", bytes: Math.round(1.2 * MB), files: 7 },
  ];
}

/** A set an earlier cleanup moved to the trash. */
function trashed(): Array<TrashedSet & { at: number }> {
  const at = Date.now() - 12 * DAY;
  return [
    {
      trashId: "20261004-5c1e09a2",
      instanceId: "skyblock",
      instanceName: "Skyblock macerası",
      minecraft: "1.17.1",
      deletedAt: new Date(at).toISOString(),
      sizeBytes: Math.round(598.2 * MB),
      at,
    },
  ];
}

/** A world, a screenshot and the mod folder a reinstall set aside, deleted earlier. */
function trashedFiles(): TrashedFile[] {
  const ago = (days: number) => new Date(Date.now() - days * DAY).toISOString();
  return [
    {
      trashId: "20261015-a41c2e90",
      instanceId: "projects",
      instanceName: "Projects",
      kind: "screenshot",
      name: "2026-10-15_21.40.12.png",
      fileName: "2026-10-15_21.40.12.png",
      deletedAt: ago(2),
      sizeBytes: Math.round(2.6 * MB),
      restorable: true,
    },
    {
      trashId: "20261012-7be03d14",
      instanceId: "projects",
      instanceName: "Projects",
      kind: "world",
      name: "Eski köy",
      fileName: "Eski köy",
      deletedAt: ago(5),
      sizeBytes: Math.round(312.4 * MB),
      restorable: true,
    },
    {
      trashId: "20261008-c2d95f01",
      instanceId: "skyblock",
      instanceName: "Skyblock macerası",
      kind: "modSet",
      name: "1.20.1 mods",
      fileName: "mods",
      deletedAt: ago(9),
      sizeBytes: Math.round(97.5 * MB),
      restorable: false,
    },
  ];
}

export function createMockCleanup(): CleanupApi {
  const empty = mode() === "empty";
  // An instance deleted three days ago ("Geri al" brings it back to the library).
  if (!empty && mockTrashedInstances.size === 0) {
    const at = Date.now() - 3 * DAY;
    mockTrashedInstances.set("20261014-3fa21c07", {
      instance: {
        id: "eski-survival",
        name: "Eski survival",
        icon: "pickaxe",
        mcVersion: "1.20.4",
        loader: "fabric",
        loaderVersion: "0.16.10",
        managed: false,
        playTimeSecs: 31 * 3600,
        state: "ready",
        createdAt: new Date(at - 60 * DAY).toISOString(),
      },
      deletedAt: at,
      sizeBytes: Math.round(1.6 * GB),
    });
  }
  const instanceBytes = () =>
    [...mockTrashedInstances.values()].reduce((sum, item) => sum + item.sizeBytes, 0);
  const state = {
    sets: empty ? [] : sets(),
    shared: empty ? { items: [], groups: [] } : shared(),
    leftovers: empty ? [] : leftovers(),
    trash: empty ? [] : trashed(),
    /** Sets put back from the trash: not offered again for a while. */
    kept: new Set<string>(),
    /** Worlds, screenshots and set-aside mod folders (listed with the rest). */
    files: empty ? [] : trashedFiles(),
    free: Math.round(118.2 * GB),
  };
  const trashBytes = () =>
    state.trash.reduce((sum, item) => sum + item.sizeBytes, 0) +
    state.files.reduce((sum, item) => sum + item.sizeBytes, 0) +
    instanceBytes();
  const offered = (olderThanDays: number) =>
    state.sets.filter((set) => set.daysAgo >= olderThanDays && !state.kept.has(set.key));

  return {
    async status() {
      await delay(120);
      const blocked: CleanupBlock | undefined = mode() === "blocked" ? "gameRunning" : undefined;
      return { blocked, freeBytes: state.free, diskBytes: Math.round(476.9 * GB) };
    },
    // Nothing runs in the preview: no game, no jobs to report.
    onStatusChanged: () => () => {},
    async oldSets(olderThanDays) {
      await delay(380 * slow());
      const list = offered(olderThanDays);
      return {
        sets: list.map(({ daysAgo: _, ...set }) => set),
        totalBytes: list.reduce((sum, set) => sum + set.sizeBytes, 0),
        olderThanDays,
      };
    },
    async removeSets(chosen, olderThanDays) {
      await delay(700 * slow());
      const removed: RemovedSets["removed"] = [];
      let skipped = 0;
      for (const ref of chosen) {
        const index = state.sets.findIndex(
          (set) =>
            set.instanceId === ref.instanceId &&
            set.minecraft === ref.minecraft &&
            set.daysAgo >= olderThanDays &&
            !state.kept.has(set.key),
        );
        if (index < 0) {
          skipped++;
          continue;
        }
        const [set] = state.sets.splice(index, 1);
        if (!set) continue;
        const item = {
          instanceId: set.instanceId,
          minecraft: set.minecraft,
          trashId: `20261016-${Math.random().toString(16).slice(2, 10)}`,
          sizeBytes: set.sizeBytes,
        };
        removed.push(item);
        const at = Date.now();
        state.trash.unshift({
          ...item,
          instanceName: set.instanceName,
          deletedAt: new Date(at).toISOString(),
          at,
        });
      }
      return {
        removed,
        movedBytes: removed.reduce((sum, item) => sum + item.sizeBytes, 0),
        skipped,
      };
    },
    async restoreSets(trashIds) {
      await delay(500 * slow());
      const result: RestoredSets = { restored: 0, taken: 0, failed: 0 };
      for (const id of trashIds) {
        const index = state.trash.findIndex((item) => item.trashId === id);
        const [item] = index >= 0 ? state.trash.splice(index, 1) : [];
        if (!item) {
          result.failed++;
          continue;
        }
        result.restored++;
        state.kept.add(`${item.instanceId}/${item.minecraft}`);
        const original = sets().find(
          (set) => set.instanceId === item.instanceId && set.minecraft === item.minecraft,
        );
        if (original) {
          state.sets.push(original);
          state.sets.sort((a, b) => b.daysAgo - a.daysAgo);
        }
      }
      return result;
    },
    async sharedFiles() {
      await delay(900 * slow());
      const { items, groups } = state.shared;
      return {
        items,
        groups,
        totalBytes:
          items.reduce((sum, item) => sum + item.bytes, 0) +
          groups.reduce((sum, group) => sum + group.bytes, 0),
      };
    },
    async deleteShared(ids, onProgress) {
      const chosen = state.shared.items.filter((item) => ids.includes(item.id));
      const total = chosen.reduce((sum, item) => sum + item.files, 0);
      const steps = 24;
      for (let i = 0; i <= steps; i++) {
        onProgress({ done: Math.round((total * i) / steps), total });
        await delay(55 * slow());
      }
      state.shared.items = state.shared.items.filter((item) => !ids.includes(item.id));
      const freedBytes = chosen.reduce((sum, item) => sum + item.bytes, 0);
      state.free += freedBytes;
      return { freedBytes, removedFiles: total };
    },
    async leftovers() {
      await delay(520 * slow());
      return state.leftovers.map((item) => ({ ...item }));
    },
    async cleanLeftovers(kinds) {
      await delay(650 * slow());
      const chosen = state.leftovers.filter((item) => kinds.includes(item.kind));
      state.leftovers = state.leftovers.filter((item) => !kinds.includes(item.kind));
      const freedBytes = chosen.reduce((sum, item) => sum + item.bytes, 0);
      state.free += freedBytes;
      return { freedBytes, removedFiles: chosen.reduce((sum, item) => sum + item.files, 0) };
    },
    async trash() {
      await delay(200);
      const deleted = [...mockTrashedInstances.entries()].sort(
        (a, b) => b[1].deletedAt - a[1].deletedAt,
      );
      const times = [
        ...state.trash.map((item) => item.at),
        ...state.files.map((item) => Date.parse(item.deletedAt ?? "")),
        ...deleted.map(([, d]) => d.deletedAt),
      ];
      const oldest = times.length > 0 ? Math.min(...times) : null;
      return {
        items: state.trash.length + state.files.length + deleted.length,
        bytes: trashBytes(),
        capBytes: Math.round(20 * GB),
        nextExpiryAt: oldest === null ? undefined : new Date(oldest + 30 * DAY).toISOString(),
        sets: state.trash.map(({ at: _, ...set }) => set),
        instances: deleted.map(([trashId, d]) => ({
          trashId,
          name: d.instance.name,
          deletedAt: new Date(d.deletedAt).toISOString(),
          sizeBytes: d.sizeBytes,
        })),
        files: state.files.map((item) => ({ ...item })),
      };
    },
    async emptyTrash() {
      await delay(800 * slow());
      const freedBytes = trashBytes();
      const removedFiles = state.trash.length + state.files.length + mockTrashedInstances.size;
      state.trash = [];
      state.files = [];
      mockTrashedInstances.clear();
      state.free += freedBytes;
      return { freedBytes, removedFiles };
    },
    async deleteTrashedSet(trashId) {
      await delay(450 * slow());
      const instance = mockTrashedInstances.get(trashId);
      if (instance) {
        mockTrashedInstances.delete(trashId);
        state.free += instance.sizeBytes;
        return { freedBytes: instance.sizeBytes, removedFiles: 1 };
      }
      const file = state.files.find((item) => item.trashId === trashId);
      if (file) {
        state.files = state.files.filter((item) => item !== file);
        state.free += file.sizeBytes;
        return { freedBytes: file.sizeBytes, removedFiles: 1 };
      }
      const index = state.trash.findIndex((item) => item.trashId === trashId);
      const [item] = index >= 0 ? state.trash.splice(index, 1) : [];
      if (!item) throw new Error("not in the trash");
      state.free += item.sizeBytes;
      return { freedBytes: item.sizeBytes, removedFiles: 1 };
    },
    async restoreTrashed(_instanceId, trashId) {
      await delay(300 * slow());
      const file = state.files.find((item) => item.trashId === trashId && item.restorable);
      if (!file) {
        throw Object.assign(
          new Error(pick("Bu öğe çöp kutusunda yok.", "This item isn't in the trash.")),
          {
            code: "COP-YOK",
            details: `${trashId}: mock`,
          },
        );
      }
      state.files = state.files.filter((item) => item !== file);
      return file.fileName;
    },
    async summary(olderThanDays) {
      await delay(300);
      const setBytes = offered(olderThanDays).reduce((sum, set) => sum + set.sizeBytes, 0);
      const sharedBytes = state.shared.items.reduce((sum, item) => sum + item.bytes, 0);
      const leftoverBytes = state.leftovers.reduce((sum, item) => sum + item.bytes, 0);
      return { reclaimableBytes: setBytes + sharedBytes + leftoverBytes, freeBytes: state.free };
    },
  };
}
