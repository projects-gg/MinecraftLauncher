// UI-facing data shapes. They mirror the Rust crates' specta types; once a command is wired
// through tauri-specta, the generated type in `bindings.ts` replaces the one here.
import type { QuickPlay } from "@/bindings";
import type { JavaChoice, JavaPrompt } from "./java-types";
import type { ModsNotice } from "./modcenter-types";
import type { CrashKind, CrashSuspect, MemoryAdvice, ModCheck } from "./modhealth-types";
import type { SetBadge } from "./modsets-types";

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
  /** From the Discord bot's news feed: links open only https:// and projects://, no remote images. */
  fromFeed?: boolean;
  /** The original Discord message ("Discord'da aç"). */
  sourceUrl?: string;
}

/**
 * One of Minecraft's own updates from the news feed: it only fills places Projects news leaves
 * empty and is never "new". Texts in the feed's language (Turkish); `en` for an English interface
 * (`lib/minecraft.ts` → `minecraftText`).
 */
export interface MinecraftNewsItem {
  id: string;
  /** Mojang's version id (`26.3`). */
  version: string;
  kind: "release" | "snapshot";
  title: string;
  summary: string;
  /** Markdown (links only https:// and projects://, no remote images). */
  body: string;
  /** The verified cover, or the season art for an entry without one. */
  coverUrl?: string;
  /** Shown until the cover is there (it may still be downloading). */
  coverBlurhash?: string;
  publishedAt: string;
  expiresAt: string;
  /** The official article on minecraft.net (https://). */
  url: string;
  en?: { title: string; summary?: string; body?: string };
}

export interface HeroConfig {
  imageUrl: string;
  videoUrl?: string;
  title: string;
  subtitle: string;
  newsId?: string;
  /** Studio's button text (the article button falls back to "Haberi oku"). */
  ctaLabel?: string;
  /** The button opens this link instead of an article. */
  ctaUrl?: string;
  particles: ParticleKind;
}

export interface BannerItem {
  id: string;
  level: "info" | "warning" | "event" | "critical";
  text: string;
  url?: string;
  /** The player may close it; a closed banner stays closed. */
  dismissible?: boolean;
  /** It leaves the screen at this moment. */
  endsAt?: string;
}

export interface EventItem {
  id: string;
  title: string;
  startsAt: string;
  /** When it ends (for an open-ended event, three hours after the start). */
  endsAt: string;
  /** The end is unknown: shown as started, without time left. */
  openEnded?: boolean;
  /** The news item about the event. */
  newsId?: string;
  url?: string;
  /** Among live events the highest leads the card (default 0). */
  priority?: number;
  /** On the server: while it is live the card offers "Katıl". */
  join?: boolean;
  /** From the Discord feed (config events win ties). */
  fromFeed?: boolean;
}

/** The patch notes box: the newest patch notes news, or else the official pack's changelog. */
export type PatchNotes =
  | {
      kind: "news";
      newsId: string;
      title: string;
      publishedAt: string;
      highlights: string[];
    }
  | {
      kind: "pack";
      packName: string;
      version: string;
      publishedAt: string;
      highlights: string[];
      /** Full notes (Markdown) when the pack publishes them. */
      changelog?: string;
    };

export interface Maintenance {
  title: string;
  message: string;
  until?: string;
}

/**
 * An active problem on the server's status page, from the Discord bot's feed: shown where the
 * maintenance notice goes while there is none. It never blocks anything.
 */
export interface HomeStatus {
  /** Services down, reachable only in part, or a warning posted on the status page. */
  level: "outage" | "degraded" | "notice";
  title: string;
  /** May be empty. */
  message: string;
  /** The status page (https:// only). */
  url?: string;
  /** It leaves the screen at this moment, refreshed or not. */
  expiresAt: string;
}

/** A community or social platform shown with its own mark. */
export type CommunityPlatform =
  | "wiki"
  | "discord"
  | "forum"
  | "instagram"
  | "tiktok"
  | "youtube"
  | "x"
  | "twitch"
  | "kick";

