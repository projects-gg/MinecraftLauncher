// UI-facing data shapes. They mirror the Rust crates' specta types; once a command is wired
// through tauri-specta, the generated type in `bindings.ts` replaces the one here.

export type LoaderKind = "vanilla" | "fabric" | "quilt" | "forge" | "neoForge";
export type NewsCategory = "update" | "event" | "announcement" | "patchNotes" | "community";
export type ParticleKind = "embers" | "sparks" | "snow" | "none";

export interface NewsItem {
  id: string;
  title: string;
  summary: string;
  body: string;
  coverUrl?: string;
  category: NewsCategory;
  publishedAt: string;
  pinned: boolean;
  featured: boolean;
  cta?: { label: string; url: string };
}

export interface HeroConfig {
  imageUrl: string;
  videoUrl?: string;
  title: string;
  subtitle: string;
  newsId?: string;
  particles: ParticleKind;
}

export interface BannerItem {
  id: string;
  level: "info" | "warning" | "event" | "critical";
  text: string;
  url?: string;
}

export interface EventItem {
  id: string;
  title: string;
  startsAt: string;
  endsAt: string;
}

export interface PatchNotes {
  packName: string;
  version: string;
  publishedAt: string;
  highlights: string[];
}

export interface HomeContent {
  hero: HeroConfig;
  banners: BannerItem[];
  events: EventItem[];
  news: NewsItem[];
  patchNotes?: PatchNotes;
  /** True when the content came from the local cache because the network failed. */
  offline: boolean;
}

export interface ServerStatus {
  name: string;
  address: string;
  online: boolean;
  players: number;
  maxPlayers: number;
  latencyMs?: number;
}

export type InstanceIcon =
  | "grass"
  | "swords"
  | "pickaxe"
  | "potion"
  | "dragon-egg"
  | "tower"
  | "map"
  | "compass";

export interface InstanceSummary {
  id: string;
  name: string;
  icon: InstanceIcon;
  mcVersion: string;
  loader: LoaderKind;
  loaderVersion?: string;
  managed: boolean;
  lastPlayedAt?: string;
  playTimeSecs: number;
  state: "ready" | "needsInstall" | "needsUpdate";
  autoJoin?: string;
}

export interface AccountSummary {
  id: string;
  kind: "microsoft" | "offline";
  username: string;
  headUrl?: string;
  active: boolean;
}

/** Mirrors pl-auth `LoginStep`. */
export type LoginStep =
  | "openingBrowser"
  | "waitingForBrowser"
  | "microsoft"
  | "xboxLive"
  | "xsts"
  | "minecraft"
  | "profile"
  | "done";

export type LaunchStage =
  | "metadata"
  | "java"
  | "libraries"
  | "assets"
  | "client"
  | "natives"
  | "loader"
  | "pack";

export type LaunchEvent =
  | {
      type: "progress";
      stage: LaunchStage;
      percent: number;
      doneFiles: number;
      totalFiles: number;
      bytesPerSec: number;
      etaSecs?: number;
    }
  | { type: "offline" }
  | { type: "launching" }
  | { type: "started"; pid: number | null }
  | { type: "windowShown" }
  | { type: "exited"; code: number | null; runtimeSecs: number }
  | {
      type: "crashed";
      title: string;
      hint: string;
      actions: string[];
      mods: string[];
      logFile: string | null;
    }
  | { type: "error"; message: string; details: string };

export interface McVersion {
  id: string;
  kind: "release" | "snapshot" | "oldBeta" | "oldAlpha";
  releaseTime: string;
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
}

export interface NewInstance {
  name: string;
  icon: InstanceIcon;
  mcVersion: string;
  loader: LoaderKind;
  loaderVersion?: string;
}

export interface ModEntry {
  fileName: string;
  name: string;
  version: string;
  authors: string[];
  description: string;
  enabled: boolean;
  iconUrl?: string;
  fromPack: boolean;
  sizeBytes: number;
}

export interface WorldEntry {
  folder: string;
  name: string;
  gameMode: "survival" | "creative" | "adventure" | "spectator" | "hardcore";
  lastPlayedAt: string;
  sizeBytes: number;
  iconUrl?: string;
}

export interface PackEntry {
  fileName: string;
  name: string;
  description: string;
  enabled: boolean;
  iconUrl?: string;
  sizeBytes: number;
}

export interface ScreenshotEntry {
  fileName: string;
  url: string;
  takenAt: string;
}

export interface SavedServer {
  name: string;
  address: string;
  online?: boolean;
  players?: number;
  maxPlayers?: number;
}

export interface InstanceSettings {
  javaMode: "auto" | "custom";
  javaPath?: string;
  minMemoryMb: number;
  maxMemoryMb: number;
  jvmArgs: string;
  width: number;
  height: number;
  fullscreen: boolean;
  preLaunch: string;
  postExit: string;
  wrapper: string;
  notes: string;
}

export interface InstanceDetail extends InstanceSummary {
  settings: InstanceSettings;
  mods: ModEntry[];
  resourcePacks: PackEntry[];
  shaderPacks: PackEntry[];
  worlds: WorldEntry[];
  screenshots: ScreenshotEntry[];
  servers: SavedServer[];
  systemMemoryMb: number;
}

export type AfkState =
  | { kind: "stopped" }
  | { kind: "queued"; inSecs: number }
  | { kind: "starting" }
  | { kind: "connecting" }
  | { kind: "connected"; since: string }
  | { kind: "reconnecting"; attempt: number; nextAttemptAt: string }
  | { kind: "stopping" }
  | { kind: "error"; reason: string };

export interface AfkAccount {
  id: string;
  nickname: string;
  server: string;
  autoStart: boolean;
  mcVersion: string;
  state: AfkState;
  health?: number;
  food?: number;
  players?: number;
}

export interface AfkLogLine {
  id: number;
  time: string;
  text: string;
  kind: "chat" | "system" | "error" | "script" | "self";
}

export interface AppInfo {
  version: string;
  os: string;
  arch: string;
  debug: boolean;
}
