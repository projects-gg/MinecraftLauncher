// Mod center and per-version sets. UI-facing shapes; the Tauri adapter (modcenter-tauri.ts)
// maps the generated bindings onto them.
import type { JobProgress } from "./content-types";
import type { LoaderKind } from "./types";

export type ModCategory = "performance" | "visual" | "interface" | "gameplay" | "library" | "other";

/** A change the player made to a recommended mod outside the launcher, which it adopted. */
export type ModChange = "removed" | "disabled" | "enabled" | "replaced";

/** One recommended mod of the official pack. */
export interface ModCenterItem {
  id: string;
  title: string;
  summary?: string;
  category: ModCategory;
  iconUrl?: string;
  /** The mod's official page (https only), opened in the browser ("Mod sayfası"). */
  pageUrl?: string;
  /** Modrinth project (opens the project page). */
  projectId?: string;
  fileName: string;
  /** `mods`, `resourcepacks`, `shaderpacks`, … */
  folder: string;
  /** Installed only because other mods need it; never a choice. */
  library: boolean;
  defaultEnabled: boolean;
  /** Ids of the mods installed together with this one. */
  requires: string[];
  /** The player's choice. */
  selected: boolean;
  /** Installed (selected, or needed by an enabled mod). */
  enabled: boolean;
  /** "Yeni": added after the player's set was made, not confirmed yet. */
  isNew: boolean;
  /** Ids of enabled mods that need this one. */
  requiredBy: string[];
  /** The launcher's in-game overlay needs it (Fabric API while the switch is on). */
  launcherNeeds?: boolean;
  installed: boolean;
  /** "Güncelleme var". */
  updateAvailable: boolean;
  sizeBytes: number;
  manual?: ModChange;
  /** The player replaced the file; it stays until "Önerilene döndür". */
  modified: boolean;
  /** Off and kept as a disabled copy. */
  disabledCopy: boolean;
}

export type ModIssueKind = "unreadable" | "notAFile" | "busy";

/** A file the launcher could not read or change; it is left as it is. */
export interface ModIssue {
  /** "Copy details" only. */
  fileName: string;
  folder: string;
  /** The recommended item the file belongs to (the UI names it by this, never by the file). */
  title?: string;
  kind: ModIssueKind;
  /** Technical details for "copy details" only. */
  detail: string;
}

export interface ModCenterView {
  instanceId: string;
  mcVersion: string;
  loader: LoaderKind;
  /** Follows the official pack, so it has recommended mods. */
  managed: boolean;
  pack?: {
    id: string;
    name: string;
    version: string;
    installedVersion?: string;
    minecraft: string;
  };
  items: ModCenterItem[];
  /** The mods folder looked wiped; the next sync installs the selection again. */
  reset: boolean;
  issues: ModIssue[];
  /** The pack no longer offers this Minecraft version. */
  dropped?: { minecraft: string; recommended?: string };
  /** Recommended mods could not be loaded (no network and nothing cached). */
  unavailable: boolean;
  gameRunning: boolean;
}

export interface ModSelection {
  /** Item id → on/off. Items not listed keep their choice. */
  items: Record<string, boolean>;
  /** Items whose replaced file goes back to the pack's ("Önerilene döndür"). */
  revert?: string[];
}

export interface CategoryCounts {
  mods: number;
  resourcePacks: number;
  shaderPacks: number;
  configs: number;
  other: number;
}

export interface SyncSummary {
  added: CategoryCounts;
  updated: CategoryCounts;
  removed: CategoryCounts;
  kept: number;
  unchanged: number;
  downloadBytes: number;
  removeBytes: number;
}

export interface ModCenterPreview {
  items: ModCenterItem[];
  summary: SyncSummary;
  changesFiles: boolean;
  /** Applying records something even without file changes (new mods seen). */
  recordsChoices: boolean;
}

export interface ModCenterApplyResult {
  downloaded: number;
  restored: number;
  removed: number;
  /** Files that could not be changed (they stay; the next sync tries again). */
  issues: number;
}

/** Shown after a launch when the launcher adopted changes from the mods folder. */
export interface ModsNotice {
  changes: Array<{ itemId: string; title: string; change: ModChange; requiredBy: string[] }>;
  /** The mods folder was empty and the selection was installed again. */
  reset: boolean;
  /** Files that could not be read or changed. */
  issues: number;
}

export interface InstanceVersion {
  minecraft: string;
  current: boolean;
  recommended: boolean;
  /** The instance can move to it. */
  available: boolean;
  /** A set is kept for it: its mods and settings come back exactly. */
  hasSet: boolean;
  /** Mods were moved into it ahead of the first visit. */
  prepared: boolean;
  /** A snapshot (development) version. */
  snapshot: boolean;
  lastPlayedAt?: string;
  sizeBytes: number;
}

