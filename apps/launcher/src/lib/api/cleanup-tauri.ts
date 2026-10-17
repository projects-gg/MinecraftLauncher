// Tauri implementation of `api.cleanup`: maps the generated bindings onto the UI shapes.
import { Channel } from "@tauri-apps/api/core";
import { type CleanupProgressView, type CmdError, commands } from "@/bindings";
import type { CleanupApi, SharedItem } from "./cleanup-types";
import { DataCommandError } from "./data-tauri";

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new DataCommandError(result.error);
  return result.data;
}

const defined = <T>(value: T | null | undefined): T | undefined => value ?? undefined;

export const tauriCleanup: CleanupApi = {
  async status() {
    const s = await ok(commands.cleanupStatus());
    return {
      blocked: defined(s.blocked),
      freeBytes: defined(s.freeBytes),
      diskBytes: defined(s.diskBytes),
    };
  },
  async oldSets(olderThanDays) {
    const v = await ok(commands.cleanupOldSets(olderThanDays));
    return {
      ...v,
      sets: v.sets.map((set) => ({
        ...set,
        loader: defined(set.loader),
        lastUsedAt: defined(set.lastUsedAt),
      })),
    };
  },
  removeSets: (sets, olderThanDays) => ok(commands.cleanupRemoveSets(sets, olderThanDays)),
  restoreSets: (trashIds) => ok(commands.cleanupRestoreSets(trashIds)),
  async sharedFiles() {
    const v = await ok(commands.cleanupSharedFiles());
    return {
      ...v,
      items: v.items.map(
        (item): SharedItem => ({
          ...item,
          minecraft: defined(item.minecraft),
          loader: defined(item.loader),
          loaderVersion: defined(item.loaderVersion),
          javaMajor: defined(item.javaMajor),
        }),
      ),
    };
  },
  async deleteShared(ids, onProgress) {
    const channel = new Channel<CleanupProgressView>();
    channel.onmessage = (p) => onProgress(p);
    return ok(commands.cleanupDeleteShared(ids, channel));
  },
  leftovers: () => ok(commands.cleanupLeftovers()),
  cleanLeftovers: (kinds) => ok(commands.cleanupCleanLeftovers(kinds)),
  async trash() {
    const t = await ok(commands.cleanupTrash());
    return {
      ...t,
      nextExpiryAt: defined(t.nextExpiryAt),
      sets: t.sets.map((set) => ({ ...set, deletedAt: defined(set.deletedAt) })),
      instances: t.instances.map((item) => ({ ...item, deletedAt: defined(item.deletedAt) })),
      files: t.files.map((item) => ({
        ...item,
        instanceName: defined(item.instanceName),
        deletedAt: defined(item.deletedAt),
      })),
    };
  },
  emptyTrash: () => ok(commands.cleanupEmptyTrash()),
  deleteTrashedSet: (trashId) => ok(commands.cleanupDeleteTrashedSet(trashId)),
  restoreTrashed: (instanceId, trashId) => ok(commands.trashRestore(instanceId, trashId)),
  async summary(olderThanDays) {
    const s = await ok(commands.cleanupSummary(olderThanDays));
    return { reclaimableBytes: s.reclaimableBytes, freeBytes: defined(s.freeBytes) };
  },
};
