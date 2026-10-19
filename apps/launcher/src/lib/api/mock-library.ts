// Browser mock of `api.library` (design previews, tests). Deterministic data per instance;
// `mockLive=<instanceId>` in the hash route's query simulates a running game that streams its log.
import buildArt from "@/assets/art/news-build-contest.webp";
import forgeArt from "@/assets/art/news-patch-forge.webp";
import pvpArt from "@/assets/art/news-pvp-arena.webp";
import spawnArt from "@/assets/art/news-spawn-city.webp";
import winterArt from "@/assets/art/news-winter.webp";
import type {
  BackupProgress,
  GameLogSession,
  GraphicsState,
  LibraryApi,
  LogLevel,
  LogLine,
  SavedServerEntry,
  Screenshot,
  ServerEdit,
  TrashItem,
  WorldBackup,
  WorldDetail,
} from "./library-types";
import { mockFreshOfficial } from "./mock-fresh";
import { pick } from "./mock-lang";
import type { SavedServer, ServerStatus } from "./types";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const hoursAgo = (h: number) => new Date(Date.now() - h * 3_600_000).toISOString();
const pad = (n: number) => String(n).padStart(2, "0");

/** Small deterministic generator so previews look the same on every load. */
function seeded(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 4_294_967_296;
  };
}

function hash(text: string): number {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  return h >>> 0;
}

function cancelled(): Error {
  return Object.assign(new Error(pick("İşlem iptal edildi.", "Cancelled.")), {
    code: "IPTAL",
    details: "",
  });
}

function failure(message: string, code: string): Error {
  return Object.assign(new Error(message), { code, details: `${code}: mock` });
}

// --- Logs ------------------------------------------------------------------------------------

type Template = [thread: string, logger: string, message: string];

const infoLines: Template[] = [
  [
    "Render thread",
    "net.minecraft.client.Minecraft",
    "Backend library: LWJGL version 3.3.3-snapshot",
  ],
  [
    "Render thread",
    "net.minecraft.server.packs.resources.ReloadableResourceManager",
    "Reloading ResourceManager: vanilla, fabric, sodium, iris, xaerominimap",
  ],
  [
    "Worker-Main-3",
    "net.minecraft.client.renderer.texture.TextureAtlas",
    "Created: 1024x512x4 minecraft:textures/atlas/blocks.png-atlas",
  ],
  ["Render thread", "net.minecraft.client.sounds.SoundEngine", "Sound engine started"],
  [
    "Server thread",
    "net.minecraft.server.MinecraftServer",
    "Saving chunks for level 'ServerLevel[Yeni Dünya]'/minecraft:overworld",
  ],
  [
    "Render thread",
    "net.minecraft.client.gui.components.ChatComponent",
    "[CHAT] KorAtesi_ joined the game",
  ],
  [
    "Render thread",
    "net.minecraft.client.gui.components.ChatComponent",
    "[CHAT] <KorAtesi_> spawn'a gelin, etkinlik başlıyor",
  ],
  [
    "Render thread",
    "net.minecraft.client.gui.components.ChatComponent",
    "[CHAT] <EjderAvcisi> geliyorum, elmasları bırakayım",
  ],
  [
    "Server thread",
    "net.minecraft.server.level.ServerPlayer",
    "EjderAvcisi has made the advancement [Diamonds!]",
  ],
  [
    "Render thread",
    "net.minecraft.client.multiplayer.ClientPacketListener",
    "Loaded 1290 advancements",
  ],
  ["Render thread", "xaero.common.XaeroMinimapSession", "New world map session initialized!"],
  [
    "Render thread",
    "net.irisshaders.iris.Iris",
    "Using shaderpack: ComplementaryReimagined_r5.4.zip",
  ],
  ["IO-Worker-12", "net.minecraft.util.ChunkStorage", "Saved 214 chunks in 38 ms"],
];