export interface InstanceVersions {
  current: string;
  managed: boolean;
  loader: LoaderKind;
  /** Newest first. */
  options: InstanceVersion[];
  /** Only part of the list could be loaded (offline). */
  partial: boolean;
  gameRunning: boolean;
}

export interface SwitchPreview {
  from: string;
  to: string;
  firstVisit: boolean;
  parkedMods: number;
  parkedBytes: number;
  hasConfig: boolean;
  restoredMods: number;
  restoredLastPlayedAt?: string;
  downloadFiles: number;
  downloadBytes: number;
  carriedChoices: number;
  newItems: number;
  ownMods: number;
  /** Minecraft is not downloaded yet: it comes with the first launch. */
  gameDownload: boolean;
  loader: LoaderKind;
  loaderVersion?: string;
  /** `to` has no set but cleanup moved its old one to the trash ("Eski seti geri getir"). */
  trashedSet?: TrashedSetPreview;
}

/** A version's set in the launcher trash, as the switch would bring it back. */
export interface TrashedSetPreview {
  trashId: string;
  mods: number;
  sizeBytes: number;
  lastPlayedAt?: string;
  /** Official pack files bringing it back would download. */
  downloadFiles: number;
  downloadBytes: number;
}

export interface SwitchResult {
  from: string;
  to: string;
  firstVisit: boolean;
  /** Titles of recommended mods that stayed on. */
  carried: string[];
  /** Titles of recommended mods the new version offers ("Yeni"). */
  newItems: string[];
  /** Own mods left in the previous set. */
  ownModsLeft: number;
  /** The pack's files are in place (false: they come with the first launch). */
  synced: boolean;
}

export type MigrationStatus = "compatible" | "alreadyThere" | "noVersion" | "unknown";

export interface MigrationMod {
  fileName: string;
  title: string;
  projectId?: string;
  currentVersion?: string;
  targetVersionId?: string;
  targetVersionNumber?: string;
  sizeBytes: number;
  status: MigrationStatus;
}

export interface MigrationPlan {
  from: string;
  to: string;
  mods: MigrationMod[];
  /** Modrinth could not be reached. */
  offline: boolean;
}

export interface MigrationMove {
  title: string;
  /** The Modrinth version that moves the mod. */
  versionId: string;
}

/** After a first visit: own mods of `from` with a version for `to`, from one Modrinth request. */
export interface MigrationCheck {
  from: string;
  to: string;
  /** Own mods left in `from`'s set. */
  own: number;
  compatible: MigrationMove[];
  /** Own mods the new set already has. */
  alreadyThere: number;
  /** Modrinth could not be reached: the full search stays on demand. */
  offline: boolean;
}

export interface MigrationResult {
  installed: number;
  alreadyInstalled: number;
  /** Required dependencies that could not be found for the target. */
  unresolved: number;
}

/** `api.modCenter`: recommended mods, per-version sets and moving own mods. */
export interface ModCenterApi {
  view(instanceId: string): Promise<ModCenterView>;
  /** Dry run: what applying `selection` would download and remove. */
  preview(instanceId: string, selection: ModSelection): Promise<ModCenterPreview>;
  apply(
    instanceId: string,
    selection: ModSelection,
    jobId: string,
    onProgress: (progress: JobProgress) => void,
  ): Promise<ModCenterApplyResult>;
  /** New mods are no longer flagged; adopted changes are acknowledged. */
  markSeen(instanceId: string): Promise<void>;
  /** Versions to move to; snapshots only when `includeSnapshots` (Settings → Oyun). */
  versions(instanceId: string, includeSnapshots: boolean): Promise<InstanceVersions>;
  switchPreview(instanceId: string, minecraft: string): Promise<SwitchPreview>;
  switchVersion(
    instanceId: string,
    minecraft: string,
    jobId: string,
    onProgress: (progress: JobProgress) => void,
  ): Promise<SwitchResult>;
  /** Puts a version's set back from the trash; the switch that follows then brings it back. */
  restoreTrashedSet(instanceId: string, trashId: string): Promise<void>;
  /** Own mods of `from`'s set (the active one when omitted) with a version for `to`. */
  migrationPlan(instanceId: string, from: string | undefined, to: string): Promise<MigrationPlan>;
  /** Quick form of `migrationPlan` for the page after a first visit (one Modrinth request). */
  migrationCheck(instanceId: string, from: string, to: string): Promise<MigrationCheck>;
  migrate(
    instanceId: string,
    to: string,
    versionIds: string[],
    jobId: string,
    onProgress: (progress: JobProgress) => void,
  ): Promise<MigrationResult>;
}
