import type {
  Lang,
  LegacyImportReport,
  LegacyImportRequest,
  LegacyScan,
  PresenceContext,
} from "@/bindings";
import { isTauri } from "../platform";
import type {
  AfkSettings,
  AfkSettingsSaveResult,
  AfkSettingsView,
  PatternProblem,
  SettingsPage,
} from "./afk-settings";
import type { CleanupApi } from "./cleanup-types";
import type { ContentApi } from "./content-types";
import type { DataApi } from "./data-types";
import type { HelpApi } from "./help-types";
import type { ImportApi } from "./import-types";
import type { JavaApi } from "./java-types";
import type { LibraryApi } from "./library-types";
import { mockApi } from "./mock";
import type { ModCenterApi } from "./modcenter-types";
import type { ModHealthApi } from "./modhealth-types";
import type { ModSetsApi } from "./modsets-types";
import type { SkinsApi } from "./skins-types";
import type { SocialApi } from "./social-types";
import { tauriApi } from "./tauri";
import type {
  AccountSummary,
  AfkAccount,
  AfkLogLine,
  AppInfo,
  ContentStatus,
  DeepLink,
  DroppedFile,
  DropResult,
  FileDragEvent,
  HomeContent,
  InstanceDetail,
  InstanceSettings,
  InstanceSummary,
  LaunchEvent,
  LaunchOptions,
  LoaderKind,
  LoaderVersion,
  LoginStep,
  McVersion,
  MicrosoftLoginResult,
  NewInstance,
  OfficialStatus,
  ResumeTarget,
  ServerStatus,
  SessionCheck,
} from "./types";

