// Tauri implementation of `api.importer`.
import { Channel } from "@tauri-apps/api/core";
import { type CmdError, commands, type ImportProgress } from "@/bindings";
import type { ImportApi } from "./import-types";

/** A failed import command: friendly `message`, plus `code`/`details` for "copy details". */
export class ImportCommandError extends Error {
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
  if (result.status === "error") throw new ImportCommandError(result.error);
  return result.data;
}

export const tauriImport: ImportApi = {
  scan: () => ok(commands.importScan()),
  measure: (sourceId) => ok(commands.importMeasure(sourceId)),
  async start(request, jobId, onProgress) {
    const channel = new Channel<ImportProgress>();
    channel.onmessage = onProgress;
    return ok(commands.importStart(request, jobId, channel));
  },
  async cancel(jobId) {
    await commands.contentCancel(jobId);
  },
};
