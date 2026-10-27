// 7/24 Bulut: what the AFK page asks of the backend (commands/cloud_afk.rs over pl-cloud-afk).
// Nothing polls: the page reads the overview when it opens and after an action, and the console's
// live socket runs only while a console is watched. Browser previews use the mock (mock.ts).
import type { InvOp } from "@pl/ui/inventory";
import { listen } from "@tauri-apps/api/event";
import {
  type CloudAccount_Serialize,
  type CloudConsoleFile,
  type CloudCopyResult_Serialize,
  type CloudCopySecret,
  type CloudCopySource,
  type CloudEdit,
  type CloudEditView_Serialize,
  type CloudInvError,
  type CloudLiveEvent,
  type CloudLogPage,
  type CloudLogRange,
  type CloudMovementError,
  type CloudMovePlan,
  type CloudMoveResult_Serialize,
  type CloudPageView_Serialize,
  type CloudReplyTry,
  type CloudRevealOutcome,
  type CloudSaveOutcome_Serialize,
  type CmdError,
  commands,
  type CloudPage as SitePage,
} from "@/bindings";
import { CommandError } from "@/lib/api/tauri";
import { isTauri } from "@/lib/platform";
import { mockCloudApi } from "./mock";
import type { MoveOp } from "./movement/model";

export type CloudAccount = CloudAccount_Serialize;
export type CloudPageData = CloudPageView_Serialize;
export type CloudOverview = CloudPageData["overview"];
export type CloudEditData = CloudEditView_Serialize;
export type CloudSaveResult = CloudSaveOutcome_Serialize;
export type CloudMoveData = CloudMoveResult_Serialize;
export type CloudCopyResult = CloudCopyResult_Serialize;
export type {
  CloudConsoleFile,
  CloudCopySecret,
  CloudCopySource,
  CloudEdit,
  CloudInvError,
  CloudLiveEvent,
  CloudLogPage,
  CloudLogRange,
  CloudMovementError,
  CloudMovePlan,
  CloudReplyTry,
  CloudRevealOutcome,
  SitePage,
};

/** A rule as "Dene" reads it. */
export interface TryRule {
  pattern: string;
  regex: boolean;
  from: string;
  reply: string;
}

export interface CloudApi {
  /** The gate and the player's accounts (`force`: "Tekrar dene", the gate read again now). */
  overview(force?: boolean): Promise<CloudPageData>;
  start(id: string): Promise<CloudAccount>;
  stop(id: string): Promise<CloudAccount>;
  /** "Yeniden bağlan" (never repeated by itself). */
  restart(id: string): Promise<CloudAccount>;
  /** A console line; a password command is sealed before it leaves the launcher. No echo. */
  send(id: string, line: string): Promise<void>;
  /** The masked log before `before` (Unix seconds; null: the last hour). */
  logs(id: string, before: number | null): Promise<CloudLogPage>;
  /** The console is on screen: its lines and state stream through `onLive`. */
  watch(id: string): Promise<void>;
  unwatch(id: string): Promise<void>;
  onLive(listener: (event: CloudLiveEvent) => void): () => void;
  /** The settings sheet's account, read now (secret rows with this computer's copies). */
  edit(id: string): Promise<CloudEditData>;
  save(id: string, edit: CloudEdit): Promise<CloudSaveResult>;
  /** "Göster": one stored secret row's words, from the machine; never kept. */
  reveal(id: string, rowId: string): Promise<CloudRevealOutcome>;
  /** "Buluta taşı": the desktop account in the cloud's form. */
  movePlan(desktopId: string): Promise<CloudMovePlan>;
  move(desktopId: string, cloudId: string, rev: number, run: boolean): Promise<CloudMoveData>;
  /** "Tümünü bağla" leaving out the accounts that run in the cloud; how many were left out. */
  connectHere(): Promise<number>;
  /** Opens a page of afk.projects.gg in the browser. */
  open(page: SitePage, account?: string): Promise<void>;
  // --- the cloud features (CAFK-FEAT), each used only while the service offers it ---
  /** An inventory view is on screen: the account's inventory streams through `onLive`. */
  inventoryWatch(id: string): Promise<void>;
  inventoryUnwatch(id: string): Promise<void>;
  /** A move; null: sent (its answer comes as `invResult`), else why it could not go out. */
  invOp(id: string, opId: string, op: InvOp): Promise<CloudInvError | null>;
  /** "Aç": the inventory view on for this account (from its next connection). */
  setInventory(id: string, enabled: boolean): Promise<CloudSaveResult>;
  /** The "Hareket" panel is on screen: the account's movement streams through `onLive`. */
  movementWatch(id: string): Promise<void>;
  movementUnwatch(id: string): Promise<void>;
  /** A movement operation; null: sent (its answer comes as `movementResult`), else why not. */
  movementOp(id: string, opId: string, op: MoveOp): Promise<CloudMovementError | null>;
  /** "Dene": what the rule answers to a sample message, on this computer. */
  tryReply(rule: TryRule, sample: string, sender: string): Promise<CloudReplyTry>;
  /** "İndir": the console of a range as a file in Downloads; null: no line in that range. */
  download(id: string, name: string, range: CloudLogRange): Promise<CloudConsoleFile | null>;
  showDownload(fileName: string): Promise<void>;
  /** "Ayarları kopyala": the source's secret rows to type first. */
  copySource(id: string): Promise<CloudCopySource>;
  copySettings(
    source: string,
    targets: string[],
    withServer: boolean,
    secrets: CloudCopySecret[],
  ): Promise<CloudCopyResult[]>;
}

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new CommandError(result.error);
  return result.data;
}

