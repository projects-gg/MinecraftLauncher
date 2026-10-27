// Instance page tabs (logs, worlds, screenshots, servers) and library groups. UI-facing shapes;
// the Tauri adapter (library-tauri.ts) maps the generated bindings onto them.
import type { SavedServer, ServerStatus } from "./types";

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error" | "fatal";

/** One parsed log entry. A stack trace stays inside `message` (several lines). */
export interface LogLine {
  /** Local wall-clock time `HH:MM:SS`, when known. */
  time?: string;
  level: LogLevel;
  thread?: string;
  logger?: string;
  message: string;
}

/**
 * Where a log comes from: a session the launcher captured, or one of the game's own files
 * (`latest.log`, `debug.log` and their `.log.gz` archives).
 */
export type LogSource = "launcher" | "game";

/** One log of an instance. */
export interface GameLogSession {
  fileName: string;
  source: LogSource;
  /** ISO time the session started (for the game's dated archives, the day), when known. */
  startedAt?: string;
  /** ISO time of the last write. */
  updatedAt?: string;
  sizeBytes: number;
  /** The running game writes this log right now (captured sessions only). */
  live: boolean;
}

export interface GameLogContent {
  /** Secrets (access tokens and similar) are already masked. */
  lines: LogLine[];
  /** Only the end of a very large log was read. */
  truncated: boolean;
  sizeBytes: number;
}

export interface SharedLog {
  id: string;
  url: string;
}

export type WorldMode = "survival" | "creative" | "adventure" | "spectator" | "unknown";

export interface WorldDetail {
  folder: string;
  name: string;
  gameMode: WorldMode;
  hardcore: boolean;
  cheats: boolean;
  lastPlayedAt?: string;
  sizeBytes: number;
  iconUrl?: string;
  /** Game version that last saved the world. */
  version?: string;
  /** False when the world's level.dat could not be read. */
  readable: boolean;
}

/** Why the launcher made a backup by itself: before the world first opened to friends (HOST1). */
export type BackupReason = "beforeSharing";

export interface WorldBackup {
  fileName: string;
  sizeBytes: number;
  createdAt?: string;
  /** Folder name of the world it was made from. */
  worldFolder?: string;
  worldName?: string;
  /** False when the archive does not hold a world. */
  valid: boolean;
  /** Set on the launcher's own backups ("Arkadaşlarına açılmadan önce"). */
  reason?: BackupReason;
}

export interface BackupProgress {
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number;
}

/**
 * `modSet`: a version set's `mods/` or `config/`, set aside by "Yeniden yükle"; `instance`: an
 * instance deleted from the library.
 */
export type TrashKind = "world" | "backup" | "screenshot" | "modSet" | "versionSet" | "instance";

/** Something in the launcher's trash (kept for 30 days). */
export interface TrashItem {
  id: string;
  kind: TrashKind;
  name: string;
  fileName: string;
  deletedAt: string;
  expiresAt: string;
  sizeBytes: number;
}

export interface RestoredWorld {
  folder: string;
  /** The world it replaced, now in the trash. */
  replaced?: TrashItem;
}

export interface Screenshot {
  fileName: string;
  /** Loadable image URL. */
  url: string;
  /** Small preview for the gallery, when it was made already (else `screenshotThumb`). */
  thumbUrl?: string;
  sizeBytes: number;
  takenAt?: string;
}

/** What reached the clipboard: the picture, or only its file path. */
export type ClipboardResult = "image" | "path";

/** A server exactly as `servers.dat` keeps it (handed back by a removal for "Geri al"). */
export interface SavedServerEntry {
  name: string;
  ip: string;
  icon: string | null;
  acceptTextures: boolean | null;
  hidden: boolean;
}

/** One edit of an instance's server list; positions count the listed servers. */
export type ServerEdit =
  | { kind: "add"; name: string; address: string }
  | { kind: "update"; index: number; was: string; name: string; address: string }
  | { kind: "remove"; index: number; was: string }
  | { kind: "restore"; index: number; server: SavedServerEntry };

export interface ServerEditResult {
  /** The listed servers after the edit. */
  servers: SavedServer[];
  /** What a removal took off, for "Geri al". */
  removed?: SavedServerEntry;
}

/** A graphics preset of options.txt: Akıcı, Dengeli, Güzel. */
export type GraphicsPreset = "smooth" | "balanced" | "quality";

/** What an instance's options.txt says about graphics. */
export type GraphicsState =
  | { kind: "preset"; preset: GraphicsPreset }
  /** Values of its own ("Özel"): changed in the game, a modpack's or the game's defaults. */
  | { kind: "custom" }
  /** No options.txt yet: the first launch writes this preset (picked, or the computer's). */
  | { kind: "firstLaunch"; preset: GraphicsPreset }
  /** A Minecraft version presets do not cover (before 1.7.2). */
  | { kind: "unsupported" };

