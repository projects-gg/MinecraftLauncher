import { isTauri } from "../platform";
import { mockApi } from "./mock";
import { tauriApi } from "./tauri";
import type { LegacyImportReport, LegacyImportRequest, LegacyScan, PresenceContext } from "@/bindings";
import type {
  AccountSummary,
  AfkAccount,
  AfkLogLine,
  AppInfo,
  HomeContent,
  InstanceDetail,
  InstanceSettings,
  InstanceSummary,
  LaunchEvent,
  LoaderKind,
  LoaderVersion,
  LoginStep,
  McVersion,
  NewInstance,
  ServerStatus,
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
  deleteInstance(id: string): Promise<void>;
  minecraftVersions(): Promise<McVersion[]>;
  loaderVersions(loader: LoaderKind, mcVersion: string): Promise<LoaderVersion[]>;
  accounts(): Promise<AccountSummary[]>;
  addOfflineAccount(name: string): Promise<AccountSummary>;
  loginMicrosoft(onStep: (step: LoginStep) => void): Promise<AccountSummary>;
  cancelMicrosoftLogin(): Promise<void>;
  setActiveAccount(id: string): Promise<void>;
  removeAccount(id: string): Promise<void>;
  afkAccounts(): Promise<AfkAccount[]>;
  afkAdd(nickname: string): Promise<AfkAccount>;
  afkRemove(id: string): Promise<void>;
  afkConnect(id: string): Promise<void>;
  afkStop(id: string): Promise<void>;
  afkConnectAll(): Promise<void>;
  afkStopAll(): Promise<void>;
  afkSend(id: string, line: string): Promise<void>;
  afkLog(id: string): Promise<AfkLogLine[]>;
  /** Subscribe to AFK state/log changes. Returns an unsubscribe function. */
  onAfkChange(listener: () => void): () => void;
  launch(instanceId: string, onEvent: (event: LaunchEvent) => void): Promise<void>;
  cancelLaunch(instanceId: string): Promise<void>;
  openUrl(url: string): Promise<void>;
  updatePresence(context: PresenceContext): Promise<void>;
  setPresenceEnabled(enabled: boolean): Promise<void>;
  legacyScan(): Promise<LegacyScan>;
  legacyImport(request: LegacyImportRequest): Promise<LegacyImportReport>;
}

export type { LegacyImportReport, LegacyImportRequest, LegacyScan, PresenceContext } from "@/bindings";

export const api: LauncherApi = isTauri() ? tauriApi : mockApi;

export * from "./types";
