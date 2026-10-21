// The troubleshooter (Settings → Sorun yardımcısı, HELP1): what each check found. UI-facing shapes;
// the Tauri adapter (help-tauri.ts) maps the bindings onto them. Nothing here is personal: levels,
// codes, numbers, versions and mod names only (`pl_data::troubleshoot`).
import type { CrashKind } from "./modhealth-types";
import type { LaunchEvent, LoaderKind } from "./types";

/** A row the launcher checks, in the order the rows are shown. */
export type HelpCheck =
  | "internet"
  | "server"
  | "content"
  | "friends"
  | "microsoft"
  | "discord"
  | "clock"
  | "disk"
  | "java"
  | "memory"
  | "mods"
  | "crash";

/** Only `warning` and `problem` count as findings; `note` is worth knowing, nothing to fix. */
export type HelpLevel = "ok" | "note" | "warning" | "problem" | "skipped";

/** What a check found, stable for support (the summary writes `serverUnreachable` as `SERVER-UNREACHABLE`). */
export type HelpCode =
  | "internetOk"
  | "internetSlow"
  | "internetOffline"
  | "internetLocal"
  | "internetSignIn"
  | "internetNoDns"
  | "internetInsecure"
  | "internetDown"
  | "serverOk"
  | "serverSlow"
  | "serverDown"
  | "serverNotFound"
  | "serverUnreachable"
  | "serverNeedsInternet"
  | "contentOk"
  | "contentUnreachable"
  | "contentMissing"
  | "contentNeedsInternet"
  | "friendsOk"
  | "friendsDown"
  | "friendsUnreachable"
  | "friendsNeedsInternet"
  | "microsoftOk"
  | "microsoftDown"
  | "microsoftUnreachable"
  | "microsoftNeedsInternet"
  | "discordOk"
  | "discordUnreachable"
  | "discordNeedsInternet"
  | "clockWrong"
  | "diskOk"
  | "diskLow"
  | "diskFull"
  | "diskUnknown"
  | "javaReady"
  | "javaDownloads"
  | "javaCustomOk"
  | "javaX86"
  | "javaMissing"
  | "javaBroken"
  | "javaTooOld"
  | "javaTooNew"
  | "javaUnknown"
  | "memoryTooHigh"
  | "memoryHigh"
  | "modsOk"
  | "modsWarning"
  | "modsBlocking"
  | "modsUnknown"
  | "crashNone"
  | "crashResolved"
  | "crashRecent"
  | "checkFailed";

/** The one-tap fix a row offers. */
export type HelpFix =
  | "makeSpace"
  | "installJava"
  | "instanceSettings"
  | "repair"
  | "showMods"
  | "crashDetails"
  | "memoryAuto"
  | "clockSettings"
  | "statusPage";

/** Numbers and names a row's words use. */
export interface HelpFacts {
  latencyMs?: number;
  players?: number;
  freeBytes?: number;
  /** A Java major version (the one there is). */
  major?: number;
  /** The Java the game needs (the lowest, or the highest for a Java that is too new). */
  need?: number;
  /** Mod issues, or mods. */
  count?: number;
  /** How far the clock is off (positive: ahead). */
  minutes?: number;
  /** When the last crash happened (ISO). */
  at?: string;
  crash?: CrashKind;
  /** The mod a crash points at. */
  name?: string;
}

export interface HelpFinding {
  check: HelpCheck;
  level: HelpLevel;
  code: HelpCode;
  fix?: HelpFix;
  facts: HelpFacts;
}

/** What a run sends while it goes. */
export type HelpEvent =
  | { type: "plan"; checks: HelpCheck[] }
  | { type: "finding"; finding: HelpFinding };

/** The computer, for the summary. */
export interface HelpSystem {
  launcher: string;
  debug: boolean;
  /** "Windows 11 Pro 24H2". */
  os: string;
  arch: string;
  memoryMb: number;
}

/** The instance the checks looked at, for the summary (never its name). */
export interface HelpInstance {
  kind: "official" | "custom";
  minecraft: string;
  loader: LoaderKind;
  loaderVersion?: string;
  packVersion?: string;
}

export interface HelpReport {
  findings: HelpFinding[];
  system: HelpSystem;
  instance?: HelpInstance;
}

/** The crash dialog's content, as a launch's crash event carries it. */
export type HelpCrash = Omit<Extract<LaunchEvent, { type: "crashed" }>, "type">;

export interface HelpApi {
  /**
   * Checks everything once, sending each row as it settles. `instanceId`: the instance Play
   * starts; without one the game rows are left out.
   */
  run(instanceId: string | null, onEvent: (event: HelpEvent) => void): Promise<HelpReport>;
  /** The crash dialog for the instance's last crash; `null` when it is resolved, old or gone. */
  crash(instanceId: string): Promise<HelpCrash | null>;
  /** The system's date and time settings (Windows and macOS). */
  openClock(): Promise<void>;
}
