// Tauri implementation of `api.modSets`: maps the generated bindings onto the UI shapes.
import { Channel } from "@tauri-apps/api/core";
import {
  type CmdError,
  commands,
  type InstanceSetView,
  type JobProgressView,
  type ModCenterItemView,
  type ModSelectionInput,
  type SetCardView,
  type SetItemPreview,
  type SetVariantView,
} from "@/bindings";
import type { JobProgress } from "./content-types";
import type { ModCenterItem, ModSelection, SyncSummary } from "./modcenter-types";
import type { InstanceSet, ModSetsApi, SetCard, SetItem, SetVariant } from "./modsets-types";
import type { InstanceIcon, InstanceSummary } from "./types";

/** A failed command: friendly `message`, plus `code`/`details` for "copy details". */
export class ModSetsCommandError extends Error {
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
  if (result.status === "error") throw new ModSetsCommandError(result.error);
  return result.data;
}

function progressChannel(onProgress: (p: JobProgress) => void): Channel<JobProgressView> {
  const channel = new Channel<JobProgressView>();
  channel.onmessage = (p) => onProgress({ ...p, percent: p.percent ?? undefined });
  return channel;
}

const selection = (s: ModSelection): ModSelectionInput => ({
  items: s.items,
  revert: s.revert ?? [],
});

const centerItem = (i: ModCenterItemView): ModCenterItem => ({
  ...i,
  summary: i.summary ?? undefined,
  iconUrl: i.iconUrl ?? undefined,
  pageUrl: i.pageUrl ?? undefined,
  projectId: i.projectId ?? undefined,
  manual: i.manual ?? undefined,
});

const setItem = (i: SetItemPreview): SetItem => ({
  ...i,
  summary: i.summary ?? undefined,
  iconUrl: i.iconUrl ?? undefined,
  pageUrl: i.pageUrl ?? undefined,
  projectId: i.projectId ?? undefined,
});

const variant = (v: SetVariantView): SetVariant => ({
  ...v,
  loader: v.loader ?? undefined,
  loaderVersion: v.loaderVersion ?? undefined,
  items: v.items.map(setItem),
});

const card = (c: SetCardView): SetCard => ({
  ...c,
  nameEn: c.nameEn ?? undefined,
  summary: c.summary ?? undefined,
  summaryEn: c.summaryEn ?? undefined,
  variants: c.variants.map(variant),
});

const instanceSet = (s: InstanceSetView): InstanceSet => ({
  ...s,
  nameEn: s.nameEn ?? undefined,
  summary: s.summary ?? undefined,
  summaryEn: s.summaryEn ?? undefined,
  items: s.items.map(centerItem),
  present: s.present.map(setItem),
  issues: s.issues.map((issue) => ({ ...issue, title: issue.title ?? undefined })),
});

export const tauriModSets: ModSetsApi = {
  async catalog() {
    const v = await ok(commands.modSetsCatalog());
    return { sets: v.sets.map(card), unavailable: v.unavailable };
  },
  async instance(instanceId) {
    const v = await ok(commands.modSetsInstance(instanceId));
    return { ...v, sets: v.sets.map(instanceSet) };
  },
  async preview(instanceId, setId, choice) {
    const p = await ok(
      commands.modSetPreview(instanceId, setId, choice ? selection(choice) : null),
    );
    return { ...p, items: p.items.map(centerItem), summary: p.summary as SyncSummary };
  },
  apply: (instanceId, setId, choice, jobId, onProgress) =>
    ok(
      commands.modSetApply(
        instanceId,
        setId,
        choice ? selection(choice) : null,
        jobId,
        progressChannel(onProgress),
      ),
    ),
  remove: (instanceId, setId, jobId, onProgress) =>
    ok(commands.modSetRemove(instanceId, setId, jobId, progressChannel(onProgress))),
  async markSeen(instanceId, setId) {
    await ok(commands.modSetMarkSeen(instanceId, setId));
  },
  async createInstance(request, jobId, onProgress) {
    const created = await ok(
      commands.instanceCreateFromSet(
        {
          setId: request.setId,
          minecraft: request.minecraft,
          name: request.name,
          icon: request.icon,
          selection: request.selection ? selection(request.selection) : null,
          maxMemoryMb: request.maxMemoryMb ?? null,
          jvmArgs: request.jvmArgs ?? null,
        },
        jobId,
        progressChannel(onProgress),
      ),
    );
    return {
      id: created.id,
      name: created.name,
      icon: created.icon as InstanceIcon,
      mcVersion: created.mcVersion,
      loader: created.loader,
      loaderVersion: created.loaderVersion ?? undefined,
      managed: created.managed,
      lastPlayedAt: created.lastPlayedAt ?? undefined,
      playTimeSecs: created.playTimeSecs,
      state: created.state as InstanceSummary["state"],
      autoJoin: created.autoJoin ?? undefined,
      group: created.group ?? undefined,
      createdAt: created.createdAt,
    };
  },
  async cancel(jobId) {
    await commands.contentCancel(jobId);
  },
};