const warnLines: Template[] = [
  [
    "Render thread",
    "net.minecraft.client.renderer.ShaderManager",
    "Shader rendertype_entity_translucent_emissive could not find sampler named Sampler2 in the specified shader program.",
  ],
  [
    "Server thread",
    "net.minecraft.server.MinecraftServer",
    "Can't keep up! Is the server overloaded? Running 2043ms or 40 ticks behind",
  ],
  [
    "Render thread",
    "net.minecraft.client.multiplayer.ClientPacketListener",
    "Received passengers for unknown entity",
  ],
  [
    "main",
    "mixin",
    "Reference map 'xaerominimap.refmap.json' for xaerominimap.mixins.json could not be read. If this is a development environment you can ignore this message",
  ],
];

const errorLines: Template[] = [
  [
    "Download-2",
    "net.minecraft.client.Minecraft",
    "Failed to fetch user properties\ncom.mojang.authlib.exceptions.MinecraftClientHttpException: Status: 401\n\tat com.mojang.authlib.minecraft.client.MinecraftClient.readInputStream(MinecraftClient.java:111)\n\tat com.mojang.authlib.yggdrasil.YggdrasilUserApiService.fetchProperties(YggdrasilUserApiService.java:150)\n\tat java.base/java.lang.Thread.run(Thread.java:1583)",
  ],
  ["IO-Worker-1", "com.mojang.realmsclient.RealmsAvailability", "Couldn't connect to realms"],
  [
    "Render thread",
    "net.minecraft.client.renderer.texture.SpriteLoader",
    "Using missing texture, unable to load minecraft:textures/block/stone_bricks_mossy.png",
  ],
];

const crashLines: Array<[LogLevel, ...Template]> = [
  [
    "error",
    "Render thread",
    "net.minecraft.client.Minecraft",
    'Unreported exception thrown!\njava.lang.NullPointerException: Cannot invoke "net.minecraft.world.entity.Entity.getX()" because "entity" is null\n\tat net.minecraft.client.renderer.entity.EntityRenderDispatcher.render(EntityRenderDispatcher.java:142)\n\tat net.minecraft.client.renderer.LevelRenderer.renderEntity(LevelRenderer.java:1680)\n\tat net.minecraft.client.renderer.GameRenderer.render(GameRenderer.java:1010)\n\tat net.minecraft.client.Minecraft.runTick(Minecraft.java:1257)\n\tat net.minecraft.client.main.Main.main(Main.java:239)',
  ],
  [
    "fatal",
    "Render thread",
    "net.minecraft.client.Minecraft",
    "Game crashed! Crash report saved to: C:\\Users\\********\\AppData\\Roaming\\ProjectsLauncher\\instances\\projects\\minecraft\\crash-reports\\crash-2026-10-15_21.47.03-client.txt",
  ],
];

function clock(secs: number): string {
  const s = Math.floor(secs) % 86_400;
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
}

function entry(level: LogLevel, [thread, logger, message]: Template, secs: number): LogLine {
  return { time: clock(secs), level, thread, logger, message };
}

function openingLines(secs: number): LogLine[] {
  return [
    entry(
      "info",
      ["main", "FabricLoader/GameProvider", "Loading Minecraft 26.1.2 with Fabric Loader 0.19.5"],
      secs,
    ),
    entry(
      "info",
      [
        "main",
        "FabricLoader",
        "Loading 42 mods:\n\t- fabric-api 0.119.2+26.1.2\n\t- fabricloader 0.19.5\n\t- iris 1.8.11+26.1.2\n\t- sodium 0.6.13+26.1.2\n\t- xaerominimap 25.2.10",
      ],
      secs,
    ),
    entry(
      "info",
      ["Render thread", "net.minecraft.client.Minecraft", "Setting user: EjderAvcisi"],
      secs + 9,
    ),
  ];
}

