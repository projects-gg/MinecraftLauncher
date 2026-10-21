// Tauri implementation of `api.help` (the troubleshooter): maps the generated bindings onto the UI
// shapes (`null` becomes absent).
import { Channel } from "@tauri-apps/api/core";
import {
  type CmdError,
  type CrashView,
  commands,
  type Facts,
  type Finding,
  type TroubleshootEvent,
} from "@/bindings";
import type { HelpApi, HelpCrash, HelpFacts, HelpFinding } from "./help-types";
import { ModCenterCommandError } from "./modcenter-tauri";

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new ModCenterCommandError(result.error);
  return result.data;
}

function facts(f: Facts): HelpFacts {
  const out: HelpFacts = {};
  if (f.latencyMs !== null) out.latencyMs = f.latencyMs;
  if (f.players !== null) out.players = f.players;
  if (f.freeBytes !== null) out.freeBytes = f.freeBytes;
  if (f.major !== null) out.major = f.major;
  if (f.need !== null) out.need = f.need;
  if (f.count !== null) out.count = f.count;
  if (f.minutes !== null) out.minutes = f.minutes;
  if (f.at !== null) out.at = f.at;
  if (f.crash !== null) out.crash = f.crash;
  if (f.name !== null) out.name = f.name;
  return out;
}

const finding = (f: Finding): HelpFinding => ({
  check: f.check,
  level: f.level,
  code: f.code,
  fix: f.fix ?? undefined,
  facts: facts(f.facts),
});

const crash = (view: CrashView): HelpCrash => ({
  title: view.title,
  hint: view.hint,
  actions: view.actions,
  mods: view.mods,
  logFile: view.logFile,
  crashReport: view.crashReport,
  kind: view.kind,
  instanceId: view.instanceId,
  suspects: view.suspects.map((s) => ({ ...s, related: s.related ?? undefined })),
  repairable: view.repairable,
  turnOff: view.turnOff,
  reinstall: view.reinstall,
  memory: view.memory,
  resetJvmArgs: view.resetJvmArgs,
});

export const tauriHelp: HelpApi = {
  async run(instanceId, onEvent) {
    const channel = new Channel<TroubleshootEvent>();
    channel.onmessage = (event) => {
      if (event.type === "plan") onEvent({ type: "plan", checks: event.checks });
      else onEvent({ type: "finding", finding: finding(event.finding) });
    };
    const report = await ok(commands.troubleshootRun(instanceId, channel));
    return {
      findings: report.findings.map(finding),
      system: report.system,
      instance: report.instance
        ? {
            ...report.instance,
            loaderVersion: report.instance.loaderVersion ?? undefined,
            packVersion: report.instance.packVersion ?? undefined,
          }
        : undefined,
    };
  },
  async crash(instanceId) {
    const view = await ok(commands.troubleshootCrash(instanceId));
    return view ? crash(view) : null;
  },
  async openClock() {
    await ok(commands.troubleshootOpenClock());
  },
};
