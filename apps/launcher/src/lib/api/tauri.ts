import { Channel, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openUrl } from "@tauri-apps/plugin-opener";
import heroFallback from "@/assets/art/hero-dragon-1.webp";
import newsBuildContest from "@/assets/art/news-build-contest.webp";
import newsPatchForge from "@/assets/art/news-patch-forge.webp";
import newsPvpArena from "@/assets/art/news-pvp-arena.webp";
import newsSpawnCity from "@/assets/art/news-spawn-city.webp";
import {
  type AccountView,
  type AfkAccountView,
  type AfkEvent,
  type LoginStep as BindingLoginStep,
  type CmdError,
  type ContentStatusView,
  commands,
  type DroppedFileView,
  type HomeView,
  type InstanceView,
  type LaunchEventView,
} from "@/bindings";
import i18n from "@/lib/i18n";
import { highlights, newsHighlights } from "@/lib/news";
import { fileNameOf } from "@/lib/paths";
import { clientState, failureText } from "./afk-client";
import type { AfkSettingsSaveResult, AfkSettingsView } from "./afk-settings";
import { tauriCleanup } from "./cleanup-tauri";
import { tauriContentApi } from "./content-tauri";
import { tauriData } from "./data-tauri";
import { cssPoint } from "./drag-point";
import { tauriImport } from "./import-tauri";
import type { LauncherApi } from "./index";
import { javaPrompt, tauriJava } from "./java-tauri";
import { tauriLibrary } from "./library-tauri";
import { mockApi } from "./mock";
import { tauriModCenter } from "./modcenter-tauri";
import { modCheck, tauriModHealth } from "./modhealth-tauri";
import { tauriModSets } from "./modsets-tauri";
import { tauriSkins } from "./skins-tauri";
import { tauriSocial } from "./social-tauri";
import type {
  AccountSummary,
  AfkAccount,
  AfkState,
  DeepLink,
  DroppedFile,
  HomeContent,
  HomeStatus,
  InstanceIcon,
  InstanceSummary,
  LaunchEvent,
  MinecraftNewsItem,
  NewsCategory,
  ParticleKind,
} from "./types";
import { welcomeHome } from "./welcome";

/** A command failed: carries the friendly message plus details for "copy details". */
export class CommandError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(error: CmdError) {
    super(error.message);
    this.code = error.code;
    this.details = error.details;
  }
}

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new CommandError(result.error);
  return result.data;
}

const fileUrl = (path: string | null | undefined) => (path ? convertFileSrc(path) : undefined);

/**
 * The persisted preferences (zustand `pl.prefs`), read directly so this module does not depend
 * on the stores. Missing or unreadable storage gives defaults.
 */
function storedPrefs(): {
  autoJoin?: boolean;
  concurrentDownloads?: number;
  verifyOnLaunch?: boolean;
  copyScreenshots?: boolean;
  defaultMaxMemoryMb?: number | null;
  defaultJvmArgs?: string;
} {
  try {
    return JSON.parse(localStorage.getItem("pl.prefs") ?? "{}")?.state ?? {};
  } catch {
    return {};
  }
}

/** Bundled art for news published without a cover, by category. */
const fallbackCovers: Record<string, string> = {
  update: newsPatchForge,
  patchNotes: newsPatchForge,
  event: newsPvpArena,
  community: newsBuildContest,
  announcement: newsSpawnCity,
};

function instance(v: InstanceView): InstanceSummary {
  return {
    id: v.id,
    name: v.name,
    icon: v.icon as InstanceIcon,
    mcVersion: v.mcVersion,
    loader: v.loader,
    loaderVersion: v.loaderVersion ?? undefined,
    managed: v.managed,
    lastPlayedAt: v.lastPlayedAt ?? undefined,
    playTimeSecs: v.playTimeSecs,
    state: v.state as InstanceSummary["state"],
    autoJoin: v.autoJoin ?? undefined,
    group: v.group ?? undefined,
    createdAt: v.createdAt,
  };
}

function account(v: AccountView): AccountSummary {
  return {
    id: v.id,
    kind: v.kind,
    username: v.username,
    active: v.active,
    // Cut by the backend from the account's own skin (Mojang texture, or the default skin of an
    // offline name from a local client jar); no avatar service is ever asked.
    headUrl: v.headUrl ?? undefined,
    addedAt: v.addedAt,
    lastUsedAt: v.lastUsedAt ?? undefined,
    needsRelogin: v.needsRelogin,
    pinned: v.pinned,
    avatar: v.avatar ?? undefined,
  };
}

