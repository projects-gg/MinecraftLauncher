// Modrinth browser, installed add-ons and modpacks. UI-facing shapes; the Tauri adapter
// (content-tauri.ts) maps the generated bindings onto them.
import type { InstanceIcon, InstanceSummary, LoaderKind } from "./types";

/** Folder an add-on lives in: `mods/`, `resourcepacks/`, `shaderpacks/`. */
export type ContentKind = "mod" | "resourcePack" | "shaderPack";
/** Modrinth project types the launcher browses. */
export type ContentProjectType = "mod" | "resourcepack" | "shader" | "modpack" | "datapack";
export type ContentSort = "relevance" | "downloads" | "updated";
export type VersionType = "release" | "beta" | "alpha";

export interface ContentSearchParams {
  projectType: ContentProjectType;
  query: string;
  sort: ContentSort;
  category?: string;
  offset: number;
  limit: number;
  /** Only what runs on this instance (game version, and the loader for mods). */
  instanceId?: string;
}

export interface ContentHit {
  projectId: string;
  slug?: string;
  title: string;
  author: string;
  description: string;
  iconUrl?: string;
  downloads: number;
  follows: number;
  dateModified: string;
  categories: string[];
}

export interface ContentSearchPage {
  hits: ContentHit[];
  offset: number;
  total: number;
}

export interface ContentCategory {
  name: string;
  projectType: ContentProjectType;
  /** `categories`, `features`, `resolutions`, `performance impact`. */
  header: string;
}

export interface ContentLink {
  kind: "source" | "issues" | "wiki" | "discord" | "donate";
  url: string;
}

export interface ContentGalleryImage {
  /** Compressed image for thumbnails. */
  url: string;
  /** The original upload (large), for the image viewer. */
  fullUrl?: string;
  title?: string;
  description?: string;
  featured: boolean;
}

export interface ContentProject {
  id: string;
  slug?: string;
  projectType: ContentProjectType | "other";
  title: string;
  description: string;
  /** Long description (Markdown with some HTML). Render it sanitized. */
  body: string;
  iconUrl?: string;
  downloads: number;
  followers: number;
  published: string;
  updated: string;
  categories: string[];
  gameVersions: string[];
  loaders: string[];
  license?: string;
  links: ContentLink[];
  gallery: ContentGalleryImage[];
}

export interface ContentVersion {
  id: string;
  projectId: string;
  name: string;
  versionNumber: string;
  versionType: VersionType;
  datePublished: string;
  downloads: number;
  gameVersions: string[];
  loaders: string[];
  fileName?: string;
  fileSize?: number;
  requiredDependencies: number;
}

export interface ContentUpdate {
  versionId: string;
  versionNumber: string;
  versionType: VersionType;
  datePublished: string;
}

/** An installed add-on file and the Modrinth project it belongs to (when known). */
export interface InstalledContent {
  fileName: string;
  enabled: boolean;
  sizeBytes: number;
  projectId?: string;
  versionId?: string;
  versionNumber?: string;
  title?: string;
  iconUrl?: string;
  /** Installed by the official pack: updated with the pack, not removable here. */
  fromPack: boolean;
  update?: ContentUpdate;
}

export interface ContentScan {
  entries: InstalledContent[];
  /** Modrinth was unreachable; entries only carry what the launcher knew already. */
  offline: boolean;
}

export interface JobProgress {
  /** resolving | downloading | finishing | pack | extracting | identifying | writing */
  stage: string;
  /** 0–100; missing while the amount of work is unknown. */
  percent?: number;
  doneFiles: number;
  totalFiles: number;
  bytesPerSec: number;
}

export interface InstalledItem {
  projectId: string;
  title?: string;
  versionNumber: string;
  fileName: string;
  dependency: boolean;
}

export interface ContentInstallResult {
  installed: InstalledItem[];
  alreadyInstalled: InstalledItem[];
  replaced: string[];
  /** Required dependencies that could not be installed. */
  unresolved: Array<{ title?: string; fileName?: string }>;
  /** Installed projects a new version declares incompatible. */
  conflicts: Array<{ title?: string; projectId: string }>;
}

export type PackFormat = "mrpack" | "prism" | "curseForge";

export interface PackPreview {
  format: PackFormat;
  name: string;
  packVersion?: string;
  summary?: string;
  author?: string;
  mcVersion: string;
  loader: LoaderKind;
  loaderVersion?: string;
  downloadFiles: number;
  downloadBytes: number;
  bundledFiles: number;
  /** CurseForge mods that have to be downloaded by hand. */
  manualFiles: number;
  unsupported: string[];
  /** `.mrpack` files the pack marks optional (counted in the downloads). */
  optional: PackOptionalFile[];
}

/** A file of an `.mrpack` the player may leave out. */
export interface PackOptionalFile {
  /** Where it goes (`mods/x.jar`); named in `skipOptional` to leave it out. */
  path: string;
  fileName: string;
  /** `mods`, `resourcepacks`, `shaderpacks` or another folder. */
  folder: string;
  size: number;
  projectId?: string;
}

/** The Modrinth modpack an instance was made from. */
export interface ModpackInfo {
  projectId: string;
  versionId: string;
  versionNumber?: string;
  title?: string;
  iconUrl?: string;
}

/** A newer version of an instance's modpack. */
export interface ModpackUpdate {
  versionId: string;
  versionNumber: string;
  name: string;
  publishedAt: string;
}

