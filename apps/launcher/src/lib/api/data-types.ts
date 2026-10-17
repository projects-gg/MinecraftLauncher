// Settings → Veri: the data folder, its caches and logs, support bundles and moving the folder.
// UI-facing shapes; the Tauri adapter (data-tauri.ts) maps the generated bindings onto them.

export interface DataOverview {
  /** The data folder. */
  root: string;
  cacheBytes: number;
  logsBytes: number;
  /** Free space on the data folder's disk; unknown when missing. */
  freeBytes?: number;
  /** False when the launch environment pins the folder (then "Taşı" is off). */
  movable: boolean;
  /** A moved folder that was missing at start (unplugged disk); the default one is in use. */
  unreachable?: string;
}

export type BundleFolder = "downloads" | "desktop" | "home";

export interface SupportBundle {
  /** Full path of the zip (for "Klasörde göster"). */
  path: string;
  fileName: string;
  folder: BundleFolder;
}

export interface MovePlan {
  from: string;
  /** The folder the data goes into: a `ProjectsLauncher` folder inside the picked one. */
  target: string;
  totalBytes: number;
  fileCount: number;
  freeBytes?: number;
  enoughSpace: boolean;
}

export type MoveStage = "scanning" | "copying" | "verifying";

export interface MoveProgress {
  stage: MoveStage;
  doneBytes: number;
  totalBytes: number;
  doneFiles: number;
  totalFiles: number;
}

/** `api.data`: everything the Data section asks of the backend. */
export interface DataApi {
  overview(): Promise<DataOverview>;
  openFolder(): Promise<void>;
  /** A masked zip of logs, crash reports and system facts in Downloads (else Desktop). */
  createSupportBundle(): Promise<SupportBundle>;
  /** Shows a file selected in the system file manager. */
  revealFile(path: string): Promise<void>;
  /** Native folder picker; `null` when the player closes it. */
  pickFolder(): Promise<string | null>;
  /** Checks the picked folder and estimates the move. Nothing is written yet. */
  planMove(picked: string): Promise<MovePlan>;
  /** Copies, verifies and switches; the launcher restarts shortly after this resolves. */
  move(picked: string, onProgress: (progress: MoveProgress) => void): Promise<void>;
  cancelMove(): Promise<void>;
}
