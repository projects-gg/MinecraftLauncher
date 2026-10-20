// Settings → Veri → Temizlik: old version sets, game files no instance uses, leftovers (caches,
// old logs, crash reports) and the launcher trash. UI-facing shapes; the Tauri adapter
// (cleanup-tauri.ts) maps the generated bindings onto them.
import type { TrashKind } from "./library-types";
import type { LoaderKind } from "./types";

/** Why cleaning is not possible right now (the lists still show). */
export type CleanupBlock = "gameRunning" | "preparing" | "busy" | "dataJob";

export interface CleanupStatus {
  blocked?: CleanupBlock;
  /** Free space on the data folder's disk. */
  freeBytes?: number;
  /** Size of that disk. */
  diskBytes?: number;
}

/** A version set an instance has not played for a long time. */
export interface OldSet {
  /** `<instance id>/<minecraft>`, unique in the list. */
  key: string;
  instanceId: string;
  instanceName: string;
  minecraft: string;
  loader?: LoaderKind;
  /** Last played (else parked or prepared), ISO time. */
  lastUsedAt?: string;
  sizeBytes: number;
}

export interface OldSets {
  /** Longest unused first. */
  sets: OldSet[];
  totalBytes: number;
  olderThanDays: number;
}

export interface RemovedSet {
  instanceId: string;
  minecraft: string;
  /** Launcher trash item ("Geri al"). */
  trashId: string;
  sizeBytes: number;
}

export interface RemovedSets {
  removed: RemovedSet[];
  /** Bytes moved into the trash (freed when the trash empties). */
  movedBytes: number;
  /** Sets left alone: played meanwhile, active now or in use. */
  skipped: number;
}

export interface RestoredSets {
  /** Back where they were (and not offered as old sets for a while). */
  restored: number;
  /** Left in the trash: that version has a new set now. */
  taken: number;
  /** Gone from the trash, instance deleted, or a file error. */
  failed: number;
}

/** A version set in the launcher trash. */
export interface TrashedSet {
  trashId: string;
  instanceId: string;
  instanceName: string;
  minecraft: string;
  /** ISO time. */
  deletedAt?: string;
  sizeBytes: number;
}

/** An instance deleted from the library, in the launcher trash ("Geri al": `restoreInstance`). */
export interface TrashedInstance {
  trashId: string;
  name: string;
  /** ISO time. */
  deletedAt?: string;
  sizeBytes: number;
}

export type SharedKind = "gameVersion" | "loaderProfile" | "javaRuntime" | "libraries" | "assets";

/** Something in the shared game folders no instance (or parked set) uses. */
export interface SharedItem {
  /** `version:<id>`, `runtime:<component>`, `libraries` or `assets`. */
  id: string;
  kind: SharedKind;
  /** Version id or runtime component (never shown as text). */
  name: string;
  minecraft?: string;
  loader?: LoaderKind;
  loaderVersion?: string;
  javaMajor?: number;
  /** Freed when only this item is deleted. */
  bytes: number;
  files: number;
}

/** Bytes freed only when every listed item is deleted (files hard-linked into each). */
export interface SharedGroup {
  items: string[];
  bytes: number;
}

export interface SharedFiles {
  items: SharedItem[];
  groups: SharedGroup[];
  totalBytes: number;
}

export type LeftoverKind = "cache" | "downloads" | "logs" | "crashReports";

export interface Leftover {
  kind: LeftoverKind;
  bytes: number;
  files: number;
}

export interface CleanupResult {
  freedBytes: number;
  removedFiles: number;
}

/**
 * Anything else in the launcher trash: a world, a backup, a screenshot, the folders "Yeniden
 * yükle" set aside, or a version set whose instance is itself in the trash.
 */
export interface TrashedFile {
  trashId: string;
  instanceId: string;
  /** The instance it came from (also when that instance is in the trash too). */
  instanceName?: string;
  kind: TrashKind;
  /** A world's own name; a file name for the rest (`trashLabel` words them). */
  name: string;
  fileName: string;
  /** ISO time. */
  deletedAt?: string;
  sizeBytes: number;
  /** "Geri al" can put it back from here (`restoreTrashed`). */
  restorable: boolean;
}

export interface TrashSummary {
  /** Everything listed (sets, instances and files). */
  items: number;
  bytes: number;
  /** Over this the oldest items go first. */
  capBytes: number;
  /** When the oldest item goes by itself. */
  nextExpiryAt?: string;
  /** Version sets in the trash, newest first. */
  sets: TrashedSet[];
  /** Deleted instances, newest first (their pages are gone, so they are listed here). */
  instances: TrashedInstance[];
  /** Everything else, newest first. */
  files: TrashedFile[];
}

export interface CleanupSummary {
  /** Old sets + unused game files + leftovers. */
  reclaimableBytes: number;
  freeBytes?: number;
}

export interface CleanupProgress {
  done: number;
  total: number;
}

/** `api.cleanup`: everything the Temizlik card asks of the backend. */
export interface CleanupApi {
  status(): Promise<CleanupStatus>;
  /**
   * Calls `listener` when the status may have changed (a game started or ended, an install,
   * backup, Java download or data job began or ended). Returns the unsubscribe.
   */
  onStatusChanged(listener: () => void): () => void;
  oldSets(olderThanDays: number): Promise<OldSets>;
  /** Moves the sets into the launcher trash (each one checked again first). */
  removeSets(
    sets: Array<{ instanceId: string; minecraft: string }>,
    olderThanDays: number,
  ): Promise<RemovedSets>;
  /** "Geri al": puts sets back from the trash. */
  restoreSets(trashIds: string[]): Promise<RestoredSets>;
  sharedFiles(): Promise<SharedFiles>;
  /** Deletes for good (they download again when needed). */
  deleteShared(
    ids: string[],
    onProgress: (progress: CleanupProgress) => void,
  ): Promise<CleanupResult>;
  leftovers(): Promise<Leftover[]>;
  cleanLeftovers(kinds: LeftoverKind[]): Promise<CleanupResult>;
  trash(): Promise<TrashSummary>;
  emptyTrash(): Promise<CleanupResult>;
  /** Deletes one item of the trash for good. */
  deleteTrashedSet(trashId: string): Promise<CleanupResult>;
  /** Puts a world, backup or screenshot back into its instance; returns its file name. */
  restoreTrashed(instanceId: string, trashId: string): Promise<string>;
  /** For the occasional reminder. */
  summary(olderThanDays: number): Promise<CleanupSummary>;
}