/** A session log: mostly info, some warnings and errors, optionally ending in a crash. */
function makeLog(count: number, seed: number, startSecs: number, crashed: boolean): LogLine[] {
  const random = seeded(seed);
  const out = openingLines(startSecs);
  let secs = startSecs + 12;
  const pick = <T>(list: T[]): T => list[Math.floor(random() * list.length)] as T;
  const end = crashed ? count - crashLines.length : count;
  while (out.length < end) {
    secs += random() < 0.7 ? 0 : random() * 4;
    const roll = random();
    if (roll < 0.006) out.push(entry("error", pick(errorLines), secs));
    else if (roll < 0.06) out.push(entry("warn", pick(warnLines), secs));
    else if (roll < 0.075)
      out.push(
        entry(
          "debug",
          [
            "Render thread",
            "com.mojang.blaze3d.platform.GLX",
            "GL debug: buffer 3 will use VIDEO memory",
          ],
          secs,
        ),
      );
    else out.push(entry("info", pick(infoLines), secs));
  }
  if (crashed) {
    for (const [level, ...template] of crashLines) out.push(entry(level, template, secs + 1));
  }
  return out;
}

interface MockSession extends GameLogSession {
  lines?: LogLine[];
  build: () => LogLine[];
}

function fileStamp(date: Date): string {
  return `${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}-${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`;
}

function sessionFor(
  instanceId: string,
  hoursBack: number,
  count: number,
  crashed: boolean,
): MockSession {
  const started = new Date(Date.now() - hoursBack * 3_600_000);
  const startSecs = started.getHours() * 3600 + started.getMinutes() * 60 + started.getSeconds();
  return {
    fileName: `game-${instanceId}-${fileStamp(started)}.log`,
    source: "launcher",
    startedAt: started.toISOString(),
    updatedAt: new Date(started.getTime() + Math.min(count * 400, 5_400_000)).toISOString(),
    sizeBytes: count * 132,
    live: false,
    build: () => makeLog(count, hash(instanceId) + count, startSecs, crashed),
  };
}

/** One of the game's own log files: `latest.log`, or a dated archive (`2026-10-15-1.log.gz`). */
function gameFileFor(instanceId: string, daysBack: number, count: number, index = 0): MockSession {
  const written = new Date(Date.now() - (daysBack * 24 + 2.3) * 3_600_000);
  const day = new Date(written.getFullYear(), written.getMonth(), written.getDate());
  const archived = daysBack > 0;
  const startSecs = written.getHours() * 3600 + written.getMinutes() * 60;
  return {
    fileName: archived
      ? `${day.getFullYear()}-${pad(day.getMonth() + 1)}-${pad(day.getDate())}-${index}.log.gz`
      : "latest.log",
    source: "game",
    startedAt: archived ? day.toISOString() : undefined,
    updatedAt: written.toISOString(),
    sizeBytes: archived ? Math.round(count * 132 * 0.14) : count * 132,
    live: false,
    build: () => makeLog(count, hash(instanceId) + count + 11, startSecs, false),
  };
}

/** Read once at startup from the hash route's query (`#/library/x?tab=logs&mockLive=x`). */
const liveInstance =
  typeof window === "undefined"
    ? null
    : new URLSearchParams(window.location.hash.split("?")[1] ?? "").get("mockLive");
const sessions = new Map<string, MockSession[]>();

function sessionsOf(instanceId: string): MockSession[] {
  let list = sessions.get(instanceId);
  if (!list) {
    list =
      instanceId === "vanilla"
        ? []
        : [
            sessionFor(instanceId, 2.4, 52_000, true),
            sessionFor(instanceId, 27, 1_240, false),
            sessionFor(instanceId, 76, 860, false),
            gameFileFor(instanceId, 0, 9_800),
            gameFileFor(instanceId, 1, 1_240, 1),
            gameFileFor(instanceId, 3, 860, 2),
          ];
    if (liveInstance === instanceId) {
      const live = sessionFor(instanceId, 0.2, 240, false);
      live.live = true;
      list.unshift(live);
    }
    sessions.set(instanceId, list);
  }
  return list;
}

function linesOf(session: MockSession): LogLine[] {
  session.lines ??= session.build();
  return session.lines;
}

const logListeners = new Set<(instanceId: string, line: LogLine) => void>();
let liveTimer: ReturnType<typeof setInterval> | undefined;
const liveRandom = seeded(7);