/** A link of the community row (`https://` only). */
export interface CommunityLink {
  platform: CommunityPlatform;
  url: string;
}

/** Studio's links (Settings → Hakkında): only https:// and projects:// ones arrive here. */
export interface ContentLinks {
  website?: string;
  discord?: string;
  store?: string;
  support?: string;
  /** The other links, by label; the community platforms are in `community`. */
  custom: Array<{ label: string; url: string }>;
  /**
   * Wiki, Discord, forum, then the social accounts, in this order (Home's news column, Settings →
   * Hakkında). Studio's addresses, or the built-in ones while it publishes none.
   */
  community: CommunityLink[];
}

export interface HomeContent {
  hero: HeroConfig;
  banners: BannerItem[];
  events: EventItem[];
  news: NewsItem[];
  /** Minecraft's updates, newest first: only where `news` leaves room (never new, never pinned). */
  minecraft: MinecraftNewsItem[];
  patchNotes?: PatchNotes;
  /** Missing before anything is published (the built-in addresses stand in). */
  links?: ContentLinks;
  /** The server is in maintenance (published by Studio). */
  maintenance?: Maintenance;
  /** An active problem on the status page; never set while `maintenance` is. */
  status?: HomeStatus;
  /** True when the content came from the local cache because the network failed. */
  offline: boolean;
}

/** Whether the last content refresh reached the servers. */
export interface ContentStatus {
  offline: boolean;
}

export interface ServerStatus {
  name: string;
  address: string;
  online: boolean;
  players: number;
  maxPlayers: number;
  latencyMs?: number;
  /** The server's own icon, when its ping sends one. */
  iconUrl?: string;
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
  /** Library group; absent for instances without one. */
  group?: string;
  /** ISO time the instance was created. */
  createdAt?: string;
}

export interface AccountSummary {
  id: string;
  kind: "microsoft" | "offline";
  username: string;
  /** The player's head (a tiny PNG), cut from the account's own skin; absent until cached. */
  headUrl?: string;
  active: boolean;
  /** ISO time the account was added. */
  addedAt?: string;
  /** ISO time the game was last started with it. */
  lastUsedAt?: string;
  /** Microsoft refused the saved session: only a new sign-in fixes it. */
  needsRelogin?: boolean;
  /** Pinned: listed first, in the player's order (the order of pinned accounts in the list). */
  pinned?: boolean;
  /** Bundled picture shown instead of the head (`preset:<slug>`, offline accounts). */
  avatar?: string;
}

/** What a Microsoft sign-in did (mirrors the command's `MicrosoftLoginOutcome`). */
export type MicrosoftLoginOutcome = "added" | "alreadyAdded" | "relogged" | "otherAccount";

export interface MicrosoftLoginResult {
  account: AccountSummary;
  outcome: MicrosoftLoginOutcome;
}

