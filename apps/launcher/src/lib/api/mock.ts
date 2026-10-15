// Realistic sample data for browser previews and for the UI before each backend command lands.
import heroArt from "@/assets/art/hero-dragon-1.webp";
import coverBuild from "@/assets/art/news-build-contest.webp";
import coverForge from "@/assets/art/news-patch-forge.webp";
import coverPvp from "@/assets/art/news-pvp-arena.webp";
import coverSpawn from "@/assets/art/news-spawn-city.webp";
import coverWinter from "@/assets/art/news-winter.webp";
import type { LauncherApi } from "./index";
import { mockAfk } from "./mock-afk";
import type {
  AccountSummary,
  HomeContent,
  InstanceDetail,
  InstanceSummary,
  LaunchEvent,
  LaunchStage,
  LoaderKind,
  LoaderVersion,
  LoginStep,
  McVersion,
  ModEntry,
  NewInstance,
  ServerStatus,
} from "./types";

const hoursAgo = (h: number) => new Date(Date.now() - h * 3_600_000).toISOString();

function nextSaturdayEvening(): Date {
  const d = new Date();
  d.setDate(d.getDate() + ((6 - d.getDay() + 7) % 7 || 7));
  d.setHours(20, 0, 0, 0);
  return d;
}

const seasonBody = `Ejderha Uyanışı ile Projects'te yeni bir sezon başlıyor.

## Neler var?

- **Yeni harita:** Volkanik adalar, gizli zindanlar ve yüzen kaleler.
- **Ejderha boss'u:** Her akşam 21.00'de uyanıyor. Takımını topla, ganimeti paylaş.
- **Sezon ödülleri:** Seviye atladıkça özel pelerinler, unvanlar ve kozmetikler.

## Nasıl katılırım?

Launcher'da **Projects** kurulumunu seç ve Oyna'ya bas. Mod paketi otomatik güncellenir, sunucuya doğrudan bağlanırsın.

Sorun yaşarsan Discord'daki destek kanalına yaz.`;

const content: HomeContent = {
  hero: {
    imageUrl: heroArt,
    title: "Ejderha Uyanışı başladı",
    subtitle: "Sezon 5 ile yeni harita, efsane ejderha boss'u ve haftalık turnuvalar sunucuda.",
    newsId: "sezon-5",
    particles: "embers",
  },
  banners: [{ id: "xp", level: "event", text: "Hafta sonu boyunca iki kat XP etkinliği açık." }],
  events: [
    {
      id: "pvp",
      title: "Cumartesi PvP turnuvası",
      startsAt: nextSaturdayEvening().toISOString(),
      endsAt: new Date(nextSaturdayEvening().getTime() + 3 * 3_600_000).toISOString(),
    },
  ],
  news: [
    {
      id: "sezon-5",
      title: "Sezon 5: Ejderha Uyanışı",
      summary: "Yeni harita, ejderha boss'u ve sezon ödülleri. Tüm ayrıntılar burada.",
      body: seasonBody,
      coverUrl: heroArt,
      category: "event",
      publishedAt: hoursAgo(2),
      pinned: true,
      featured: true,
    },
    {
      id: "pvp-turnuvasi",
      title: "Cumartesi PvP turnuvası",
      summary: "Arena kayıtları açıldı. İlk üçe 2.500 kredi ve özel pelerin.",
      body: "Kayıtlar cuma gece yarısına kadar açık. Takımlar en fazla üç kişi olabilir.",
      coverUrl: coverPvp,
      category: "event",
      publishedAt: hoursAgo(26),
      pinned: false,
      featured: false,
    },
    {
      id: "paket-v13",
      title: "Mod paketi v13 yayında",
      summary: "Sodium güncellendi, yeni minimap ve FPS iyileştirmeleri geldi.",
      body: "Paket bir sonraki Oyna'da otomatik güncellenir. Eklediğin modlara dokunulmaz.",
      coverUrl: coverForge,
      category: "patchNotes",
      publishedAt: hoursAgo(72),
      pinned: false,
      featured: false,
    },
    {
      id: "insa-yarismasi",
      title: "İnşa yarışması: Gökyüzü şehri",
      summary: "Takımını kur, 14 gün içinde en iyi gökyüzü şehrini inşa et.",
      body: "Jüri oylaması yarışma bitiminden sonraki hafta yapılır.",
      coverUrl: coverBuild,
      category: "community",
      publishedAt: hoursAgo(140),
      pinned: false,
      featured: false,
    },
    {
      id: "kis-etkinligi",
      title: "Kış etkinliği yaklaşıyor",
      summary: "Karlı köy, buz pateni yarışları ve sınırlı süreli kozmetikler.",
      body: "Etkinlik aralık ayının ilk haftası başlıyor.",
      coverUrl: coverWinter,
      category: "announcement",
      publishedAt: hoursAgo(220),
      pinned: false,
      featured: false,
    },
    {
      id: "spawn-yenilendi",
      title: "Spawn alanı yenilendi",
      summary: "Yeni meydan, hızlı ulaşım portalları ve rehber NPC'ler eklendi.",
      body: "Portallar ana dünyalara tek tıkla ışınlar.",
      coverUrl: coverSpawn,
      category: "update",
      publishedAt: hoursAgo(330),
      pinned: false,
      featured: false,
    },
  ],
  patchNotes: {
    packName: "Projects",
    version: "v13",
    publishedAt: hoursAgo(72),
    highlights: [
      "Sodium ve Lithium güncellendi",
      "Xaero minimap eklendi, H tuşuyla açılır",
      "Complementary shader 5.4 isteğe bağlı",
    ],
  },
  offline: false,
};

