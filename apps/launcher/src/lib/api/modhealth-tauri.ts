// Tauri implementation of `api.modHealth`: maps the generated bindings onto the UI shapes.
import { Channel } from "@tauri-apps/api/core";
import {
  type CmdError,
  commands,
  type KeyChangeView,
  type ModCheckView,
  type ModHealthIssueView,
  type ReinstallProgressView,
  type RepairProgressView,
  type RepairStepView,
} from "@/bindings";
import { ModCenterCommandError } from "./modcenter-tauri";
import type {
  HealthIssue,
  KeyChange,
  ModCheck,
  ModHealthApi,
  ReinstallProgress,
  RepairProgress,
  RepairStep,
} from "./modhealth-types";

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new ModCenterCommandError(result.error);
  return result.data;
}

const keyChange = (k: KeyChangeView): KeyChange => ({ ...k, to: k.to ?? undefined });

const issue = ({ keys, ...i }: ModHealthIssueView): HealthIssue => ({
  ...i,
  target: i.target ?? undefined,
  madeFor: i.madeFor ?? undefined,
  java: i.java ?? undefined,
  offer: i.offer ? { ...i.offer, name: i.offer.name ?? undefined } : undefined,
  keys: keys.length > 0 ? keys.map(keyChange) : undefined,
});

export const modCheck = (c: ModCheckView): ModCheck => ({ ...c, issues: c.issues.map(issue) });

const step = (s: RepairStepView): RepairStep => ({ ...s, note: s.note ?? undefined });

const repairProgress = (p: RepairProgressView): RepairProgress => ({
  steps: p.steps.map(step),
  percent: p.percent ?? undefined,
  checking: p.checking,
});

const reinstallProgress = (p: ReinstallProgressView): ReinstallProgress => ({
  ...p,
  percent: p.percent ?? undefined,
});

export const tauriModHealth: ModHealthApi = {
  async check(instanceId) {
    return modCheck(await ok(commands.modsCheck(instanceId)));
  },
  async repair(instanceId, scope, jobId, onProgress) {
    const channel = new Channel<RepairProgressView>();
    channel.onmessage = (p) => onProgress(repairProgress(p));
    const r = await ok(commands.modsRepair(instanceId, scope, jobId, channel));
    return { steps: r.steps.map(step), check: modCheck(r.check), cancelled: r.cancelled };
  },
  reinstallPreview: (instanceId) => ok(commands.modsReinstallPreview(instanceId)),
  async reinstall(instanceId, jobId, onProgress) {
    const channel = new Channel<ReinstallProgressView>();
    channel.onmessage = (p) => onProgress(reinstallProgress(p));
    const r = await ok(commands.modsReinstall(instanceId, jobId, channel));
    return { ...r, keptFolder: r.keptFolder ?? undefined, check: modCheck(r.check) };
  },
  setMaxMemory: (instanceId, maxMb) => ok(commands.instanceSetMaxMemory(instanceId, maxMb)),
  setJvmArgs: (instanceId, jvmArgs) => ok(commands.instanceSetJvmArgs(instanceId, jvmArgs)),
  async cancel(jobId) {
    await commands.contentCancel(jobId);
  },
  async fixKeys(instanceId) {
    const fix = await ok(commands.modsFixKeys(instanceId));
    return { backup: fix.backup ?? undefined, changes: fix.changes.map(keyChange) };
  },
  undoKeys: (instanceId, backup) => ok(commands.modsUndoKeys(instanceId, backup)),
};