function home(v: HomeView): HomeContent {
  const heroImage = fileUrl(v.hero.imagePath) ?? heroFallback;
  const news = v.news.map((n) => ({
    id: n.id,
    title: n.title,
    summary: n.summary,
    body: n.body,
    // A feed post without a picture wears the season's art; Studio posts keep the bundled
    // art of their category.
    coverUrl:
      fileUrl(n.coverPath) ??
      (n.fromFeed ? heroImage : (fallbackCovers[n.category] ?? newsSpawnCity)),
    category: (n.category || "announcement") as NewsCategory,
    publishedAt: n.publishedAt,
    pinned: n.pinned,
    featured: n.featured,
    cta: n.ctaLabel && n.ctaUrl ? { label: n.ctaLabel, url: n.ctaUrl } : undefined,
    fromFeed: n.fromFeed,
    sourceUrl: n.sourceUrl ?? undefined,
  }));
  const notesItem = v.patchNotesId ? news.find((n) => n.id === v.patchNotesId) : undefined;
  const minecraft = v.minecraft.map((m) => ({
    id: m.id,
    version: m.version,
    kind: (m.kind === "release" ? "release" : "snapshot") as MinecraftNewsItem["kind"],
    title: m.title,
    summary: m.summary,
    body: m.body,
    // Its BlurHash stands in while the cover downloads; an entry without a cover wears the
    // season's art, like a feed post without a picture.
    coverUrl: fileUrl(m.coverPath) ?? (m.coverBlurhash ? undefined : heroImage),
    coverBlurhash: m.coverBlurhash ?? undefined,
    publishedAt: m.publishedAt,
    expiresAt: m.expiresAt,
    url: m.url,
    en: m.en
      ? {
          title: m.en.title,
          summary: m.en.summary ?? undefined,
          body: m.en.body ?? undefined,
        }
      : undefined,
  }));
  return {
    hero: {
      imageUrl: heroImage,
      videoUrl: fileUrl(v.hero.videoPath),
      title: v.hero.title,
      subtitle: v.hero.subtitle,
      newsId: v.hero.newsId ?? undefined,
      ctaLabel: v.hero.ctaLabel?.trim() || undefined,
      ctaUrl: v.hero.ctaUrl ?? undefined,
      particles: (v.hero.particles || "embers") as ParticleKind,
    },
    banners: v.banners.map((b) => ({
      id: b.id,
      // A level added by a newer Studio reads like "info".
      level: (["info", "warning", "event", "critical"].includes(b.level)
        ? b.level
        : "info") as HomeContent["banners"][number]["level"],
      text: b.text,
      url: b.url ?? undefined,
      dismissible: b.dismissible,
      endsAt: b.endsAt ?? undefined,
    })),
    events: v.events.map((e) => ({
      id: e.id,
      title: e.title,
      startsAt: e.startsAt,
      endsAt: e.endsAt,
      openEnded: e.openEnded,
      newsId: e.newsId ?? undefined,
      url: e.url ?? undefined,
      priority: e.priority,
      join: e.join,
      fromFeed: e.fromFeed,
    })),
    news,
    minecraft,
    maintenance: v.maintenance
      ? {
          title: v.maintenance.title,
          message: v.maintenance.message,
          until: v.maintenance.until ?? undefined,
        }
      : undefined,
    status: v.status
      ? {
          // A level added by a newer bot reads like a notice.
          level: (["outage", "degraded", "notice"].includes(v.status.level)
            ? v.status.level
            : "notice") as HomeStatus["level"],
          title: v.status.title,
          message: v.status.message,
          url: v.status.url ?? undefined,
          expiresAt: v.status.expiresAt,
        }
      : undefined,
    patchNotes: notesItem
      ? {
          kind: "news",
          newsId: notesItem.id,
          title: notesItem.title,
          publishedAt: notesItem.publishedAt,
          highlights: newsHighlights(notesItem.body, notesItem.summary),
        }
      : v.packNotes
        ? {
            kind: "pack",
            packName: v.packNotes.name,
            version: v.packNotes.version,
            publishedAt: v.packNotes.publishedAt,
            highlights: highlights(v.packNotes.changelog),
            changelog: v.packNotes.changelog,
          }
        : undefined,
    links: {
      website: v.links.website ?? undefined,
      discord: v.links.discord ?? undefined,
      store: v.links.store ?? undefined,
      support: v.links.support ?? undefined,
      custom: v.links.custom,
      community: v.links.community,
    },
    offline: v.offline,
  };
}

