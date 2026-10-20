// Mod health: the pre-launch mod check, repairs ("Onar") and set reinstalls ("Yeniden
// yükle"). UI-facing shapes; the Tauri adapter (modhealth-tauri.ts) maps the bindings onto them.
import type { LoaderKind } from "./types";

export type HealthSeverity = "blocking" | "warning" | "info";

export type HealthIssueKind =
  | "corrupt"
  | "unreadable"
  | "providedByLoader"
  | "duplicate"
  | "missingDependency"
  | "dependencyVersion"
  | "incompatibleMinecraft"
  | "incompatibleLoader"
  | "javaVersion"
  | "wrongLoader"
  | "breaks"
  | "conflicts"
  | "brokenMetadata"
  | "noMetadata"
  /** The official pack's mod keys still clash in an older `options.txt` (KEYS2). */
  | "keyClash";

/**
 * A binding the key fix changes: one clash of the official pack's mods, or (no `partner`) an
 * unbound push to talk it gives its key along with them (KEYS3).
 */
export interface KeyChange {
  /** The key mapping (`key.wi_zoom.zoom`); the UI names its action. */
  name: string;
  /** The mod it belongs to ("WI Zoom"). */
  modName: string;
  /** What it does, in English (for a mapping the UI has no words for). */
  action: string;
  /** The mapping it shares the key with; none for a binding that only gets a key. */
  partner?: string;
  /** The key it has (`key.keyboard.v`); `key.keyboard.unknown` for one that only gets a key. */
  key: string;
  /** The key it gets (`key.keyboard.c`); none leaves it without one. */
  to?: string;
}

/** What "Düzelt" did to the key clash. */
export interface KeyFix {
  /** "Geri al" puts the keys back from this backup; none when nothing clashed any more. */
  backup?: string;
  changes: KeyChange[];
}

/** What an issue's button says: "Düzelt", "Bu modu kapat", "<ad> modunu kapat", … */
export type FixLabel = "fix" | "disable" | "disableNamed" | "disableOld" | "enable";

export interface FixOffer {
  label: FixLabel;
  /** The mod the button acts on (`disableNamed`). */
  name?: string;
  /** Files it changes (older copies of a duplicate can be several). */
  count: number;
}

/** One problem of the mod check. Visible text is built from `kind` and the names. */
export interface HealthIssue {
  key: string;
  severity: HealthSeverity;
  kind: HealthIssueKind;
  /** Display names of the mods it is about. */
  mods: string[];
  /** The other side: the dependency, the incompatible mod, the mod that bundles a copy. */
  target?: string;
  /** A mod made for another loader: that loader. */
  madeFor?: string;
  /** A mod that needs a newer Java: the version. */
  java?: number;
  /** A copy is bundled inside another mod. */
  bundled: boolean;
  /** Files involved ("copy details" only). */
  files: string[];
  offer?: FixOffer;
  /** A key clash: the bindings "Düzelt" changes (the clashes, then an unbound push to talk). */
  keys?: KeyChange[];
}

export interface ModCheck {
  instanceId: string;
  minecraft: string;
  loader: LoaderKind;
  /** Blocking issues and warnings, worst first. */
  issues: HealthIssue[];
  blocking: number;
  checkedFiles: number;
  /** "Onar" has something to do. */
  repairable: boolean;
  /** The set has mods ("Yeniden yükle" makes sense). */
  hasMods: boolean;
}

export type RepairStepKind =
  | "verifyPack"
  | "redownloadPack"
  | "revertPack"
  | "redownload"
  | "enable"
  | "disable"
  | "disableOld"
  | "loader"
  | "java"
  | "update"
  | "fetch";

export type StepStatus = "pending" | "running" | "done" | "failed" | "skipped";

/** Why a step ended as it did. */
export type StepNote =
  | "disabledInstead"
  | "noVersion"
  | "notFound"
  | "fromPack"
  | "notNeeded"
  | "offline"
  | "busy"
  | "packDecides"
  | "notPossible"
  | "keptByPack"
  | "cancelled";

