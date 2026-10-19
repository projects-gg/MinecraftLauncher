// Ready-made mod sets ("Hazır setler", MODSET1): the catalog the team publishes in Studio and the
// sets an instance has. UI-facing shapes; modsets-tauri.ts maps the generated bindings onto them.
import type { JobProgress } from "./content-types";
import type { ModCenterItem, ModCenterPreview, ModIssue, ModSelection } from "./modcenter-types";
import type { InstanceSummary, LoaderKind } from "./types";

/** What a set is for: its card's mark. */
export type SetPurpose = "pvp" | "performance" | "visual" | "other";

/** Kind of a set item (from the folder its file goes to). */
export type SetItemKind = "mod" | "resourcePack" | "shaderPack";

/** One item of a set, as cards and the add dialog show it. */
export interface SetItem {
  id: string;
  title: string;
  summary?: string;
  kind: SetItemKind;
  iconUrl?: string;
  /** The project's page (https only), opened in the browser. */
  pageUrl?: string;
  projectId?: string;
  defaultEnabled: boolean;
  /** Installed only because another item needs it; never a choice. */
  library: boolean;
  requires: string[];
  sizeBytes: number;
}

/** One Minecraft version (and loader) of a set. */
export interface SetVariant {
  minecraft: string;
  /** Missing: resource and shader packs only, for any loader (vanilla too). */
  loader?: LoaderKind;
  /** The loader version a new instance from the set gets. */
  loaderVersion?: string;
  items: SetItem[];
  /** What adding the set without changes installs (default items and what they need). */
  mods: number;
  resourcePacks: number;
  shaderPacks: number;
  downloadBytes: number;
}

/** A set card. `*En` carry the English wording when the team wrote one. */
export interface SetCard {
  id: string;
  name: string;
  nameEn?: string;
  summary?: string;
  summaryEn?: string;
  purpose: SetPurpose;
  /** Newest Minecraft version first. */
  variants: SetVariant[];
}

export interface SetCatalog {
  sets: SetCard[];
  /** The sets could not be loaded (no network and nothing cached). */
  unavailable: boolean;
}

/** A set an instance has. */
export interface InstanceSet {
  id: string;
  name: string;
  nameEn?: string;
  summary?: string;
  summaryEn?: string;
  purpose: SetPurpose;
  /** Installed for the instance's Minecraft version. */
  installed: boolean;
  /** The catalog has a variant for this instance (needed to install or update it). */
  available: boolean;
  /** The team changed the set's files since it was installed. */
  updateAvailable: boolean;
  /** The catalog no longer has the set: it keeps working and can be removed. */
  retired: boolean;
  /** The set's items under the current choices, like the mod center's. */
  items: ModCenterItem[];
  /** Items the instance already has from elsewhere (the official pack, own mods). */
  present: SetItem[];
  /** Applying now would change files (a file went missing, something it needed went away). */
  pending: boolean;
  issues: ModIssue[];
  /** Bytes of the files the set installed here. */
  sizeBytes: number;
}

export interface InstanceSets {
  instanceId: string;
  mcVersion: string;
  loader: LoaderKind;
  sets: InstanceSet[];
  /** The game of this instance is running: changes wait until it is closed. */
  gameRunning: boolean;
  /** The catalog could not be loaded (the instance's sets still show from its records). */
  unavailable: boolean;
}

export interface SetApplyResult {
  downloaded: number;
  restored: number;
  removed: number;
  /** Files that could not be changed (they stay; the next apply tries again). */
  issues: number;
}

/** The badge of a file a set installed. */
export interface SetBadge {
  id: string;
  name: string;
  purpose: SetPurpose;
}

/** A new instance started from a set. */
export interface NewSetInstance {
  setId: string;
  /** The variant's Minecraft version. */
  minecraft: string;
  name: string;
  icon: string;
  /** Choices made before creating (items not listed keep their default). */
  selection?: ModSelection;
  maxMemoryMb?: number;
  jvmArgs?: string;
}

/** `api.modSets`. */
export interface ModSetsApi {
  /** The ready-made sets this launcher offers. */
  catalog(): Promise<SetCatalog>;
  /** The sets of an instance with their items under the current choices (a dry run). */
  instance(instanceId: string): Promise<InstanceSets>;
  /** What adding a set, applying choices or updating it would do (nothing changes). */
  preview(instanceId: string, setId: string, selection?: ModSelection): Promise<ModCenterPreview>;
  /** Adds, installs, updates or applies choices; refused while the instance's game runs. */
  apply(
    instanceId: string,
    setId: string,
    selection: ModSelection | undefined,
    jobId: string,
    onProgress: (progress: JobProgress) => void,
  ): Promise<SetApplyResult>;
  /** Removes a set: its unchanged files go, the player's changed ones stay. */
  remove(
    instanceId: string,
    setId: string,
    jobId: string,
    onProgress: (progress: JobProgress) => void,
  ): Promise<SetApplyResult>;
  /** New items of a set are no longer flagged. */
  markSeen(instanceId: string, setId: string): Promise<void>;
  /** A new instance with the variant's version and loader, the set installed into it. */
  createInstance(
    request: NewSetInstance,
    jobId: string,
    onProgress: (progress: JobProgress) => void,
  ): Promise<InstanceSummary>;
  cancel(jobId: string): Promise<void>;
}