function launchEvent(e: LaunchEventView): LaunchEvent {
  switch (e.type) {
    case "packUpdated":
      return {
        type: "packUpdated",
        name: e.name,
        version: e.version,
        changelog: e.changelog ?? undefined,
        changedFiles: e.changedFiles,
      };
    case "progress":
      return {
        type: "progress",
        stage: e.stage as Extract<LaunchEvent, { type: "progress" }>["stage"],
        percent: e.percent ?? 0,
        doneFiles: e.doneFiles,
        totalFiles: e.totalFiles,
        bytesPerSec: e.bytesPerSec,
        etaSecs: e.etaSecs ?? undefined,
      };
    case "crashed":
      return {
        type: "crashed",
        title: e.title,
        hint: e.hint,
        actions: e.actions,
        mods: e.mods,
        logFile: e.logFile,
        crashReport: e.crashReport,
        kind: e.kind,
        instanceId: e.instanceId,
        suspects: e.suspects.map((s) => ({ ...s, related: s.related ?? undefined })),
        repairable: e.repairable,
        turnOff: e.turnOff,
        reinstall: e.reinstall,
        memory: e.memory,
        resetJvmArgs: e.resetJvmArgs,
      };
    case "javaMismatch": {
      const { type, ...prompt } = e;
      return { type, ...javaPrompt(prompt) };
    }
    case "modsBlocked":
    case "modsWarning":
    case "keyClash": {
      const { type, ...check } = e;
      return { type, check: modCheck(check) };
    }
    case "error":
      return { type: "error", message: e.message, details: `${e.code}\n${e.details}` };
    case "exited":
      return { type: "exited", code: e.code, runtimeSecs: e.runtimeSecs };
    case "started":
      return { type: "started", pid: e.pid, firstStart: e.firstStart };
    case "modsNotice":
      return {
        type: "modsNotice",
        changes: e.changes,
        reset: e.reset,
        issues: e.issues,
      };
    case "packVersionDropped":
      return {
        type: "packVersionDropped",
        minecraft: e.minecraft,
        recommended: e.recommended ?? undefined,
      };
    case "newScreenshots":
      // The backend allowed their folder in the asset protocol for the thumbnails.
      return {
        type: "newScreenshots",
        count: e.count,
        recent: e.recent.map((path) => convertFileSrc(path)),
        newest: fileNameOf(e.recent[0] ?? ""),
      };
    default:
      return { type: e.type };
  }
}

function dropped(file: DroppedFileView): DroppedFile {
  return { path: file.path, name: file.name, kind: file.kind };
}

// --- AFK -------------------------------------------------------------------------------------

/** Live "queued for N seconds" countdowns from bulk connects, keyed by account id. */
const afkCountdowns = new Map<string, number>();

/**
 * The account's state for the cards. Why an account stopped is worded in the interface's language
 * (`failureText`); the AFK list is read again when the language changes (main.tsx), so the words
 * follow it.
 */
function afkState(v: AfkAccountView): AfkState {
  const queued = afkCountdowns.get(v.id);
  if (queued && v.state.state === "stopped") return { kind: "queued", inSecs: queued };
  const client = clientState(v.client, v.state.state);
  if (client) return client;
  const s = v.state;
  switch (s.state) {
    case "connected":
      return { kind: "connected", since: s.since };
    case "reconnecting":
      return { kind: "reconnecting", attempt: s.attempt, nextAttemptAt: s.nextAttemptAt };
    case "error":
      return { kind: "error", ...failureText(s.reason, (key) => i18n.t(key)) };
    default:
      return { kind: s.state };
  }
}

function afkAccount(v: AfkAccountView): AfkAccount {
  return {
    id: v.id,
    nickname: v.nickname,
    server: v.server,
    autoStart: v.autoStart,
    mcVersion: v.mcVersion,
    mcVersionRecommended: v.mcVersionRecommended,
    state: afkState(v),
    health: v.stats.hp === null ? undefined : Math.round(v.stats.hp),
    food: v.stats.food ?? undefined,
    players: v.stats.players ?? undefined,
    settingsChanged: v.settingsChanged,
    showDate: v.showDate,
    running: v.running,
  };
}