const tauriCloudApi: CloudApi = {
  overview: (force = false) => ok(commands.cloudAfkOverview(force)),
  start: (id) => ok(commands.cloudAfkStart(id)),
  stop: (id) => ok(commands.cloudAfkStop(id)),
  restart: (id) => ok(commands.cloudAfkRestart(id)),
  send: async (id, line) => {
    await ok(commands.cloudAfkSend(id, line));
  },
  logs: (id, before) => ok(commands.cloudAfkLogs(id, before)),
  watch: async (id) => {
    await ok(commands.cloudAfkWatch(id));
  },
  unwatch: (id) => commands.cloudAfkUnwatch(id),
  onLive(listener) {
    const unlisten = listen<CloudLiveEvent>("cloud-afk-live", (event) => listener(event.payload));
    return () => {
      void unlisten.then((stop) => stop());
    };
  },
  edit: (id) => ok(commands.cloudAfkEdit(id)),
  save: (id, edit) => ok(commands.cloudAfkSave(id, edit)),
  reveal: (id, rowId) => ok(commands.cloudAfkReveal(id, rowId)),
  movePlan: (desktopId) => ok(commands.cloudAfkMovePlan(desktopId)),
  move: (desktopId, cloudId, rev, run) => ok(commands.cloudAfkMove(desktopId, cloudId, rev, run)),
  connectHere: () => commands.cloudAfkConnectHere(),
  open: async (page, account) => {
    await ok(commands.cloudAfkOpen(page, account ?? null));
  },
  inventoryWatch: async (id) => {
    await ok(commands.cloudAfkInventoryWatch(id));
  },
  inventoryUnwatch: (id) => commands.cloudAfkInventoryUnwatch(id),
  invOp: (id, opId, op) => commands.cloudAfkInvOp(id, opId, JSON.stringify(op)),
  setInventory: (id, enabled) => ok(commands.cloudAfkSetInventory(id, enabled)),
  movementWatch: async (id) => {
    await ok(commands.cloudAfkMovementWatch(id));
  },
  movementUnwatch: (id) => commands.cloudAfkMovementUnwatch(id),
  movementOp: (id, opId, op) => commands.cloudAfkMovementOp(id, opId, JSON.stringify(op)),
  tryReply: (rule, sample, sender) =>
    commands.cloudAfkTryReply(rule.pattern, rule.regex, rule.from, rule.reply, sample, sender),
  download: (id, name, range) => ok(commands.cloudAfkConsoleDownload(id, name, range)),
  showDownload: async (fileName) => {
    await ok(commands.cloudAfkShowDownload(fileName));
  },
  copySource: (id) => ok(commands.cloudAfkCopySource(id)),
  copySettings: (source, targets, withServer, secrets) =>
    ok(commands.cloudAfkCopySettings(source, targets, withServer, secrets)),
};

export const cloudApi: CloudApi = isTauri() ? tauriCloudApi : mockCloudApi;