/** Whether a Microsoft account's saved session still works. */
export type SessionCheck = "valid" | "expired" | "unknown";

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
  /**
   * The game process started; its window is not on screen yet ("Açılıyor"). `firstStart`: the
   * first start with this version and loader, which takes longer.
   */
  | { type: "started"; pid: number | null; firstStart: boolean }
  /** The game's window is on screen ("Oyunda"); once, never after the exit. */
  | { type: "windowShown" }
  | { type: "exited"; code: number | null; runtimeSecs: number }
  | {
      type: "crashed";
      title: string;
      hint: string;
      actions: string[];
      mods: string[];
      logFile: string | null;
      crashReport: string | null;
      kind: CrashKind;
      instanceId: string;
      /** Mods the crash points at, strongest evidence first. */
      suspects: CrashSuspect[];
      /** "Onar" has something to do. */
      repairable: boolean;
      /** The first strongly suspected mod that can be turned off. */
      turnOff: { file: string; name: string } | null;
      /** "Yeniden yükle" makes sense. */
      reinstall: boolean;
      /** Memory crashes: the maximum memory to retry with. */
      memory: MemoryAdvice | null;
      /** The instance's own JVM arguments stopped the JVM: "Argümanları sıfırla". */
      resetJvmArgs?: boolean;
    }
  | { type: "error"; message: string; details: string }
  /** `cancelLaunch` stopped the launch before the game started (not an error). */
  | { type: "cancelled" }
  | {
      type: "packUpdated";
      name: string;
      version: string;
      changelog?: string;
      changedFiles: number;
    }
  | ({ type: "modsNotice" } & ModsNotice)
  | {
      /** The pack no longer offers this version; the installed set was launched as it is. */
      type: "packVersionDropped";
      minecraft: string;
      recommended?: string;
    }
  /** The mods would certainly stop the game: it was not started. */
  | { type: "modsBlocked"; check: ModCheck }
  /** The instance's custom Java does not suit the game: it was not started (J1). */
  | ({ type: "javaMismatch" } & JavaPrompt)
  /** Some mods will not work; the game starts anyway. */
  | { type: "modsWarning"; check: ModCheck }
  /**
   * The official pack's mod keys clash in an older `options.txt`: the game was not started
   * (asked once per session: "Düzelt" or "Yine de başlat").
   */
  | { type: "keyClash"; check: ModCheck }
  /**
   * The session took screenshots (sent right before `exited`); `recent` are image URLs (newest
   * first), `newest` is the newest one's file name ("Kopyala" copies it).
   */
  | { type: "newScreenshots"; count: number; recent: string[]; newest: string }
  /** "Ekran görüntülerini panoya kopyala" could not copy a new screenshot (once per game). */
  | { type: "screenshotNotCopied" };

/** A world or server joined right after start (1.20+ quick play). */
export type QuickPlayTarget = QuickPlay;

/** Options of a launch. */
export interface LaunchOptions {
  /** Start even when the mod check finds problems that stop the game ("Yine de başlat"). */
  ignoreModIssues?: boolean;
  /** Join the instance's server whatever the auto-join setting ("Katıl" from the friends list). */
  autoJoin?: boolean;
  /** "Devam et": join this world or server right away (wins over auto-join). */
  quickPlay?: QuickPlayTarget;
  /** The answer to the Java question of the previous attempt ("Kur ve oyna", "Yine de dene"). */
  java?: JavaChoice;
}

/** Where "Devam et" goes: the world or server the instance played last. */
export interface ResumeTarget {
  kind: "world" | "server";
  name: string;
  lastPlayedAt?: string;
  /** The world's or the server's own icon. */
  iconUrl?: string;
  quickPlay: QuickPlayTarget;
}

/** What a dropped file is, read from its contents. */
export type DroppedKind = "mod" | "resourcePack" | "shaderPack" | "modpack" | "unknown";

export interface DroppedFile {
  path: string;
  name: string;
  kind: DroppedKind;
}

/** What happened to the files of a drop. */
export interface DropResult {
  added: DroppedFile[];
  /** A file of the same name is already there. */
  existing: DroppedFile[];
  unknown: DroppedFile[];
  /** Mods dropped onto an instance without a mod loader. */
  needsLoader: DroppedFile[];
  /** Mods not copied while the instance's mods were changing. */
  busy: DroppedFile[];
  modpacks: DroppedFile[];
  failed: DroppedFile[];
}

/** A point in the window, in CSS pixels from its top-left corner. */
export interface DragPoint {
  x: number;
  y: number;
}

/**
 * Files dragged over the window. `enter` carries the paths (before the drop); `position` is the
 * cursor (absent when the source cannot tell).
 */
export type FileDragEvent =
  | { type: "enter"; paths: string[]; position?: DragPoint }
  | { type: "over"; position?: DragPoint }
  | { type: "drop"; paths: string[]; position?: DragPoint }
  | { type: "leave" };

export interface OfficialPack {
  id: string;
  name: string;
  version: string;
  mcVersion: string;
  loader: LoaderKind;
  summary?: string;
}

export interface OfficialStatus {
  /** The pack the server publishes; absent until one is published. */
  pack?: OfficialPack;
  /** The instance that follows it (or the "Projects" instance while none is published). */
  instanceId?: string;
  /**
   * There is none because the player deleted it (LIB1): nothing makes it again by itself; the
   * library and OYNA offer "Resmî paketi kur".
   */
  removed: boolean;
}

