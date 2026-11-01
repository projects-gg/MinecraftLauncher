// Bringing a player over from another launcher (IMPORT1): what the first-run step and Settings →
// Veri ask of the backend. The shapes are the generated bindings' own (pl_import).
import type {
  ImportProgress,
  ImportReport,
  ImportRequest,
  ImportScanView,
  SourceMeasure,
} from "@/bindings";

export type {
  CreatedInstance,
  Destination,
  ImportProfile,
  ImportProgress,
  ImportReport,
  ImportRequest,
  ImportScanView,
  ImportShared,
  ImportSource,
  ImportStage,
  ItemOutcome,
  ItemRef,
  OutcomeReason,
  OutcomeStatus,
  ProfileMeasure,
  SettingsChange,
  SettingsOrigin,
  SharedData,
  SharedSizes,
  SourceKind,
  SourceMeasure,
  SourceRequest,
  Unsupported,
} from "@/bindings";

/** `api.importer`. */
export interface ImportApi {
  /** Other launchers' data on this computer (quick: small files and folder listings). */
  scan(): Promise<ImportScanView>;
  /** Sizes and the mods that fit, for one source's rows (reads folders and mod jars). */
  measure(sourceId: string): Promise<SourceMeasure>;
  /** Copies what was picked; every item reports how it went. */
  start(
    request: ImportRequest,
    jobId: string,
    onProgress: (progress: ImportProgress) => void,
  ): Promise<ImportReport>;
  /** Stops after the file being copied; finished items stay. */
  cancel(jobId: string): Promise<void>;
  /**
   * "Geri al" for the game settings an import changed in an instance (`ImportReport.settings`,
   * the v1 step's report too): every file back as it was. `false` when that change can no longer
   * be taken back (a newer import replaced it). Refused while the game runs.
   */
  undoSettings(instanceId: string, backup: string): Promise<boolean>;
}