/** `api.library`: everything the logs, worlds and screenshots tabs ask of the backend. */
export interface LibraryApi {
  /** Captured sessions first, then the game's own log files; each part newest first. */
  logSessions(instanceId: string): Promise<GameLogSession[]>;
  readLog(instanceId: string, fileName: string, source?: LogSource): Promise<GameLogContent>;
  /** Uploads the masked log to mclo.gs (public). Ask the player first. */
  shareLog(instanceId: string, fileName: string, source?: LogSource): Promise<SharedLog>;
  /** Shows the log file in the file manager (its logs folder without a file). */
  revealLog(instanceId: string, fileName?: string, source?: LogSource): Promise<void>;
  /** Live lines of the running game. Returns an unsubscribe function. */
  onGameLog(listener: (instanceId: string, line: LogLine) => void): () => void;
  /** The instances whose games run (PLAY-AGAIN: several may), each once. */
  runningInstances(): Promise<string[]>;

  worlds(instanceId: string): Promise<WorldDetail[]>;
  backups(instanceId: string): Promise<WorldBackup[]>;
  backupWorld(
    instanceId: string,
    folder: string,
    jobId: string,
    onProgress: (p: BackupProgress) => void,
  ): Promise<WorldBackup>;
  restoreBackup(
    instanceId: string,
    fileName: string,
    replace: boolean,
    jobId: string,
    onProgress: (p: BackupProgress) => void,
  ): Promise<RestoredWorld>;
  cancelJob(jobId: string): Promise<void>;
  deleteWorld(instanceId: string, folder: string): Promise<TrashItem>;
  deleteBackup(instanceId: string, fileName: string): Promise<TrashItem>;
  trash(instanceId: string): Promise<TrashItem[]>;
  /** Puts an item back; returns its (possibly numbered) file or folder name. */
  restoreFromTrash(instanceId: string, id: string): Promise<string>;
  deleteFromTrash(instanceId: string, id: string): Promise<void>;

  screenshots(instanceId: string): Promise<Screenshot[]>;
  /** URL of the picture's thumbnail, made now when missing; `null` when it cannot be made. */
  screenshotThumb(instanceId: string, fileName: string): Promise<string | null>;
  copyScreenshot(instanceId: string, fileName: string): Promise<ClipboardResult>;
  /** Opens the picture in the system's image viewer. */
  openScreenshot(instanceId: string, fileName: string): Promise<void>;
  deleteScreenshot(instanceId: string, fileName: string): Promise<TrashItem>;
  /** Shows a file of the instance (`screenshots`, `backups`) in the file manager. */
  revealFile(
    instanceId: string,
    folder: "screenshots" | "backups",
    fileName: string,
  ): Promise<void>;

  /** Library group of an instance; `null` removes it from its group. */
  setGroup(instanceId: string, group: string | null): Promise<void>;
  /**
   * Bytes the whole instance folder takes (every version set, worlds, backups, screenshots,
   * settings; never the game files all instances share). Measured off the UI thread and answered
   * from the backend's cache while the folder's top levels are unchanged; aborting `signal` stops
   * a walk still running.
   */
  diskUsage(instanceId: string, signal?: AbortSignal): Promise<number>;

  /** Server List Ping of one address; never fails (an unreachable server is `online: false`). */
  pingServer(address: string): Promise<ServerStatus>;
  /** Adds, changes, removes or puts back a server of the instance's list (not while it runs). */
  editServers(instanceId: string, edit: ServerEdit): Promise<ServerEditResult>;

  /** The graphics preset of the instance's options.txt. */
  graphics(instanceId: string): Promise<GraphicsState>;
  /** Writes the preset the player picked (not while the instance's game runs). */
  setGraphics(instanceId: string, preset: GraphicsPreset): Promise<GraphicsState>;
  /**
   * `setGraphics` that applies at once (Settings → Grafik ve FPS): options.txt is copied first,
   * so `undoGraphics` brings back what was there, the player's own values included.
   */
  switchGraphics(instanceId: string, preset: GraphicsPreset): Promise<GraphicsSwitch>;
  /** "Geri al" after `switchGraphics`; `null` when its copy is gone (nothing changed). */
  undoGraphics(instanceId: string, backup: string): Promise<GraphicsState | null>;
  /**
   * The shader pack Iris draws in the instance's game, on or off; `null` when there is none to
   * draw (no Iris among its mods, no pack chosen, the pack gone).
   */
  shader(instanceId: string): Promise<ShaderState | null>;
  /**
   * Turns that shader on or off for the game's next start (only Iris's `enableShaders` changes,
   * so "Geri al" is the opposite call). Refused while the instance's game runs; `null` when there
   * is no shader to turn.
   */
  setShader(instanceId: string, on: boolean): Promise<ShaderState | null>;
}

/** The shader pack Iris draws in an instance (Settings → Grafik ve FPS). */
export interface ShaderState {
  /** The pack's name: its Modrinth title, else its file name without `.zip`. */
  name: string;
  /** Iris draws it; off, it waits to be turned back on. */
  on: boolean;
}

/** What `switchGraphics` did. */
export interface GraphicsSwitch {
  state: GraphicsState;
  /** The copy of options.txt from before; missing without a file yet or when nothing changed. */
  backup?: string;
}