/** Where a projects:// link or a tray action sends the user. */
export type DeepLink =
  | { kind: "home" }
  | { kind: "play" }
  /** projects://play/<instance>: a desktop shortcut starts this instance. */
  | { kind: "playInstance"; id: string }
  | { kind: "news"; id: string }
  | { kind: "instance"; id: string }
  | { kind: "afk" }
  /** projects://friend/<code>: add a friend by code (the panel shows who it is first). */
  | { kind: "friend"; code: string }
  /** projects://crew/<link>: a crew's invite link (the crew page shows the crew first). */
  | { kind: "crew"; link: string };

export interface McVersion {
  id: string;
  kind: "release" | "snapshot" | "oldBeta" | "oldAlpha";
  releaseTime: string;
  /** The launcher's recommended version ("Önerilen"; preselected for new instances). */
  recommended: boolean;
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
  /** Recommended mod of the official pack: toggling it changes the mod center selection. */
  itemId?: string;
  sizeBytes: number;
  /** Modrinth project, when the launcher knows where the file came from. */
  projectId?: string;
  /** The ready-made set that installed the file. */
  modSet?: SetBadge;
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
  /** Modrinth project, when the launcher knows where the file came from. */
  projectId?: string;
  /** The ready-made set that installed the pack. */
  modSet?: SetBadge;
}

export interface ScreenshotEntry {
  fileName: string;
  url: string;
  takenAt: string;
}

/** A server of the instance's in-game list (status comes from `api.library.pingServer`). */
export interface SavedServer {
  name: string;
  address: string;
  /** The icon the game saved for it. */
  iconUrl?: string;
}

export interface InstanceSettings {
  javaMode: "auto" | "custom";
  javaPath?: string;
  minMemoryMb: number;
  maxMemoryMb: number;
  /** "Otomatik": the memory follows the computer and the enabled mods. */
  memoryAuto: boolean;
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
  /** The maximum memory "Otomatik" gives this instance now. */
  autoMemoryMb: number;
}

/** Why a connect could not get the AFK client (`pl_afk::ClientProblem`). */
export type AfkClientProblem = "notPublished" | "offline" | "diskFull" | "blocked" | "broken";

export type AfkState =
  | { kind: "stopped" }
  | { kind: "queued"; inSecs: number }
  /**
   * The connect waits for the AFK client: looked up in the content, downloaded (`percent` once its
   * size is known) or unpacked.
   */
  | { kind: "preparing"; step: "checking" | "downloading" | "installing"; percent?: number }
  /** The last connect could not get the AFK client; the account is idle. */
  | { kind: "clientFailed"; problem: AfkClientProblem }
  | { kind: "starting" }
  | { kind: "connecting" }
  | { kind: "connected"; since: string }
  /**
   * `nextAttemptAt` is null until the backend knows when: the client is still closing, or the
   * attempt is starting.
   */
  | { kind: "reconnecting"; attempt: number; nextAttemptAt: string | null }
  | { kind: "stopping" }
  /** `standalone`: the reason says it all, without "Bağlantı kesildi" before it. */
  | { kind: "error"; reason: string; standalone?: boolean };

export interface AfkAccount {
  id: string;
  nickname: string;
  server: string;
  autoStart: boolean;
  /** The game version the client connects with ("auto" or a listed version). */
  mcVersion: string;
  /** `mcVersion` is the recommended version (the row shows no version chip then). */
  mcVersionRecommended: boolean;
  state: AfkState;
  health?: number;
  food?: number;
  players?: number;
  /** The running client has older settings than the saved ones (reconnect to apply). */
  settingsChanged?: boolean;
  /** Show the date next to the time in the console (chat log settings). */
  showDate?: boolean;
  /** The client runs, so the console can send commands and chat. */
  running?: boolean;
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
  /** Started at login: stays in the tray until opened. */
  startMinimized?: boolean;
  /** Installed RAM in MB (for memory sliders). */
  systemMemoryMb?: number;
}
