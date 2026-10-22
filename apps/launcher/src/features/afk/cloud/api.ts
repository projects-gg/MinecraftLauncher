// 7/24 Bulut: what the AFK page asks of the backend (commands/cloud_afk.rs over pl-cloud-afk).
// Nothing polls: the page reads the overview when it opens and after an action, and the console's
// live socket runs only while a console is watched. Browser previews use the mock (mock.ts).
import { listen } from "@tauri-apps/api/event";
import {
  type CloudAccount_Serialize,
  type CloudEdit,
  type CloudEditView_Serialize,
  type CloudLiveEvent,
  type CloudLogPage,
  type CloudMovePlan,
  type CloudMoveResult_Serialize,
  type CloudPageView_Serialize,
  type CloudRevealOutcome,
  type CloudSaveOutcome_Serialize,
  type CmdError,
  commands,
  type CloudPage as SitePage,
} from "@/bindings";
import { CommandError } from "@/lib/api/tauri";
import { isTauri } from "@/lib/platform";
import { mockCloudApi } from "./mock";

export type CloudAccount = CloudAccount_Serialize;
export type CloudPageData = CloudPageView_Serialize;
export type CloudOverview = CloudPageData["overview"];
export type CloudEditData = CloudEditView_Serialize;
export type CloudSaveResult = CloudSaveOutcome_Serialize;
export type CloudMoveData = CloudMoveResult_Serialize;
export type {
  CloudEdit,
  CloudLiveEvent,
  CloudLogPage,
  CloudMovePlan,
  CloudRevealOutcome,
  SitePage,
};

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
};

export const cloudApi: CloudApi = isTauri() ? tauriCloudApi : mockCloudApi;