export interface RepairStep {
  /** Stable while the repair runs (list key). */
  key: string;
  kind: RepairStepKind;
  /** The mod, the loader or "Java 21"; empty for the pack check. */
  name: string;
  status: StepStatus;
  note?: StepNote;
}

export interface RepairProgress {
  steps: RepairStep[];
  /** Download progress of the running step, 0–100. */
  percent?: number;
  /** The mods are being checked between phases. */
  checking: boolean;
}

export interface RepairResult {
  steps: RepairStep[];
  /** The check after the repair. */
  check: ModCheck;
  cancelled: boolean;
}

export type RepairScope = { kind: "all" } | { kind: "crash" } | { kind: "issue"; key: string };

export type ReinstallPhase = "keepAside" | "trash" | "pack" | "modrinth" | "check";

export interface ReinstallPreview {
  minecraft: string;
  /** Mod settings go back to their defaults. */
  resetsSettings: boolean;
  /** Names of jars of unknown origin, kept aside. */
  kept: string[];
  packMods: number;
  modrinthMods: number;
  /** The official pack is needed but cannot be loaded now. */
  needsNetwork: boolean;
}

export interface ReinstallProgress {
  phases: ReinstallPhase[];
  phase: ReinstallPhase;
  percent?: number;
  /** It can still be stopped (nothing has moved yet). */
  cancellable: boolean;
}

export interface ReinstallResult {
  kept: string[];
  /** Folder of the kept jars, relative to the game folder ("Klasörü aç"). */
  keptFolder?: string;
  packSynced: boolean;
  reinstalled: number;
  /** Mods without a version for this game version: their old files came back. */
  restored: string[];
  settingsReset: boolean;
  check: ModCheck;
  cancelled: boolean;
}

/** `api.modHealth`. */
export interface ModHealthApi {
  check(instanceId: string): Promise<ModCheck>;
  repair(
    instanceId: string,
    scope: RepairScope,
    jobId: string,
    onProgress: (progress: RepairProgress) => void,
  ): Promise<RepairResult>;
  reinstallPreview(instanceId: string): Promise<ReinstallPreview>;
  reinstall(
    instanceId: string,
    jobId: string,
    onProgress: (progress: ReinstallProgress) => void,
  ): Promise<ReinstallResult>;
  /** Returns the maximum that was saved (kept within what the computer allows). */
  setMaxMemory(instanceId: string, maxMb: number): Promise<number>;
  /** Sets the instance's own JVM arguments ("" resets them); returns the ones it replaced. */
  setJvmArgs(instanceId: string, jvmArgs: string): Promise<string>;
  cancel(jobId: string): Promise<void>;
  /** "Düzelt" for the key clash: backs up `options.txt`, moves only the clashing bindings. */
  fixKeys(instanceId: string): Promise<KeyFix>;
  /** "Geri al" after `fixKeys`; false when the backup is gone (nothing changed). */
  undoKeys(instanceId: string, backup: string): Promise<boolean>;
}

// --- crashes -----------------------------------------------------------------------------------

export type CrashKind =
  | "memoryReserve"
  | "outOfMemory"
  | "invalidJvmArgument"
  | "javaTooOld"
  | "javaTooNew"
  | "graphicsDriver"
  | "incompatibleMods"
  | "missingDependency"
  | "duplicateMods"
  | "modError"
  | "corruptedFiles"
  | "accessDenied"
  | "noOutput"
  | "unknown";

export type SuspectReason =
  | "requiresMissing"
  | "missing"
  | "requiresOtherVersion"
  | "wrongVersion"
  | "incompatibleMinecraft"
  | "needsNewerLoader"
  | "needsNewerJava"
  | "declaresIncompatible"
  | "incompatible"
  | "duplicate"
  | "mixinFailed"
  | "loadingFailed"
  | "namedInReport"
  | "inStackTrace"
  | "corruptFile";

export interface CrashSuspect {
  name: string;
  reason: SuspectReason;
  related?: string;
  /** Strong evidence (weak: only seen in the stack trace). */
  strong: boolean;
}

export interface MemoryAdvice {
  currentMb: number;
  suggestedMb: number;
  increase: boolean;
}