const instances: InstanceSummary[] = [
  {
    id: "projects",
    name: "Projects",
    icon: "dragon-egg",
    mcVersion: "26.1.2",
    loader: "fabric",
    loaderVersion: "0.17.3",
    managed: true,
    lastPlayedAt: hoursAgo(20),
    playTimeSecs: 152 * 3600 + 14 * 60,
    state: "ready",
    autoJoin: "play.projects.gg",
  },
  {
    id: "vanilla",
    name: "Vanilla",
    icon: "grass",
    mcVersion: "26.3",
    loader: "vanilla",
    managed: false,
    playTimeSecs: 0,
    state: "needsInstall",
  },
  {
    id: "hardcore",
    name: "Hardcore macera",
    icon: "swords",
    mcVersion: "1.20.1",
    loader: "forge",
    loaderVersion: "47.4.0",
    managed: false,
    lastPlayedAt: hoursAgo(300),
    playTimeSecs: 41 * 3600,
    state: "ready",
  },
  {
    id: "yaratici",
    name: "Yaratıcı dünya",
    icon: "tower",
    mcVersion: "1.21.1",
    loader: "fabric",
    loaderVersion: "0.16.14",
    managed: false,
    lastPlayedAt: hoursAgo(800),
    playTimeSecs: 9 * 3600,
    state: "needsUpdate",
  },
];

const accounts: AccountSummary[] = [
  { id: "a1", kind: "offline", username: "EjderAvcisi", active: true },
  { id: "a2", kind: "microsoft", username: "KorAtesi_", active: false },
];

const versions: McVersion[] = [
  ["26.3", "release", 30],
  ["26w41a", "snapshot", 12],
  ["26.2", "release", 120],
  ["26.1.2", "release", 160],
  ["26.1.1", "release", 175],
  ["26.1", "release", 190],
  ["1.21.11", "release", 300],
  ["1.21.10", "release", 340],
  ["1.21.8", "release", 420],
  ["1.21.4", "release", 600],
  ["1.21.1", "release", 790],
  ["1.20.6", "release", 900],
  ["1.20.4", "release", 1000],
  ["1.20.1", "release", 1180],
  ["1.19.4", "release", 1300],
  ["1.18.2", "release", 1600],
  ["1.16.5", "release", 2000],
  ["1.12.2", "release", 3000],
  ["1.8.9", "release", 3800],
  ["1.7.10", "release", 4200],
  ["b1.7.3", "oldBeta", 5400],
  ["a1.2.6", "oldAlpha", 5600],
].map(([id, kind, daysAgo]) => ({
  id: id as string,
  kind: kind as McVersion["kind"],
  releaseTime: hoursAgo((daysAgo as number) * 24),
}));