function emitLiveLine() {
  if (!liveInstance) return;
  const session = sessionsOf(liveInstance).find((s) => s.live);
  if (!session) return;
  const now = new Date();
  const secs = now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds();
  const roll = liveRandom();
  const template =
    roll < 0.08 ? warnLines[0] : infoLines[Math.floor(liveRandom() * infoLines.length)];
  const line = entry(roll < 0.08 ? "warn" : "info", template as Template, secs);
  linesOf(session).push(line);
  session.sizeBytes += 132;
  for (const listener of logListeners) listener(liveInstance, line);
}

// --- Worlds, backups, trash, screenshots ------------------------------------------------------

interface TrashSlot extends TrashItem {
  instanceId: string;
  restore: () => string;
}

interface InstanceFiles {
  worlds: WorldDetail[];
  backups: WorldBackup[];
  screenshots: Screenshot[];
}

const files = new Map<string, InstanceFiles>();
const trashSlots: TrashSlot[] = [];
const jobs = new Map<string, { cancelled: boolean }>();

function filesOf(instanceId: string): InstanceFiles {
  let entry = files.get(instanceId);
  if (!entry) {
    const empty = instanceId === "vanilla" || (instanceId === "projects" && mockFreshOfficial());
    const art = [spawnArt, buildArt, pvpArt, winterArt, forgeArt];
    entry = {
      worlds: empty
        ? []
        : [
            {
              folder: "Yeni Dünya",
              name: "Yeni Dünya",
              gameMode: "survival",
              hardcore: false,
              cheats: false,
              lastPlayedAt: hoursAgo(3),
              sizeBytes: 182_000_000,
              iconUrl: spawnArt,
              version: "26.1.2",
              readable: true,
            },
            {
              folder: "Yaratıcı test",
              name: "Yaratıcı test",
              gameMode: "creative",
              hardcore: false,
              cheats: true,
              lastPlayedAt: hoursAgo(200),
              sizeBytes: 44_000_000,
              version: "26.1.2",
              readable: true,
            },
            {
              folder: "Hardcore 2",
              name: "Zorlu macera",
              gameMode: "survival",
              hardcore: true,
              cheats: false,
              lastPlayedAt: hoursAgo(900),
              sizeBytes: 96_000_000,
              version: "1.21.1",
              readable: true,
            },
          ],
      backups: empty
        ? []
        : [
            {
              fileName: "2026-10-12_20-11-04_Yeni Dünya.zip",
              sizeBytes: 118_000_000,
              createdAt: hoursAgo(76),
              worldFolder: "Yeni Dünya",
              worldName: "Yeni Dünya",
              valid: true,
            },
            {
              fileName: "2026-09-30_18-40-55_Eski Köy.zip",
              sizeBytes: 61_000_000,
              createdAt: hoursAgo(380),
              worldFolder: "Eski Köy",
              worldName: "Eski köy",
              valid: true,
            },
          ],
      screenshots: empty
        ? []
        : Array.from({ length: 11 }, (_, i) => {
            const taken = new Date(Date.now() - (i * 19 + 2) * 3_600_000);
            const url = art[i % art.length] as string;
            return {
              fileName: `${taken.getFullYear()}-${pad(taken.getMonth() + 1)}-${pad(taken.getDate())}_${pad(taken.getHours())}.${pad(taken.getMinutes())}.${pad(taken.getSeconds())}.png`,
              url,
              // The older half has thumbnails already; the newest are made on first view.
              thumbUrl: i >= 5 ? url : undefined,
              sizeBytes: 2_400_000 + i * 113_000,
              takenAt: taken.toISOString(),
            };
          }),
    };
    files.set(instanceId, entry);
  }
  return entry;
}

// --- Servers ---------------------------------------------------------------------------------

/** Each instance's `servers.dat` (vanilla: empty), edited in place by `editServers`. */
const serverLists = new Map<string, SavedServerEntry[]>();

function serverListOf(instanceId: string): SavedServerEntry[] {
  let list = serverLists.get(instanceId);
  if (!list) {
    const entry = (name: string, ip: string): SavedServerEntry => ({
      name,
      ip,
      icon: null,
      acceptTextures: null,
      hidden: false,
    });
    list =
      instanceId === "vanilla"
        ? []
        : [
            entry("Projects", "play.projects.gg"),
            { ...entry("Doğrudan bağlantı", "192.168.1.20"), hidden: true },
            entry("Hypixel", "mc.hypixel.net"),
            entry("Eski kale sunucusu", "kale.ornek-sunucu.net:25570"),
          ];
    serverLists.set(instanceId, list);
  }
  return list;
}

