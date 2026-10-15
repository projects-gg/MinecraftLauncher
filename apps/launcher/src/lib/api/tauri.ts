import { Channel, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import heroFallback from "@/assets/art/hero-dragon-1.webp";
import {
  type AccountView,
  type AfkAccountView,
  type AfkEvent,
  type LoginStep as BindingLoginStep,
  type CmdError,
  commands,
  type FailureReason,
  type HomeView,
  type InstanceView,
  type LaunchEventView,
} from "@/bindings";
import type { LauncherApi } from "./index";
import { mockApi } from "./mock";
import type {
  AccountSummary,
  AfkAccount,
  AfkState,
  HomeContent,
  InstanceIcon,
  InstanceSummary,
  LaunchEvent,
  NewsCategory,
  ParticleKind,
} from "./types";

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
  };
}

function account(v: AccountView): AccountSummary {
  return {
    id: v.id,
    kind: v.kind,
    username: v.username,
    active: v.active,
    // Premium players get their real face; offline names fall back to the lettered tile.
    headUrl: v.kind === "microsoft" ? `https://mc-heads.net/avatar/${v.uuid}/64` : undefined,
  };
}

function home(v: HomeView): HomeContent {
  return {
    hero: {
      imageUrl: fileUrl(v.hero.imagePath) ?? heroFallback,
      videoUrl: fileUrl(v.hero.videoPath),
      title: v.hero.title,
      subtitle: v.hero.subtitle,
      newsId: v.hero.newsId ?? undefined,
      particles: (v.hero.particles || "embers") as ParticleKind,
    },
    banners: v.banners.map((b) => ({
      id: b.id,
      level: b.level as HomeContent["banners"][number]["level"],
      text: b.text,
      url: b.url ?? undefined,
    })),
    events: v.events,
    news: v.news.map((n) => ({
      id: n.id,
      title: n.title,
      summary: n.summary,
      body: n.body,
      coverUrl: fileUrl(n.coverPath),
      category: (n.category || "announcement") as NewsCategory,
      publishedAt: n.publishedAt,
      pinned: n.pinned,
      featured: n.featured,
      cta: n.ctaLabel && n.ctaUrl ? { label: n.ctaLabel, url: n.ctaUrl } : undefined,
    })),
    offline: v.offline,
  };
}

function launchEvent(e: LaunchEventView): LaunchEvent {
  switch (e.type) {
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
      };
    case "error":
      return { type: "error", message: e.message, details: `${e.code}\n${e.details}` };
    case "exited":
      return { type: "exited", code: e.code, runtimeSecs: e.runtimeSecs };
    case "started":
      return { type: "started", pid: e.pid };
    default:
      return { type: e.type };
  }
}

// --- AFK -------------------------------------------------------------------------------------

const failureText: Record<string, string> = {
  kicked: "Sunucudan atıldı",
  loginRejected: "Sunucu girişi reddetti",
  connectionLost: "Bağlantı koptu",
  loggedOut: "Oturum kapatıldı",
  disconnected: "Bağlantı kesildi",
  unsupportedVersion: "Sürüm desteklenmiyor",
};

function failure(reason: FailureReason | null | undefined): string {
  if (!reason) return "Bağlantı kesildi";
  const base = failureText[reason.kind] ?? "Bağlantı kesildi";
  const message = "message" in reason ? reason.message : null;
  return message ? `${base} (${message})` : base;
}

/** Live "queued for N seconds" countdowns from bulk connects, keyed by account id. */
const afkCountdowns = new Map<string, number>();

function afkState(v: AfkAccountView): AfkState {
  const queued = afkCountdowns.get(v.id);
  if (queued && v.state.state === "stopped") return { kind: "queued", inSecs: queued };
  const s = v.state;
  switch (s.state) {
    case "connected":
      return { kind: "connected", since: s.since };
    case "reconnecting":
      return {
        kind: "reconnecting",
        attempt: s.attempt,
        nextAttemptAt: s.nextAttemptAt ?? new Date(Date.now() + 15_000).toISOString(),
      };
    case "error":
      return { kind: "error", reason: failure(s.reason) };
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
    state: afkState(v),
    health: v.stats.hp === null ? undefined : Math.round(v.stats.hp),
    food: v.stats.food ?? undefined,
    players: v.stats.players ?? undefined,
  };
}

export const tauriApi: LauncherApi = {
  ...mockApi,

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
    return view ? home(view) : { ...(await mockApi.homeContent()), offline: false };
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
      },
      mods: d.mods.map((m) => ({ ...m, iconUrl: m.iconUrl ?? undefined })),
      resourcePacks: d.resourcePacks.map((p) => ({ ...p, iconUrl: p.iconUrl ?? undefined })),
      shaderPacks: d.shaderPacks.map((p) => ({ ...p, iconUrl: p.iconUrl ?? undefined })),
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
      servers: d.servers,
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
    await ok(commands.instanceDelete(id));
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
    }));
  },

  async accounts() {
    return (await commands.accountsList()).map(account);
  },
  async addOfflineAccount(name) {
    return account(await ok(commands.accountAddOffline(name)));
  },
  async loginMicrosoft(onStep) {
    const channel = new Channel<BindingLoginStep>();
    channel.onmessage = onStep;
    return account(await ok(commands.accountLoginMicrosoft(channel)));
  },
  async cancelMicrosoftLogin() {
    await commands.accountCancelLogin();
  },
  async setActiveAccount(id) {
    await ok(commands.accountSetActive(id));
  },
  async removeAccount(id) {
    await ok(commands.accountRemove(id));
  },

  async launch(instanceId, onEvent) {
    const channel = new Channel<LaunchEventView>();
    channel.onmessage = (e) => onEvent(launchEvent(e));
    const autoJoin = JSON.parse(localStorage.getItem("pl.prefs") ?? "{}")?.state?.autoJoin ?? true;
    await ok(commands.launchInstance(instanceId, autoJoin, channel));
  },
  async cancelLaunch() {
    await commands.launchCancel();
  },
};