const loaderVersionTable: Record<Exclude<LoaderKind, "vanilla">, LoaderVersion[]> = {
  fabric: [
    { version: "0.17.3", stable: true },
    { version: "0.17.2", stable: true },
    { version: "0.16.14", stable: true },
  ],
  quilt: [
    { version: "0.29.2", stable: true },
    { version: "0.29.3-beta.1", stable: false },
  ],
  forge: [
    { version: "47.4.0", stable: true },
    { version: "47.3.39", stable: true },
  ],
  neoForge: [
    { version: "21.1.209", stable: true },
    { version: "21.1.210-beta", stable: false },
  ],
};

const mod = (
  name: string,
  version: string,
  author: string,
  description: string,
  fromPack = true,
  enabled = true,
): ModEntry => ({
  fileName: `${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-${version}.jar${enabled ? "" : ".disabled"}`,
  name,
  version,
  authors: [author],
  description,
  enabled,
  fromPack,
  sizeBytes: 180_000 + name.length * 41_000,
});

const projectsMods: ModEntry[] = [
  mod("Fabric API", "0.119.2", "FabricMC", "Core API library for the Fabric toolchain."),
  mod(
    "Sodium",
    "0.6.13",
    "CaffeineMC",
    "The fastest and most compatible rendering optimization mod.",
  ),
  mod(
    "Lithium",
    "0.15.1",
    "CaffeineMC",
    "General-purpose optimization mod with no gameplay changes.",
  ),
  mod("Iris Shaders", "1.8.11", "IrisShaders", "Shader pack loader compatible with Sodium."),
  mod(
    "Xaero's Minimap",
    "25.2.10",
    "xaero96",
    "Displays a minimap with waypoints and entity radar.",
  ),
  mod("Mod Menu", "13.0.3", "Prospector", "Adds a mod menu to view and configure mods."),
  mod("Entity Culling", "1.7.4", "tr7zw", "Skips rendering entities you cannot see."),
  mod("FerriteCore", "7.0.3", "malte0811", "Reduces memory usage."),
  mod("ImmediatelyFast", "1.6.5", "RaphiMC", "Speeds up immediate mode rendering."),
  mod("AppleSkin", "3.0.6", "squeek502", "Shows hunger and saturation values."),
  mod("Zoomify", "2.14.2", "isXander", "A zoom mod with smooth transitions.", false),
  mod("Continuity", "3.0.0", "PepperCode1", "Connected textures support.", false, false),
];