/** The listed servers of a mock instance, as the instance detail carries them. */
export function mockServers(instanceId: string): SavedServer[] {
  return serverListOf(instanceId)
    .filter((s) => !s.hidden)
    .map((s) => ({ name: s.name, address: s.ip, iconUrl: s.icon ?? undefined }));
}

/** What the mock servers answer: Projects and Hypixel are up, the rest unreachable. */
const mockStatuses: Record<string, Omit<ServerStatus, "name" | "address">> = {
  "play.projects.gg": { online: true, players: 214, maxPlayers: 500, latencyMs: 31 },
  "mc.hypixel.net": { online: true, players: 31_402, maxPlayers: 100_000, latencyMs: 88 },
};

function editMockServers(instanceId: string, edit: ServerEdit): SavedServerEntry | undefined {
  const list = serverListOf(instanceId);
  const listed = () => list.filter((s) => !s.hidden);
  const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();
  const checkAddress = (address: string) => {
    const text = address.trim();
    if (!text || /\s/.test(text) || text.includes("/")) {
      throw failure(
        pick("Bu bir sunucu adresi gibi görünmüyor.", "That doesn't look like a server address."),
        "SUNUCU-ADRES",
      );
    }
    return text;
  };
  const unique = (address: string, except?: SavedServerEntry) => {
    if (listed().some((s) => s !== except && same(s.ip, address))) {
      throw failure(
        pick("Bu sunucu listede zaten var.", "This server is already on the list."),
        "SUNUCU-VAR",
      );
    }
  };
  const find = (index: number, was: string) => {
    const atIndex = listed()[index];
    const found =
      atIndex && same(atIndex.ip, was) ? atIndex : listed().find((s) => same(s.ip, was));
    if (!found) {
      throw failure(
        pick(
          "Sunucu listesi oyunda değişmiş. Liste yenilendi, tekrar dene.",
          "The server list changed in the game. The list is refreshed; try again.",
        ),
        "SUNUCU-DEGISTI",
      );
    }
    return found;
  };
  const nameOf = (name: string, address: string) => (name.trim() || address).slice(0, 64);
  switch (edit.kind) {
    case "add": {
      const address = checkAddress(edit.address);
      unique(address);
      list.push({
        name: nameOf(edit.name, address),
        ip: address,
        icon: null,
        acceptTextures: null,
        hidden: false,
      });
      return undefined;
    }
    case "update": {
      const server = find(edit.index, edit.was);
      const address = checkAddress(edit.address);
      unique(address, server);
      if (!same(server.ip, address)) server.icon = null;
      server.name = nameOf(edit.name, address);
      server.ip = address;
      return undefined;
    }
    case "remove": {
      const server = find(edit.index, edit.was);
      list.splice(list.indexOf(server), 1);
      return { ...server };
    }
    case "restore": {
      const address = checkAddress(edit.server.ip);
      unique(address);
      const before = listed()[edit.index];
      const at = before ? list.indexOf(before) : list.length;
      list.splice(at, 0, { ...edit.server, ip: address, hidden: false });
      return undefined;
    }
  }
}

function freeName(taken: string[], name: string): string {
  if (!taken.includes(name)) return name;
  for (let n = 2; ; n++) if (!taken.includes(`${name} (${n})`)) return `${name} (${n})`;
}

function toTrash(
  instanceId: string,
  kind: TrashItem["kind"],
  name: string,
  fileName: string,
  sizeBytes: number,
  restore: () => string,
): TrashItem {
  const deleted = new Date();
  const slot: TrashSlot = {
    id: `${fileStamp(deleted)}-${Math.random().toString(16).slice(2, 10)}`,
    kind,
    name,
    fileName,
    deletedAt: deleted.toISOString(),
    expiresAt: new Date(deleted.getTime() + 30 * 86_400_000).toISOString(),
    sizeBytes,
    instanceId,
    restore,
  };
  trashSlots.unshift(slot);
  const { instanceId: _owner, restore: _restore, ...item } = slot;
  return item;
}