/** Everything the UI asks of the backend. One implementation per environment. */
export interface LauncherApi {
  appInfo(): Promise<AppInfo>;
  homeContent(): Promise<HomeContent>;
  serverStatus(): Promise<ServerStatus>;
  instances(): Promise<InstanceSummary[]>;
  instance(id: string): Promise<InstanceDetail>;
  createInstance(spec: NewInstance): Promise<InstanceSummary>;
  updateInstanceSettings(id: string, settings: InstanceSettings): Promise<void>;
  setModEnabled(instanceId: string, fileName: string, enabled: boolean): Promise<void>;
  openInstanceFolder(id: string, sub?: string): Promise<void>;
  duplicateInstance(id: string): Promise<InstanceSummary>;
  /** Moves the instance to the launcher trash; resolves to its trash id ("Geri al"). */
  deleteInstance(id: string): Promise<string>;
  /** Puts a deleted instance back (a numbered name when its name was taken meanwhile). */
  restoreInstance(trashId: string): Promise<InstanceSummary>;
  minecraftVersions(): Promise<McVersion[]>;
  loaderVersions(loader: LoaderKind, mcVersion: string): Promise<LoaderVersion[]>;
  accounts(): Promise<AccountSummary[]>;
  addOfflineAccount(name: string): Promise<AccountSummary>;
  /**
   * Browser sign-in. Without `relogin` the account is added (or renewed) and becomes active;
   * with `relogin` (an account id) it renews that account's session and keeps the selection.
   */
  loginMicrosoft(
    onStep: (step: LoginStep) => void,
    options?: { relogin?: string },
  ): Promise<MicrosoftLoginResult>;
  cancelMicrosoftLogin(): Promise<void>;
  setActiveAccount(id: string): Promise<void>;
  /** The sign-in leaves this computer at once; `undoRemoveAccount` can bring it back for a while. */
  removeAccount(id: string): Promise<{ activeId: string | null }>;
  undoRemoveAccount(id: string): Promise<AccountSummary>;
  /** The undo window ended: the removal is final. */
  forgetRemovedAccount(id: string): Promise<void>;
  /** Offline accounts only; the UUID follows the name. */
  renameAccount(id: string, name: string): Promise<AccountSummary>;
  /** A newly pinned account joins the end of the pinned ones. */
  setAccountPinned(id: string, pinned: boolean): Promise<AccountSummary>;
  /** Saves the order of the pinned accounts; unpinned ids are ignored. */
  reorderPinnedAccounts(ids: string[]): Promise<void>;
  /** Offline accounts: a bundled picture (`preset:<slug>`), or null for the player head. */
  setAccountAvatar(id: string, avatar: string | null): Promise<AccountSummary>;
  /** Refreshes the session when it is about to expire, like a launch would. */
  checkAccountSession(id: string): Promise<SessionCheck>;
  /** Cuts and caches missing player heads; returns how many were added. */
  ensureAccountHeads(): Promise<number>;
  afkAccounts(): Promise<AfkAccount[]>;
  afkAdd(nickname: string): Promise<AfkAccount>;
  afkRemove(id: string): Promise<void>;
  afkConnect(id: string): Promise<void>;
  afkStop(id: string): Promise<void>;
  afkConnectAll(): Promise<void>;
  afkStopAll(): Promise<void>;
  afkSend(id: string, line: string): Promise<void>;
  afkLog(id: string): Promise<AfkLogLine[]>;
  /** One account's settings sheet; passwords in login commands stay in the OS keychain. */
  afkSettings(id: string): Promise<AfkSettingsView>;
  /** Validated again by the backend: incomplete settings come back as `invalid`. */
  afkSaveSettings(id: string, settings: SettingsPage): Promise<AfkSettingsSaveResult>;
  /** Whether the AFK client can use an auto-respond pattern (null), checked as it is typed. */
  afkCheckPattern(pattern: string, isRegex: boolean): Promise<PatternProblem | null>;
  /** Stops and connects the account again so saved settings apply (connects an idle one). */
  afkReconnect(id: string): Promise<void>;
  afkGlobalSettings(): Promise<AfkSettings>;
  afkSetShowScriptCommands(show: boolean): Promise<void>;
  /** Opens the account's folder (chat log, alerts, console log) in the file manager. */
  afkOpenFolder(id: string): Promise<void>;
  /** Subscribe to AFK state/log changes. Returns an unsubscribe function. */
  onAfkChange(listener: () => void): () => void;
  launch(
    instanceId: string,
    onEvent: (event: LaunchEvent) => void,
    options?: LaunchOptions,
  ): Promise<void>;
  cancelLaunch(instanceId: string): Promise<void>;
  /**
   * "Bağlan" (FIX2): asks the game `game` (the launcher's number for it) to close the way its own
   * close button does (it saves and leaves its server). False when nothing of it could be asked.
   */
  closeGame(game: number): Promise<boolean>;
  /** Ends the game `game` at once (only at the player's word, after the warning). */
  killGame(game: number): Promise<void>;
  /**
   * Whether one more game of the instance fits in the memory the computer has free now
   * (PLAY-AGAIN: asked before any game beside the ones running).
   */
  gameMemoryFits(instanceId: string): Promise<boolean>;
  /** "Ekran görüntülerini panoya kopyala" switched: a game that runs now follows it at once. */
  setScreenshotCopy(enabled: boolean): Promise<void>;
  openUrl(url: string): Promise<void>;
  updatePresence(context: PresenceContext): Promise<void>;
  setPresenceEnabled(enabled: boolean): Promise<void>;
  /**
   * The language the UI shows: the backend's own texts (tray, presence, errors) follow it.
   * `pick` is the player's own pick (`null`: "Otomatik"), which the backend keeps for its next
   * start: AFK accounts that start automatically connect before the page names the language.
   */
  setLanguage(lang: Lang, pick: Lang | null): Promise<void>;
  /** Whether the selected account is in game: the tray's "Oyna" reads "Oyunda" meanwhile. */
  setTrayInGame(inGame: boolean): Promise<void>;
  legacyScan(): Promise<LegacyScan>;
  legacyImport(request: LegacyImportRequest): Promise<LegacyImportReport>;
  officialStatus(): Promise<OfficialStatus>;
  /**
   * The Projects instance, made when there is none (files install on the first launch), unless
   * the player deleted it: then `null`, and the caller offers "Resmî paketi kur" (LIB1).
   */
  ensureOfficialInstance(): Promise<InstanceSummary | null>;
  /** "Resmî paketi kur": the Projects instance, made now even after the player deleted it. */
  installOfficialInstance(): Promise<InstanceSummary>;
  /** New content arrived in the background. Returns an unsubscribe function. */
  onContentChanged(listener: () => void): () => void;
  /** Whether the last content refresh reached the servers (the home screen's offline strip). */
  contentStatus(): Promise<ContentStatus>;
  /** The servers became reachable or unreachable. Returns an unsubscribe function. */
  onContentStatus(listener: (status: ContentStatus) => void): () => void;
  /** Checks the servers again now (the system reported the network gone or back). */
  contentRecheck(): Promise<void>;
  /** Tray actions and projects:// links. Returns an unsubscribe function. */
  onDeepLink(listener: (link: DeepLink) => void): () => void;
  /** The link that started the app, if any (taken once). */
  takePendingDeepLink(): Promise<DeepLink | null>;
  /** Quits for real; AFK sessions are stopped cleanly first. */
  quitApp(): Promise<void>;
  /** Files dragged over the window (enter carries the paths). Returns an unsubscribe function. */
  onFileDrag(listener: (event: FileDragEvent) => void): () => void;
  /** What dragged files are, read from their contents. */
  classifyFiles(paths: string[]): Promise<DroppedFile[]>;
  /** Copy dropped mods and packs into an instance (modpacks come back for the import flow). */
  dropFiles(instanceId: string, paths: string[]): Promise<DropResult>;
  /** "Devam et": the world or server the instance played last (1.20+), or null. */
  resumeTarget(instanceId: string): Promise<ResumeTarget | null>;
  /** A desktop shortcut that starts the instance; `iconPng` is its icon tile (base64 PNG). */
  createShortcut(instanceId: string, iconPng?: string): Promise<void>;
  /** Modrinth browser, installed add-ons and modpacks. */
  content: ContentApi;
  /** Skins and capes of Microsoft accounts. */
  skins: SkinsApi;
  /** Instance page logs, worlds and screenshots; library groups. */
  library: LibraryApi;
  /** Settings → Veri: data folder, support bundle, moving the folder. */
  data: DataApi;
  /** Settings → Veri → Temizlik: old sets, unused game files, leftovers, the launcher trash. */
  cleanup: CleanupApi;
  /** Mod center (recommended mods), per-version sets and moving own mods. */
  modCenter: ModCenterApi;
  /** Ready-made mod sets ("Hazır setler"): the catalog, an instance's sets, new instances. */
  modSets: ModSetsApi;
  /** Friends (optional feature; hidden until the signed config turns it on). */
  social: SocialApi;
  /** Mod health: the mod check, repairs and set reinstalls. */
  modHealth: ModHealthApi;
  /** Settings → Java: the launcher's runtimes and the Javas on the computer. */
  java: JavaApi;
  /** Bringing a player over from another launcher (the welcome flow, Settings → Veri). */
  importer: ImportApi;
  /** The troubleshooter (Settings → Sorun yardımcısı): every check once, row by row. */
  help: HelpApi;
}

export type {
  LegacyImportReport,
  LegacyImportRequest,
  LegacyScan,
  PresenceContext,
} from "@/bindings";

export const api: LauncherApi = isTauri() ? tauriApi : mockApi;

export * from "./afk-settings";
export * from "./cleanup-types";
export * from "./content-types";
export * from "./data-types";
export * from "./help-types";
export * from "./import-types";
export * from "./java-types";
export * from "./library-types";
export * from "./modcenter-types";
export * from "./modhealth-types";
export * from "./modsets-types";
export * from "./skins-types";
export * from "./social-types";
export * from "./types";