function detailFor(summary: InstanceSummary): InstanceDetail {
  const modded = summary.loader !== "vanilla";
  return {
    ...summary,
    systemMemoryMb: 16_384,
    settings: {
      javaMode: "auto",
      minMemoryMb: 2048,
      maxMemoryMb: summary.managed ? 6144 : 4096,
      jvmArgs: "",
      width: 1280,
      height: 720,
      fullscreen: false,
      preLaunch: "",
      postExit: "",
      wrapper: "",
      notes: summary.managed
        ? "Resmi paket. Mod listesi otomatik güncellenir; eklediğin modlara dokunulmaz."
        : "",
    },
    mods: modded ? (summary.managed ? projectsMods : projectsMods.slice(0, 5)) : [],
    resourcePacks: [
      {
        fileName: "Faithful 32x.zip",
        name: "Faithful 32x",
        description: "Klasik dokuların iki kat çözünürlüklü hâli.",
        enabled: true,
        sizeBytes: 8_400_000,
      },
      {
        fileName: "FreshAnimations.zip",
        name: "Fresh Animations",
        description: "Yaratıklara yeni animasyonlar.",
        enabled: false,
        sizeBytes: 2_100_000,
      },
    ],
    shaderPacks: modded
      ? [
          {
            fileName: "ComplementaryReimagined_r5.4.zip",
            name: "Complementary Reimagined",
            description: "r5.4",
            enabled: true,
            sizeBytes: 3_400_000,
          },
          {
            fileName: "BSL_v10.0.zip",
            name: "BSL Shaders",
            description: "v10.0",
            enabled: false,
            sizeBytes: 1_900_000,
          },
        ]
      : [],
    worlds: [
      {
        folder: "Yeni Dünya",
        name: "Yeni Dünya",
        gameMode: "survival",
        lastPlayedAt: hoursAgo(30),
        sizeBytes: 182_000_000,
      },
      {
        folder: "Yaratıcı test",
        name: "Yaratıcı test",
        gameMode: "creative",
        lastPlayedAt: hoursAgo(200),
        sizeBytes: 44_000_000,
      },
      {
        folder: "Hardcore 2",
        name: "Hardcore 2",
        gameMode: "hardcore",
        lastPlayedAt: hoursAgo(900),
        sizeBytes: 96_000_000,
      },
    ],
    screenshots: [coverSpawn, coverBuild, coverPvp, coverWinter].map((url, i) => ({
      fileName: `2026-10-${12 - i}_21.4${i}.12.png`,
      url,
      takenAt: hoursAgo(24 * (i + 1)),
    })),
    servers: [
      {
        name: "Projects",
        address: "play.projects.gg",
        online: true,
        players: 214,
        maxPlayers: 500,
      },
      {
        name: "Hypixel",
        address: "mc.hypixel.net",
        online: true,
        players: 31_402,
        maxPlayers: 100_000,
      },
    ],
  };
}

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const running = new Map<string, { cancelled: boolean }>();
let microsoftCancelled = false;