// --- Graphics presets ------------------------------------------------------------------------

const graphicsStates = new Map<string, GraphicsState>();

/**
 * Each sample instance shows another state: Projects a preset, Vanilla (never played) the first
 * launch's preset, Hardcore its own values ("Özel"). `mockGraphics=custom|first|smooth|balanced|
 * quality|unsupported` in the hash route's query puts every instance in one state (previews).
 */
function graphicsOf(instanceId: string): GraphicsState {
  const known = graphicsStates.get(instanceId);
  if (known) return known;
  const forced =
    typeof window === "undefined"
      ? null
      : new URLSearchParams(window.location.hash.split("?")[1] ?? "").get("mockGraphics");
  const state: GraphicsState =
    forced === "custom" || forced === "unsupported"
      ? { kind: forced }
      : forced === "first"
        ? { kind: "firstLaunch", preset: "smooth" }
        : forced === "smooth" || forced === "balanced" || forced === "quality"
          ? { kind: "preset", preset: forced }
          : instanceId === "vanilla"
            ? { kind: "firstLaunch", preset: "smooth" }
            : instanceId === "hardcore"
              ? { kind: "custom" }
              : { kind: "preset", preset: instanceId === "projects" ? "balanced" : "quality" };
  graphicsStates.set(instanceId, state);
  return state;
}

async function runJob(jobId: string, total: number, onProgress: (p: BackupProgress) => void) {
  const job = { cancelled: false };
  jobs.set(jobId, job);
  try {
    const files = Math.max(24, Math.round(total / 3_000_000));
    for (let step = 0; step <= 20; step++) {
      if (job.cancelled) throw cancelled();
      onProgress({
        filesDone: Math.round((files * step) / 20),
        filesTotal: files,
        bytesDone: Math.round((total * step) / 20),
        bytesTotal: total,
      });
      await delay(120);
    }
  } finally {
    jobs.delete(jobId);
  }
}