export interface PackChange {
  path: string;
  /** The Modrinth project's title, else the file name. */
  name: string;
  /** First folder of the path (`mods`, `config`, …; empty at the top). */
  folder: string;
  iconUrl?: string;
}

/** Why a pack file stays as the player left it. */
export type KeptReason = "changed" | "removed" | "disabled" | "theirs";

export interface ModpackUpdatePlan {
  fromVersion?: string;
  toVersion: string;
  added: PackChange[];
  updated: PackChange[];
  removed: PackChange[];
  kept: Array<{ item: PackChange; reason: KeptReason }>;
  downloadBytes: number;
  loaderFrom?: string;
  loaderTo?: string;
}

export interface ModpackUpdateResult {
  versionNumber: string;
  added: number;
  updated: number;
  removed: number;
  kept: number;
}

/** A data pack of a world. */
export interface Datapack {
  fileName: string;
  name: string;
  description?: string;
  sizeBytes: number;
  folder: boolean;
  /** `pack.png` as a data URL. */
  icon?: string;
}

export interface ModpackImportResult {
  instance: InstanceSummary;
  manualMods: Array<{ name: string; url: string }>;
  manualCount: number;
  files: number;
  skipped: number;
  unsupported: string[];
}

export interface ModpackExportOptions {
  path: string;
  name: string;
  version: string;
  summary?: string;
  includeWorlds: boolean;
  includeConfig: boolean;
  includeOptions: boolean;
}

export interface ModpackExportResult {
  path: string;
  hostedFiles: number;
  bundledFiles: number;
  bundledBytes: number;
  /** Modrinth was unreachable: every mod was bundled into the file. */
  offline: boolean;
}

export type FileDropEvent =
  | { type: "over" }
  | { type: "leave" }
  | { type: "drop"; paths: string[] };

export type PickKind = ContentKind | "modpack";

/** Content browser, add-on management and modpacks. `api.content`. */
export interface ContentApi {
  search(params: ContentSearchParams): Promise<ContentSearchPage>;
  categories(): Promise<ContentCategory[]>;
  project(id: string): Promise<ContentProject>;
  versions(projectId: string, instanceId?: string, kind?: ContentKind): Promise<ContentVersion[]>;
  scan(instanceId: string, kind: ContentKind, checkUpdates: boolean): Promise<ContentScan>;
  install(
    job: {
      instanceId: string;
      kind: ContentKind;
      projectId: string;
      versionId?: string;
      jobId: string;
    },
    onProgress: (progress: JobProgress) => void,
  ): Promise<ContentInstallResult>;
  update(
    job: { instanceId: string; kind: ContentKind; fileNames: string[]; jobId: string },
    onProgress: (progress: JobProgress) => void,
  ): Promise<ContentInstallResult>;
  remove(instanceId: string, kind: ContentKind, fileName: string): Promise<void>;
  addFiles(
    instanceId: string,
    kind: ContentKind,
    paths: string[],
  ): Promise<{ added: string[]; skipped: string[] }>;
  cancel(jobId: string): Promise<void>;
  previewPack(path: string): Promise<PackPreview>;
  importPack(
    job: {
      path: string;
      name?: string;
      icon: InstanceIcon;
      jobId: string;
      /** Optional files (their `path`) the player leaves out. */
      skipOptional?: string[];
    },
    onProgress: (progress: JobProgress) => void,
  ): Promise<ModpackImportResult>;
  installModpack(
    job: {
      projectId: string;
      versionId?: string;
      name?: string;
      icon: InstanceIcon;
      jobId: string;
    },
    onProgress: (progress: JobProgress) => void,
  ): Promise<ModpackImportResult>;
  exportPack(
    instanceId: string,
    options: ModpackExportOptions,
    jobId: string,
    onProgress: (progress: JobProgress) => void,
  ): Promise<ModpackExportResult>;
  /** The Modrinth modpack an instance was made from (`null` for other instances). */
  modpackInfo(instanceId: string): Promise<ModpackInfo | null>;
  /** A newer version of the instance's pack (one request), `null` when it is up to date. */
  modpackUpdateCheck(instanceId: string): Promise<ModpackUpdate | null>;
  /** Downloads the new version and tells what updating would change (nothing changes yet). */
  modpackUpdatePlan(
    instanceId: string,
    versionId: string,
    jobId: string,
    onProgress: (progress: JobProgress) => void,
  ): Promise<ModpackUpdatePlan>;
  modpackUpdateApply(
    instanceId: string,
    versionId: string,
    jobId: string,
    onProgress: (progress: JobProgress) => void,
  ): Promise<ModpackUpdateResult>;
  /** Titles and icons of Modrinth projects (one request; unknown ids are left out). */
  projectBriefs(ids: string[]): Promise<Array<{ id: string; title: string; iconUrl?: string }>>;
  datapacks(instanceId: string, world: string): Promise<Datapack[]>;
  installDatapack(
    job: { instanceId: string; world: string; projectId: string; jobId: string },
    onProgress: (progress: JobProgress) => void,
  ): Promise<Datapack>;
  removeDatapack(instanceId: string, world: string, fileName: string): Promise<void>;
  /** Native file picker; `null` when the player closes it. */
  pickFiles(kind: PickKind, multiple: boolean): Promise<string[] | null>;
  /** Native save dialog for an `.mrpack`; `null` when cancelled. */
  pickSavePath(defaultName: string): Promise<string | null>;
  /** Files dragged onto the window. Returns an unsubscribe function. */
  onFileDrop(listener: (event: FileDropEvent) => void): () => void;
}
