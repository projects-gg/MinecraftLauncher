// Tauri implementation of `api.library`: maps the generated bindings onto the UI shapes.
import { Channel, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  type BackupProgress,
  type CmdError,
  commands,
  type GameLogEvent,
  type LogLine as LogLineView,
  type TrashItemView,
  type WorldBackupView,
} from "@/bindings";
import type { LibraryApi, LogLine, TrashItem, WorldBackup, WorldMode } from "./library-types";

/** A failed library command: friendly `message`, plus `code`/`details` for "copy details". */
export class LibraryCommandError extends Error {
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
  if (result.status === "error") throw new LibraryCommandError(result.error);
  return result.data;
}

const line = (l: LogLineView): LogLine => ({
  time: l.time ?? undefined,
  level: l.level,
  thread: l.thread ?? undefined,
  logger: l.logger ?? undefined,
  message: l.message,
});

const trashItem = (t: TrashItemView): TrashItem => ({ ...t });

const backup = (b: WorldBackupView): WorldBackup => ({
  fileName: b.fileName,
  sizeBytes: b.sizeBytes,
  createdAt: b.createdAt ?? undefined,
  worldFolder: b.worldFolder ?? undefined,
  worldName: b.worldName ?? undefined,
  valid: b.valid,
  reason: b.reason ?? undefined,
});

function progressChannel(onProgress: (p: BackupProgress) => void): Channel<BackupProgress> {
  const channel = new Channel<BackupProgress>();
  channel.onmessage = onProgress;
  return channel;
}

export const tauriLibrary: LibraryApi = {
  async logSessions(instanceId) {
    return (await ok(commands.gameLogs(instanceId))).map((s) => ({
      fileName: s.fileName,
      source: s.source,
      startedAt: s.startedAt ?? undefined,
      updatedAt: s.updatedAt ?? undefined,
      sizeBytes: s.sizeBytes,
      live: s.live,
    }));
  },
  async readLog(instanceId, fileName, source) {
    const log = await ok(commands.gameLogRead(instanceId, fileName, source ?? null));
    return { lines: log.lines.map(line), truncated: log.truncated, sizeBytes: log.sizeBytes };
  },
  shareLog: (instanceId, fileName, source) =>
    ok(commands.gameLogShare(instanceId, fileName, source ?? null)),
  async revealLog(instanceId, fileName, source) {
    await ok(commands.gameLogReveal(instanceId, fileName ?? null, source ?? null));
  },
  onGameLog(listener) {
    const unlisten = listen<GameLogEvent>("game-log", (event) =>
      listener(event.payload.instanceId, line(event.payload.line)),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  async runningInstance() {
    return (await commands.gameRunning()) ?? null;
  },

  async worlds(instanceId) {
    return (await ok(commands.worldsList(instanceId))).map((w) => ({
      folder: w.folder,
      name: w.name,
      gameMode: w.gameMode as WorldMode,
      hardcore: w.hardcore,
      cheats: w.cheats,
      lastPlayedAt: w.lastPlayedAt ?? undefined,
      sizeBytes: w.sizeBytes,
      iconUrl: w.iconUrl ?? undefined,
      version: w.version ?? undefined,
      readable: w.readable,
    }));
  },
  async backups(instanceId) {
    return (await ok(commands.worldBackups(instanceId))).map(backup);
  },
  async backupWorld(instanceId, folder, jobId, onProgress) {
    return backup(
      await ok(commands.worldBackup(instanceId, folder, jobId, progressChannel(onProgress))),
    );
  },
  async restoreBackup(instanceId, fileName, replace, jobId, onProgress) {
    const restored = await ok(
      commands.worldRestore(instanceId, fileName, replace, jobId, progressChannel(onProgress)),
    );
    return {
      folder: restored.folder,
      replaced: restored.replaced ? trashItem(restored.replaced) : undefined,
    };
  },
  async cancelJob(jobId) {
    await commands.libraryCancel(jobId);
  },
  async deleteWorld(instanceId, folder) {
    return trashItem(await ok(commands.worldDelete(instanceId, folder)));
  },
  async deleteBackup(instanceId, fileName) {
    return trashItem(await ok(commands.worldBackupDelete(instanceId, fileName)));
  },
  async trash(instanceId) {
    return (await ok(commands.trashList(instanceId))).map(trashItem);
  },
  restoreFromTrash: (instanceId, id) => ok(commands.trashRestore(instanceId, id)),
  async deleteFromTrash(instanceId, id) {
    await ok(commands.trashDelete(instanceId, id));
  },

  async screenshots(instanceId) {
    return (await ok(commands.screenshotsList(instanceId))).map((s) => ({
      fileName: s.fileName,
      url: convertFileSrc(s.path),
      thumbUrl: s.thumb ? convertFileSrc(s.thumb) : undefined,
      sizeBytes: s.sizeBytes,
      takenAt: s.takenAt ?? undefined,
    }));
  },
  async screenshotThumb(instanceId, fileName) {
    const path = await ok(commands.screenshotThumbnail(instanceId, fileName));
    return path ? convertFileSrc(path) : null;
  },
  copyScreenshot: (instanceId, fileName) => ok(commands.screenshotCopy(instanceId, fileName)),
  async openScreenshot(instanceId, fileName) {
    await ok(commands.screenshotOpen(instanceId, fileName));
  },
  async deleteScreenshot(instanceId, fileName) {
    return trashItem(await ok(commands.screenshotDelete(instanceId, fileName)));
  },
  async revealFile(instanceId, folder, fileName) {
    await ok(commands.instanceRevealFile(instanceId, folder, fileName));
  },

  async setGroup(instanceId, group) {
    await ok(commands.instanceSetGroup(instanceId, group));
  },
  async diskUsage(instanceId, signal) {
    // The request's own id: aborting stops exactly this walk, never a newer one.
    const requestId = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const stop = () => void commands.instanceDiskUsageCancel(requestId);
    signal?.addEventListener("abort", stop, { once: true });
    try {
      return (await ok(commands.instanceDiskUsage(instanceId, requestId))).bytes;
    } finally {
      signal?.removeEventListener("abort", stop);
    }
  },

  async pingServer(address) {
    const s = await commands.serverStatus(null, address);
    return {
      name: s.name,
      address: s.address,
      online: s.online,
      players: s.players,
      maxPlayers: s.maxPlayers,
      latencyMs: s.latencyMs ?? undefined,
      iconUrl: s.iconUrl ?? undefined,
    };
  },
  async editServers(instanceId, edit) {
    const result = await ok(commands.instanceServersEdit(instanceId, edit));
    return {
      servers: result.servers.map((s) => ({
        name: s.name,
        address: s.address,
        iconUrl: s.iconUrl ?? undefined,
      })),
      removed: result.removed ?? undefined,
    };
  },

  graphics: (instanceId) => ok(commands.instanceGraphics(instanceId)),
  setGraphics: (instanceId, preset) => ok(commands.instanceSetGraphics(instanceId, preset)),
  async switchGraphics(instanceId, preset) {
    const result = await ok(commands.instanceSwitchGraphics(instanceId, preset));
    return { state: result.state, backup: result.backup ?? undefined };
  },
  undoGraphics: (instanceId, backup) => ok(commands.instanceUndoGraphics(instanceId, backup)),
  shader: (instanceId) => ok(commands.instanceShader(instanceId)),
  setShader: (instanceId, on) => ok(commands.instanceSetShader(instanceId, on)),
};