export const tauriApi: LauncherApi = {
  ...mockApi,
  content: tauriContentApi,
  skins: tauriSkins,
  library: tauriLibrary,
  data: tauriData,
  cleanup: tauriCleanup,
  modCenter: tauriModCenter,
  modSets: tauriModSets,
  social: tauriSocial,
  modHealth: tauriModHealth,
  java: tauriJava,
  importer: tauriImport,

  async afkAccounts() {
    return (await commands.afkOverview()).map(afkAccount);
  },
  async afkAdd(nickname) {
    await ok(commands.afkAdd(nickname));
    const added = (await commands.afkOverview()).find((a) => a.nickname === nickname);
    if (!added) throw new Error("account not found after add");
    return afkAccount(added);
  },
  async afkRemove(id) {
    await ok(commands.afkRemove(id));
  },
  async afkConnect(id) {
    await ok(commands.afkConnect(id));
  },
  async afkStop(id) {
    await commands.afkDisconnect(id);
  },
  async afkConnectAll() {
    await commands.afkConnectAll();
  },
  async afkStopAll() {
    await ok(commands.afkStopAll());
  },
  async afkSend(id, line) {
    await ok(commands.afkSend(id, line));
  },
  async afkLog(id) {
    return (await commands.afkConsole(id)).map((l, index) => ({
      id: index,
      time: l.time,
      text: l.text,
      kind: l.kind === "script" ? ("script" as const) : ("chat" as const),
    }));
  },
  // The backend always sends complete pages (see afk-settings.ts for why the casts are safe).
  async afkSettings(id) {
    return (await ok(commands.afkSettings(id))) as AfkSettingsView;
  },
  async afkSaveSettings(id, settings) {
    return (await ok(commands.afkSaveSettings(id, settings))) as AfkSettingsSaveResult;
  },
  async afkCheckPattern(pattern, isRegex) {
    return commands.afkCheckPattern(pattern, isRegex);
  },
  async afkReconnect(id) {
    await ok(commands.afkReconnect(id));
  },
  async afkGlobalSettings() {
    return { showScriptCommands: false, ...(await commands.afkGlobalSettings()) };
  },
  async afkSetShowScriptCommands(show) {
    await ok(commands.afkSetShowScriptCommands(show));
  },
  async afkOpenFolder(id) {
    await ok(commands.afkOpenFolder(id));
  },
  onAfkChange(listener) {
    let pending = false;
    const unlisten = listen<AfkEvent>("afk-event", (event) => {
      const { accountId, kind } = event.payload;
      if (kind.type === "countdown") {
        if (kind.data.secs > 0) afkCountdowns.set(accountId, kind.data.secs);
        else afkCountdowns.delete(accountId);
      }
      // Coalesce bursts (log lines arrive fast) into one refresh per frame.
      if (!pending) {
        pending = true;
        requestAnimationFrame(() => {
          pending = false;
          listener();
        });
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  appInfo: () => commands.appInfo(),
  openUrl: (url) => openUrl(url),

  async homeContent() {
    const view = await ok(commands.contentHome());
    // Nothing published or cached yet: show the bundled welcome content.
    return view ? home(view) : welcomeHome();
  },
  async serverStatus() {
    const s = await commands.serverStatus(null, null);
    return {
      name: s.name,
      address: s.address,
      online: s.online,
      players: s.players,
      maxPlayers: s.maxPlayers,
      latencyMs: s.latencyMs ?? undefined,
    };
  },

  async instances() {
    return (await ok(commands.instancesList())).map(instance);
  },
  async instance(id) {
    const d = await ok(commands.instanceDetail(id));
    return {
      ...instance(d),
      settings: {
        ...d.settings,
        javaMode: d.settings.javaMode as "auto" | "custom",
        javaPath: d.settings.javaPath ?? undefined,
        memoryAuto: d.settings.memoryAuto ?? false,
      },
      autoMemoryMb: d.autoMemoryMb,
      // Modrinth icons are cached on disk for files without an icon of their own.
      mods: d.mods.map((m) => ({
        ...m,
        iconUrl: m.iconUrl ?? fileUrl(m.iconPath),
        projectId: m.projectId ?? undefined,
        itemId: m.itemId ?? undefined,
        modSet: m.modSet ?? undefined,
      })),
      resourcePacks: d.resourcePacks.map((p) => ({
        ...p,
        iconUrl: p.iconUrl ?? fileUrl(p.iconPath),
        projectId: p.projectId ?? undefined,
        modSet: p.modSet ?? undefined,
      })),
      shaderPacks: d.shaderPacks.map((p) => ({
        ...p,
        iconUrl: p.iconUrl ?? fileUrl(p.iconPath),
        projectId: p.projectId ?? undefined,
        modSet: p.modSet ?? undefined,
      })),
      worlds: d.worlds.map((w) => ({
        ...w,
        gameMode: w.gameMode as "survival",
        iconUrl: w.iconUrl ?? undefined,
      })),
      screenshots: d.screenshots.map((s) => ({
        fileName: s.fileName,
        url: convertFileSrc(s.path),
        takenAt: s.takenAt,
      })),
      servers: d.servers.map((s) => ({
        name: s.name,
        address: s.address,
        iconUrl: s.iconUrl ?? undefined,
      })),
      systemMemoryMb: d.systemMemoryMb,
    };
  },
  async createInstance(spec) {
    return instance(
      await ok(
        commands.instanceCreate({
          name: spec.name,
          icon: spec.icon,
          mcVersion: spec.mcVersion,
          loader: spec.loader,
          loaderVersion: spec.loaderVersion ?? null,
          // Settings → Oyun defaults for new instances.
          maxMemoryMb: storedPrefs().defaultMaxMemoryMb ?? null,
          jvmArgs: storedPrefs().defaultJvmArgs || null,
        }),
      ),
    );
  },
  async updateInstanceSettings(id, settings) {
    await ok(
      commands.instanceUpdateSettings(id, {
        ...settings,
        javaPath: settings.javaPath ?? null,
      }),
    );
  },
  async setModEnabled(instanceId, fileName, enabled) {
    await ok(commands.modSetEnabled(instanceId, fileName, enabled));
  },
  async openInstanceFolder(id, sub) {
    await ok(commands.instanceOpenFolder(id, sub ?? null));
  },
  async duplicateInstance(id) {
    return instance(await ok(commands.instanceDuplicate(id)));
  },
  async deleteInstance(id) {
    return ok(commands.instanceDelete(id));
  },
  async restoreInstance(trashId) {
    return instance(await ok(commands.instanceRestore(trashId)));
  },
  async loaderVersions(loader, mcVersion) {
    return (await ok(commands.loaderVersions(loader, mcVersion))).map((v) => ({
      version: v.version,
      stable: v.stable,
    }));
  },
  async updatePresence(context) {
    await commands.discordUpdate(context);
  },
  async setPresenceEnabled(enabled) {
    await commands.discordSetEnabled(enabled);
  },
  async setLanguage(lang, pick) {
    await commands.appSetLanguage(lang, pick);
  },
  async legacyScan() {
    return ok(commands.legacyScan());
  },
  async legacyImport(request) {
    return ok(commands.legacyImport(request));
  },
  async minecraftVersions() {
    return (await ok(commands.minecraftVersions())).map((v) => ({
      id: v.id,
      kind: v.kind as "release",
      releaseTime: v.releaseTime,
      recommended: v.recommended,
    }));
  },

  async accounts() {
    return (await commands.accountsList()).map(account);
  },
  async addOfflineAccount(name) {
    return account(await ok(commands.accountAddOffline(name)));
  },
  async loginMicrosoft(onStep, options) {
    const channel = new Channel<BindingLoginStep>();
    channel.onmessage = onStep;
    const result = await ok(commands.accountLoginMicrosoft(channel, options?.relogin ?? null));
    return { account: account(result.account), outcome: result.outcome };
  },
  async cancelMicrosoftLogin() {
    await commands.accountCancelLogin();
  },
  async setActiveAccount(id) {
    await ok(commands.accountSetActive(id));
  },
  async removeAccount(id) {
    const removal = await ok(commands.accountRemove(id));
    return { activeId: removal.activeId ?? null };
  },
  async undoRemoveAccount(id) {
    return account(await ok(commands.accountUndoRemove(id)));
  },
  async forgetRemovedAccount(id) {
    await ok(commands.accountForgetRemoved(id));
  },
  async renameAccount(id, name) {
    return account(await ok(commands.accountRename(id, name)));
  },
  async setAccountPinned(id, pinned) {
    return account(await ok(commands.accountSetPinned(id, pinned)));
  },
  async reorderPinnedAccounts(ids) {
    await ok(commands.accountsReorderPinned(ids));
  },
  async setAccountAvatar(id, avatar) {
    return account(await ok(commands.accountSetAvatar(id, avatar)));
  },
  async checkAccountSession(id) {
    return ok(commands.accountCheckSession(id));
  },
  async ensureAccountHeads() {
    return ok(commands.accountsEnsureHeads());
  },

  async launch(instanceId, onEvent, options) {
    const channel = new Channel<LaunchEventView>();
    channel.onmessage = (e) => onEvent(launchEvent(e));
    const prefs = storedPrefs();
    await ok(
      commands.launchInstance(
        instanceId,
        options?.autoJoin ?? prefs.autoJoin ?? true,
        {
          concurrentDownloads: prefs.concurrentDownloads ?? 16,
          verifyFiles: prefs.verifyOnLaunch ?? false,
          ignoreModIssues: options?.ignoreModIssues ?? false,
          quickPlay: options?.quickPlay ?? null,
          java: options?.java ?? null,
          copyScreenshots: prefs.copyScreenshots ?? false,
        },
        channel,
      ),
    );
  },
  async cancelLaunch() {
    await commands.launchCancel();
  },
  async setScreenshotCopy(enabled) {
    await commands.gameCopyScreenshots(enabled);
  },

  async officialStatus() {
    const s = await ok(commands.officialStatus());
    return {
      pack: s.pack ? { ...s.pack, summary: s.pack.summary ?? undefined } : undefined,
      instanceId: s.instanceId ?? undefined,
    };
  },
  async ensureOfficialInstance() {
    return instance(await ok(commands.officialInstanceEnsure()));
  },
  onContentChanged(listener) {
    const unlisten = listen("content-changed", () => listener());
    // The refresh that follows the first home request (content, then the news feed) can finish
    // before this subscription is in place: catch up once as soon as it is.
    void unlisten.then(() => listener());
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  async contentStatus() {
    return commands.contentStatus();
  },
  onContentStatus(listener) {
    const unlisten = listen<ContentStatusView>("content-status", (event) =>
      listener(event.payload),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  async contentRecheck() {
    await commands.contentRecheck();
  },
  onDeepLink(listener) {
    const unlisten = listen<DeepLink>("deep-link", (event) => listener(event.payload));
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  async takePendingDeepLink() {
    return (await commands.deepLinkPending()) ?? null;
  },
  async quitApp() {
    await commands.appQuit();
  },
  onFileDrag(listener) {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      // Physical pixels from the webview's corner; the scale can change with the monitor.
      const at = (position: { x: number; y: number }) =>
        cssPoint(position, window.devicePixelRatio);
      switch (payload.type) {
        case "enter":
          listener({ type: "enter", paths: payload.paths, position: at(payload.position) });
          break;
        case "over":
          listener({ type: "over", position: at(payload.position) });
          break;
        case "drop":
          listener({ type: "drop", paths: payload.paths, position: at(payload.position) });
          break;
        default:
          listener({ type: "leave" });
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  async classifyFiles(paths) {
    return (await commands.filesClassify(paths)).map(dropped);
  },
  async dropFiles(instanceId, paths) {
    const r = await ok(commands.filesDrop(instanceId, paths));
    return {
      added: r.added.map(dropped),
      existing: r.existing.map(dropped),
      unknown: r.unknown.map(dropped),
      needsLoader: r.needsLoader.map(dropped),
      busy: r.busy.map(dropped),
      modpacks: r.modpacks.map(dropped),
      failed: r.failed.map(dropped),
    };
  },
  async resumeTarget(instanceId) {
    const target = await ok(commands.resumeTarget(instanceId));
    if (!target) return null;
    return {
      kind: target.kind,
      name: target.name,
      lastPlayedAt: target.lastPlayedAt ?? undefined,
      iconUrl: target.iconUrl ?? undefined,
      quickPlay: target.quickPlay,
    };
  },
  async createShortcut(instanceId, iconPng) {
    await ok(commands.instanceShortcutCreate(instanceId, iconPng ?? null));
  },
};
