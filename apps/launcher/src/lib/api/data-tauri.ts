// Tauri implementation of `api.data`: maps the generated bindings onto the UI shapes.
import { Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { type CmdError, commands, type MoveProgressView } from "@/bindings";
import type { BundleFolder, DataApi, MoveStage } from "./data-types";

/** A failed data command: friendly `message`, plus `code`/`details` for "copy details". */
export class DataCommandError extends Error {
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
  if (result.status === "error") throw new DataCommandError(result.error);
  return result.data;
}

export const tauriData: DataApi = {
  async overview() {
    const o = await ok(commands.dataOverview());
    return {
      root: o.root,
      cacheBytes: o.cacheBytes,
      logsBytes: o.logsBytes,
      freeBytes: o.freeBytes ?? undefined,
      movable: o.movable,
      unreachable: o.unreachable ?? undefined,
    };
  },
  async openFolder() {
    await ok(commands.dataOpenFolder());
  },
  async createSupportBundle() {
    const b = await ok(commands.dataSupportBundle());
    return { path: b.path, fileName: b.fileName, folder: b.folder as BundleFolder };
  },
  revealFile: (path) => revealItemInDir(path),
  async pickFolder() {
    const chosen = await open({ directory: true, multiple: false });
    return typeof chosen === "string" ? chosen : null;
  },
  async planMove(picked) {
    const p = await ok(commands.dataMovePlan(picked));
    return { ...p, freeBytes: p.freeBytes ?? undefined };
  },
  async move(picked, onProgress) {
    const channel = new Channel<MoveProgressView>();
    channel.onmessage = (p) => onProgress({ ...p, stage: p.stage as MoveStage });
    await ok(commands.dataMoveStart(picked, channel));
  },
  async cancelMove() {
    await commands.dataMoveCancel();
  },
};
