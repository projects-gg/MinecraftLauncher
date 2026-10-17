// Tauri implementation of `api.modCenter`: maps the generated bindings onto the UI shapes.
import { Channel } from "@tauri-apps/api/core";
import {
  type CmdError,
  commands,
  type JobProgressView,
  type MigrationPlanView,
  type ModCenterItemView,
  type ModSelectionInput,
} from "@/bindings";
import type { JobProgress } from "./content-types";
import type {
  MigrationPlan,
  ModCenterApi,
  ModCenterItem,
  ModSelection,
  SyncSummary,
} from "./modcenter-types";

/** A failed command: friendly `message`, plus `code`/`details` for "copy details". */
export class ModCenterCommandError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(error: CmdError) {
    super(error.message);
    this.code = error.code;
    this.details = error.details;
  }
}

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new ModCenterCommandError(result.error);
  return result.data;
}

function progressChannel(onProgress: (p: JobProgress) => void): Channel<JobProgressView> {
  const channel = new Channel<JobProgressView>();
  channel.onmessage = (p) => onProgress({ ...p, percent: p.percent ?? undefined });
  return channel;
}

const item = (i: ModCenterItemView): ModCenterItem => ({
  ...i,
  summary: i.summary ?? undefined,
  iconUrl: i.iconUrl ?? undefined,
  pageUrl: i.pageUrl ?? undefined,
  projectId: i.projectId ?? undefined,
  manual: i.manual ?? undefined,
});

const selection = (s: ModSelection): ModSelectionInput => ({
  items: s.items,
  revert: s.revert ?? [],
});

const plan = (p: MigrationPlanView): MigrationPlan => ({
  ...p,
  mods: p.mods.map((m) => ({
    ...m,
    projectId: m.projectId ?? undefined,
    currentVersion: m.currentVersion ?? undefined,
    targetVersionId: m.targetVersionId ?? undefined,
    targetVersionNumber: m.targetVersionNumber ?? undefined,
  })),
});

export const tauriModCenter: ModCenterApi = {
  async view(instanceId) {
    const v = await ok(commands.modCenter(instanceId));
    return {
      ...v,
      pack: v.pack
        ? { ...v.pack, installedVersion: v.pack.installedVersion ?? undefined }
        : undefined,
      items: v.items.map(item),
      issues: v.issues.map((issue) => ({ ...issue, title: issue.title ?? undefined })),
      dropped: v.dropped
        ? { minecraft: v.dropped.minecraft, recommended: v.dropped.recommended ?? undefined }
        : undefined,
    };
  },
  async preview(instanceId, choice) {
    const p = await ok(commands.modCenterPreview(instanceId, selection(choice)));
    return { ...p, items: p.items.map(item), summary: p.summary as SyncSummary };
  },
  apply: (instanceId, choice, jobId, onProgress) =>
    ok(commands.modCenterApply(instanceId, selection(choice), jobId, progressChannel(onProgress))),
  async markSeen(instanceId) {
    await ok(commands.modCenterMarkSeen(instanceId));
  },
  async versions(instanceId, includeSnapshots) {
    const v = await ok(commands.instanceVersions(instanceId, includeSnapshots));
    return {
      ...v,
      options: v.options.map((o) => ({ ...o, lastPlayedAt: o.lastPlayedAt ?? undefined })),
    };
  },
  async switchPreview(instanceId, minecraft) {
    const p = await ok(commands.instanceSwitchPreview(instanceId, minecraft));
    return {
      ...p,
      restoredLastPlayedAt: p.restoredLastPlayedAt ?? undefined,
      loaderVersion: p.loaderVersion ?? undefined,
      trashedSet: p.trashedSet
        ? { ...p.trashedSet, lastPlayedAt: p.trashedSet.lastPlayedAt ?? undefined }
        : undefined,
    };
  },
  switchVersion: (instanceId, minecraft, jobId, onProgress) =>
    ok(commands.instanceSwitchVersion(instanceId, minecraft, jobId, progressChannel(onProgress))),
  async restoreTrashedSet(instanceId, trashId) {
    await ok(commands.instanceRestoreTrashedSet(instanceId, trashId));
  },
  async migrationPlan(instanceId, from, to) {
    return plan(await ok(commands.modsMigrationPlan(instanceId, from ?? null, to)));
  },
  migrationCheck: (instanceId, from, to) => ok(commands.modsMigrationCheck(instanceId, from, to)),
  migrate: (instanceId, to, versionIds, jobId, onProgress) =>
    ok(commands.modsMigrate(instanceId, to, versionIds, jobId, progressChannel(onProgress))),
};