export function createMockLibrary(options: {
  setGroup: (instanceId: string, group: string | null) => void;
}): LibraryApi {
  return {
    async logSessions(instanceId) {
      await delay(220);
      return sessionsOf(instanceId).map(({ lines: _lines, build: _build, ...s }) => ({ ...s }));
    },
    async readLog(instanceId, fileName, source = "launcher") {
      await delay(source === "game" && fileName.endsWith(".gz") ? 520 : 320);
      const session = sessionsOf(instanceId).find(
        (s) => s.fileName === fileName && s.source === source,
      );
      if (!session)
        throw failure(pick("Bu kayıt bulunamadı.", "This log wasn't found."), "KAYIT-YOK");
      return { lines: [...linesOf(session)], truncated: false, sizeBytes: session.sizeBytes };
    },
    async shareLog() {
      await delay(1400);
      return { id: "Ab3xK9q", url: "https://mclo.gs/Ab3xK9q" };
    },
    async revealLog() {},
    onGameLog(listener) {
      logListeners.add(listener);
      if (liveInstance && !liveTimer) liveTimer = setInterval(emitLiveLine, 650);
      return () => {
        logListeners.delete(listener);
        if (logListeners.size === 0 && liveTimer) {
          clearInterval(liveTimer);
          liveTimer = undefined;
        }
      };
    },
    async runningInstance() {
      return liveInstance;
    },

    async worlds(instanceId) {
      await delay(260);
      return filesOf(instanceId).worlds.map((w) => ({ ...w }));
    },
    async backups(instanceId) {
      await delay(200);
      return filesOf(instanceId).backups.map((b) => ({ ...b }));
    },
    async backupWorld(instanceId, folder, jobId, onProgress) {
      const world = filesOf(instanceId).worlds.find((w) => w.folder === folder);
      if (!world) throw failure(pick("Dünya bulunamadı.", "The world wasn't found."), "BULUNAMADI");
      await runJob(jobId, world.sizeBytes, onProgress);
      const now = new Date();
      const created: WorldBackup = {
        fileName: `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}_${pad(now.getHours())}-${pad(now.getMinutes())}-${pad(now.getSeconds())}_${folder}.zip`,
        sizeBytes: Math.round(world.sizeBytes * 0.64),
        createdAt: now.toISOString(),
        worldFolder: folder,
        worldName: world.name,
        valid: true,
      };
      filesOf(instanceId).backups.unshift(created);
      return { ...created };
    },
    async restoreBackup(instanceId, fileName, replace, jobId, onProgress) {
      const state = filesOf(instanceId);
      const source = state.backups.find((b) => b.fileName === fileName);
      if (!source?.valid)
        throw failure(
          pick(
            "Bu dosya bir dünya yedeği değil ya da bozuk.",
            "This file isn't a world backup, or it's damaged.",
          ),
          "YEDEK-GECERSIZ",
        );
      await runJob(jobId, Math.round(source.sizeBytes / 0.64), onProgress);
      const folder = source.worldFolder ?? "world";
      let replaced: TrashItem | undefined;
      const existing = state.worlds.find((w) => w.folder === folder);
      if (replace && existing) {
        state.worlds = state.worlds.filter((w) => w !== existing);
        replaced = toTrash(
          instanceId,
          "world",
          existing.name,
          existing.folder,
          existing.sizeBytes,
          () => {
            const name = freeName(
              state.worlds.map((w) => w.folder),
              existing.folder,
            );
            state.worlds.unshift({ ...existing, folder: name });
            return name;
          },
        );
      }
      const target = replace
        ? folder
        : freeName(
            state.worlds.map((w) => w.folder),
            folder,
          );
      state.worlds.unshift({
        folder: target,
        name: source.worldName ?? target,
        gameMode: "survival",
        hardcore: false,
        cheats: false,
        lastPlayedAt: source.createdAt,
        sizeBytes: Math.round(source.sizeBytes / 0.64),
        version: "26.1.2",
        readable: true,
      });
      return { folder: target, replaced };
    },
    async cancelJob(jobId) {
      const job = jobs.get(jobId);
      if (job) job.cancelled = true;
    },
    async deleteWorld(instanceId, folder) {
      await delay(260);
      const state = filesOf(instanceId);
      const world = state.worlds.find((w) => w.folder === folder);
      if (!world) throw failure(pick("Dünya bulunamadı.", "The world wasn't found."), "BULUNAMADI");
      state.worlds = state.worlds.filter((w) => w !== world);
      return toTrash(instanceId, "world", world.name, world.folder, world.sizeBytes, () => {
        const name = freeName(
          state.worlds.map((w) => w.folder),
          world.folder,
        );
        state.worlds.unshift({ ...world, folder: name });
        return name;
      });
    },
    async deleteBackup(instanceId, fileName) {
      await delay(200);
      const state = filesOf(instanceId);
      const found = state.backups.find((b) => b.fileName === fileName);
      if (!found)
        throw failure(pick("Yedek bulunamadı.", "The backup wasn't found."), "BULUNAMADI");
      state.backups = state.backups.filter((b) => b !== found);
      return toTrash(instanceId, "backup", found.fileName, found.fileName, found.sizeBytes, () => {
        state.backups.unshift(found);
        return found.fileName;
      });
    },
    async trash(instanceId) {
      await delay(160);
      return trashSlots
        .filter((t) => t.instanceId === instanceId)
        .map(({ instanceId: _owner, restore: _restore, ...item }) => item);
    },
    async restoreFromTrash(instanceId, id) {
      await delay(240);
      const index = trashSlots.findIndex((t) => t.id === id && t.instanceId === instanceId);
      const slot = trashSlots[index];
      if (!slot)
        throw failure(
          pick("Bu öğe çöp kutusunda yok.", "This item isn't in the trash."),
          "COP-YOK",
        );
      trashSlots.splice(index, 1);
      return slot.restore();
    },
    async deleteFromTrash(instanceId, id) {
      await delay(200);
      const index = trashSlots.findIndex((t) => t.id === id && t.instanceId === instanceId);
      if (index < 0)
        throw failure(
          pick("Bu öğe çöp kutusunda yok.", "This item isn't in the trash."),
          "COP-YOK",
        );
      trashSlots.splice(index, 1);
    },

    async screenshots(instanceId) {
      await delay(240);
      return filesOf(instanceId).screenshots.map((s) => ({ ...s }));
    },
    async screenshotThumb(instanceId, fileName) {
      // Thumbnails take a moment the first time (decoding a big PNG), then they are cached.
      const shot = filesOf(instanceId).screenshots.find((s) => s.fileName === fileName);
      if (!shot) return null;
      if (!shot.thumbUrl) {
        await delay(280 + (hash(fileName) % 600));
        shot.thumbUrl = shot.url;
      }
      return shot.thumbUrl;
    },
    async copyScreenshot() {
      await delay(150);
      return "image";
    },
    async openScreenshot(instanceId, fileName) {
      const shot = filesOf(instanceId).screenshots.find((s) => s.fileName === fileName);
      if (shot) window.open(shot.url, "_blank", "noopener");
    },
    async deleteScreenshot(instanceId, fileName) {
      await delay(180);
      const state = filesOf(instanceId);
      const shot = state.screenshots.find((s) => s.fileName === fileName);
      if (!shot)
        throw failure(
          pick("Ekran görüntüsü bulunamadı.", "The screenshot wasn't found."),
          "BULUNAMADI",
        );
      state.screenshots = state.screenshots.filter((s) => s !== shot);
      return toTrash(instanceId, "screenshot", shot.fileName, shot.fileName, shot.sizeBytes, () => {
        state.screenshots = [...state.screenshots, shot].sort((a, b) =>
          (b.takenAt ?? "").localeCompare(a.takenAt ?? ""),
        );
        return shot.fileName;
      });
    },
    async revealFile() {},

    async setGroup(instanceId, group) {
      await delay(120);
      options.setGroup(instanceId, group);
    },
    async diskUsage(instanceId, signal) {
      // The worlds, backups and screenshots, plus mods, settings and the other versions' sets.
      await delay(650);
      if (signal?.aborted) throw failure(pick("İşlem iptal edildi.", "Cancelled."), "IPTAL");
      const state = filesOf(instanceId);
      const sum = (items: Array<{ sizeBytes: number }>) =>
        items.reduce((total, item) => total + item.sizeBytes, 0);
      const rest =
        instanceId === "projects" && mockFreshOfficial()
          ? 892
          : instanceId === "vanilla"
            ? 6_400_000
            : 412_000_000 + (hash(instanceId) % 9) * 37_000_000;
      return sum(state.worlds) + sum(state.backups) + sum(state.screenshots) + rest;
    },

    async pingServer(address) {
      const known = mockStatuses[address.trim().toLowerCase()];
      // Unreachable servers take the whole timeout, like the real ping.
      await delay(known ? 260 + (hash(address) % 500) : 1800);
      return known
        ? { name: address, address, ...known }
        : { name: address, address, online: false, players: 0, maxPlayers: 0 };
    },
    async editServers(instanceId, edit) {
      await delay(160);
      if (liveInstance === instanceId) {
        throw failure(
          pick(
            "Oyun açıkken sunucu listesi değiştirilemez. Önce oyunu kapat.",
            "You can't change the server list while the game is open. Close the game first.",
          ),
          "OYUN-ACIK",
        );
      }
      const removed = editMockServers(instanceId, edit);
      return { servers: mockServers(instanceId), removed };
    },

    async graphics(instanceId) {
      await delay(40);
      return graphicsOf(instanceId);
    },
    async setGraphics(instanceId, preset) {
      await delay(180);
      if (liveInstance === instanceId) {
        throw failure(
          pick(
            "Oyun açıkken grafik ayarı değiştirilemez. Önce oyunu kapat.",
            "You can't change the graphics setting while the game is open. Close the game first.",
          ),
          "OYUN-ACIK",
        );
      }
      const next: GraphicsState =
        graphicsOf(instanceId).kind === "firstLaunch"
          ? { kind: "firstLaunch", preset }
          : { kind: "preset", preset };
      graphicsStates.set(instanceId, next);
      return next;
    },
  };
}