export const mockApi: LauncherApi = {
  ...mockAfk,
  async appInfo() {
    return { version: "2.0.0", os: "windows", arch: "x86_64", debug: true };
  },
  async homeContent() {
    await delay(250);
    return content;
  },
  async serverStatus(): Promise<ServerStatus> {
    await delay(300);
    return {
      name: "Projects",
      address: "play.projects.gg",
      online: true,
      players: 200 + Math.floor(Math.random() * 30),
      maxPlayers: 500,
      latencyMs: 28 + Math.floor(Math.random() * 12),
    };
  },
  async instances() {
    await delay(150);
    return [...instances];
  },
  async instance(id: string) {
    await delay(180);
    const summary = instances.find((i) => i.id === id);
    if (!summary) throw new Error(`instance ${id} not found`);
    return detailFor(summary);
  },
  async createInstance(spec: NewInstance) {
    await delay(400);
    const created: InstanceSummary = {
      id: `${spec.name.toLocaleLowerCase("tr").replace(/[^a-z0-9]+/g, "-")}-${Math.random().toString(36).slice(2, 6)}`,
      name: spec.name,
      icon: spec.icon,
      mcVersion: spec.mcVersion,
      loader: spec.loader,
      loaderVersion: spec.loaderVersion,
      managed: false,
      playTimeSecs: 0,
      state: "needsInstall",
    };
    instances.push(created);
    return created;
  },
  async updateInstanceSettings() {
    await delay(200);
  },
  async setModEnabled(_instanceId, fileName, enabled) {
    const entry = projectsMods.find(
      (m) => m.fileName.replace(/\.disabled$/, "") === fileName.replace(/\.disabled$/, ""),
    );
    if (entry) entry.enabled = enabled;
    await delay(120);
  },
  async openInstanceFolder() {},
  async duplicateInstance(id: string) {
    const source = instances.find((i) => i.id === id);
    if (!source) throw new Error("not found");
    const copy = {
      ...source,
      id: `${source.id}-kopya`,
      name: `${source.name} (kopya)`,
      managed: false,
    };
    instances.push(copy);
    return copy;
  },
  async deleteInstance(id: string) {
    const index = instances.findIndex((i) => i.id === id);
    if (index >= 0) instances.splice(index, 1);
  },
  async minecraftVersions() {
    await delay(200);
    return versions;
  },
  async loaderVersions(loader: LoaderKind) {
    await delay(200);
    return loader === "vanilla" ? [] : loaderVersionTable[loader];
  },
  async accounts() {
    return [...accounts];
  },
  async addOfflineAccount(name: string) {
    await delay(200);
    for (const a of accounts) a.active = false;
    const account: AccountSummary = {
      id: `off-${name}`,
      kind: "offline",
      username: name,
      active: true,
    };
    accounts.push(account);
    return account;
  },
  async loginMicrosoft(onStep: (step: LoginStep) => void) {
    microsoftCancelled = false;
    const steps: LoginStep[] = [
      "openingBrowser",
      "waitingForBrowser",
      "microsoft",
      "xboxLive",
      "xsts",
      "minecraft",
      "profile",
    ];
    for (const step of steps) {
      if (microsoftCancelled) throw new Error("cancelled");
      onStep(step);
      await delay(step === "waitingForBrowser" ? 2600 : 650);
    }
    onStep("done");
    for (const a of accounts) a.active = false;
    const account: AccountSummary = {
      id: "ms-new",
      kind: "microsoft",
      username: "AlevKanat",
      active: true,
    };
    accounts.push(account);
    return account;
  },
  async cancelMicrosoftLogin() {
    microsoftCancelled = true;
  },
  async setActiveAccount(id: string) {
    for (const a of accounts) a.active = a.id === id;
  },
  async removeAccount(id: string) {
    const index = accounts.findIndex((a) => a.id === id);
    if (index >= 0) accounts.splice(index, 1);
    if (accounts.length && !accounts.some((a) => a.active) && accounts[0])
      accounts[0].active = true;
  },
  async launch(instanceId: string, onEvent: (e: LaunchEvent) => void) {
    const job = { cancelled: false };
    running.set(instanceId, job);
    const instance = instances.find((i) => i.id === instanceId);
    const heavy = instance?.state !== "ready";
    const stages: Array<[LaunchStage, number]> = heavy
      ? [
          ["metadata", 4],
          ["java", 14],
          ["libraries", 38],
          ["assets", 86],
          ["natives", 90],
          ["pack", 100],
        ]
      : [
          ["metadata", 30],
          ["pack", 100],
        ];
    let percent = 0;
    const totalFiles = heavy ? 3204 : 48;
    for (const [stage, target] of stages) {
      while (percent < target) {
        if (job.cancelled) throw new Error("cancelled");
        await delay(110);
        percent = Math.min(target, percent + (heavy ? 1.6 : 9));
        onEvent({
          type: "progress",
          stage,
          percent,
          doneFiles: Math.round((percent / 100) * totalFiles),
          totalFiles,
          bytesPerSec: 9_400_000 + Math.random() * 6_000_000,
          etaSecs: heavy ? Math.round((100 - percent) / 14) : 1,
        });
      }
    }
    onEvent({ type: "launching" });
    await delay(1600);
    onEvent({ type: "started", pid: 4242 });
    if (instance) instance.state = "ready";
    await delay(25_000);
    onEvent({ type: "exited", code: 0, runtimeSecs: 25 });
  },
  async cancelLaunch(instanceId: string) {
    const job = running.get(instanceId);
    if (job) job.cancelled = true;
  },
  async updatePresence() {},
  async setPresenceEnabled() {},
  async legacyScan() {
    return {
      found: true,
      hasPremium: false,
      nickname: "EjderAvcisi",
      afkAccounts: 3,
      hasAfkClient: true,
      worlds: 4,
      worldsBytes: 812_000_000,
      screenshots: 37,
      resourcePacks: 2,
      shaderPacks: 1,
      freeBytes: 6_700_000_000,
    };
  },
  async legacyImport() {
    await delay(1800);
    return {
      premiumImported: false,
      nickname: "EjderAvcisi",
      afkAccounts: 3,
      afkClient: true,
      instanceId: "projects",
      copiedFiles: 214,
      linkedFiles: 3912,
      notes: [],
    };
  },
  async openUrl(url: string) {
    window.open(url, "_blank", "noopener");
  },
};
