// Realistic sample data for browser previews and for the UI before each backend command lands.
import heroArt from "@/assets/art/hero-dragon-1.webp";
import coverBuild from "@/assets/art/news-build-contest.webp";
import coverForge from "@/assets/art/news-patch-forge.webp";
import coverPvp from "@/assets/art/news-pvp-arena.webp";
import coverSpawn from "@/assets/art/news-spawn-city.webp";
import coverWinter from "@/assets/art/news-winter.webp";
import { newsHighlights } from "@/lib/news";
import type { LauncherApi } from "./index";
import { mockAccountName, mockAccounts } from "./mock-accounts";
import { mockAfk } from "./mock-afk";
import { createMockCleanup } from "./mock-cleanup";
import { createMockContent, mockExtraMods } from "./mock-content";
import { createMockConveniences, mockNewScreenshots, mockSessionMs } from "./mock-conveniences";
import { createMockData } from "./mock-data";
import { mockFreshOfficial, mockFreshOwnMod } from "./mock-fresh";
import { createMockImport } from "./mock-import";
import { mockCustomJava, mockJava, mockJavaPrompt } from "./mock-java";
import { pick } from "./mock-lang";
import { mockLaunchStop } from "./mock-launch-stop";
import { createMockLibrary, mockServers } from "./mock-library";
import { createMockModCenter } from "./mock-modcenter";
import { mockAskKeys, mockCrash, mockCrashScenario, mockModHealth } from "./mock-modhealth";
import {
  createMockModSets,
  DEMO_SET_INSTANCE,
  loadMockSets,
  mockSetFiles,
  modSetsFlag,
} from "./mock-modsets";
import { mockOpening, mockWindowShown } from "./mock-opening";
import { createMockSkins } from "./mock-skins";
import { mockSocial } from "./mock-social";
import { mockTrashedInstances } from "./mock-trash";
import type {
  ContentLinks,
  HomeContent,
  HomeStatus,
  InstanceDetail,
  InstanceSummary,
  LaunchEvent,
  LaunchOptions,
  LaunchStage,
  LoaderKind,
  LoaderVersion,
  McVersion,
  MinecraftNewsItem,
  ModEntry,
  NewInstance,
  NewsItem,
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
  banners: [
    {
      id: "xp",
      level: "event",
      text: "Hafta sonu boyunca iki kat XP etkinliği açık.",
      url: "https://projects.gg/etkinlik",
      dismissible: true,
    },
  ],
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
    kind: "pack",
    packName: "Projects",
    version: "v13",
    publishedAt: hoursAgo(72),
    highlights: [
      "Sodium ve Lithium güncellendi",
      "Xaero minimap eklendi, H tuşuyla açılır",
      "Complementary shader 5.4 isteğe bağlı",
    ],
  },
  minecraft: [],
  offline: false,
};

/**
 * Browser previews only: items the Discord bot's news feed adds (the bundled welcome content the
 * real launcher shows before anything is published stays without them).
 */
const feedNews: NewsItem[] = [
  {
    id: "discord-1300000000000000007",
    title: "Çifte XP haftası başladı",
    summary: "Pazara kadar tüm görevlerden iki kat XP. Günlük ödüller de ikiye katlandı.",
    body: "Çifte XP haftası **şimdi** sunucuda.\n\n- Tüm görevlerden iki kat XP\n- Günlük ödüller ikiye katlandı\n- Pazar 23.59'da biter\n\nAyrıntılar [etkinlik sayfasında](https://projects.gg/etkinlik).",
    coverUrl: coverWinter,
    category: "event",
    publishedAt: hoursAgo(1),
    pinned: false,
    featured: false,
    fromFeed: true,
    sourceUrl:
      "https://discord.com/channels/480094926164590593/480099230275600394/1300000000000000007",
  },
  {
    id: "discord-1300000000000000006",
    title: "2.4 yama notları",
    summary: "Yeni zindan, PvP dengelemeleri ve hata düzeltmeleri.",
    body: "- Yeni zindan: Kül Mağarası\n- Kılıç hasarı dengelendi\n- Arenada takılma düzeltildi\n- Market menüsü hızlandı",
    // No picture in the Discord post: the season's art stands in.
    coverUrl: heroArt,
    category: "patchNotes",
    publishedAt: hoursAgo(5),
    pinned: false,
    featured: false,
    fromFeed: true,
    sourceUrl:
      "https://discord.com/channels/480094926164590593/480099524652695576/1300000000000000006",
  },
];

/**
 * Browser previews only: Minecraft updates as the bot writes them (MCN1 dry runs, in its words).
 * Their covers live outside the app (`/mock-mc/…`, copied into a preview build for screenshots);
 * where they are missing the BlurHash stays.
 */
function mockMinecraft(): MinecraftNewsItem[] {
  const cover = (name: string) => `${import.meta.env.BASE_URL}mock-mc/${name}.webp`;
  return [
    {
      id: "minecraft-26.3",
      version: "26.3",
      kind: "release",
      title: "Minecraft 26.3 çıktı: Wilderness Bound",
      summary:
        "Yeni Dappled Forest biyomunu keşfet, Poplar ağaçlarıyla kamp kur, Straw Bed ile geceyi atlat ve Cushion üzerinde dinlen.",
      body: "Yabana açılma zamanı geldi. Wilderness Bound güncellemesiyle doğada keşif yapabileceğin ve kamp kurabileceğin yepyeni içerikler oyuna ekleniyor.\n\n## Öne çıkanlar\n\n- Soğuk biyomların yakınında ortaya çıkan ve rengarenk yapraklarıyla dikkat çeken **Dappled Forest** biyomu\n- Kendi odun setine sahip üç farklı renkteki **Poplar Trees** ve gövdelerinde yetişen zıplatıcı **Shelf Mushrooms**\n- Sandıklarında ganimetler ve yeni keşif haritaları barındıran **Abandoned Camp** yapıları\n- Doğuş noktanı değiştirmeden tek seferlik geceyi geçirmeni sağlayan **Straw Bed**\n- Üzerine oturabileceğin 16 farklı renkteki **Cushion** eşyası",
      coverUrl: cover("cover-09-autumn-valley"),
      coverBlurhash: "LZFzA:9^I==v}rI=NbxDxGW;R+WV",
      publishedAt: hoursAgo(50),
      expiresAt: hoursAgo(-24 * 40),
      url: "https://www.minecraft.net/article/minecraft-java-edition-26-3",
      en: {
        title: "Minecraft 26.3 released: Wilderness Bound",
        summary:
          "Explore the new Dappled Forest biome, set up camp with Poplar wood, sleep on Straw Beds, and relax on colorful Cushions.",
        body: "It is time to venture into the wild. The Wilderness Bound update brings exciting new features for setting up camps and exploring uncharted forests.\n\n## Highlights\n\n- The new **Dappled Forest** biome found near cold regions, filled with colorful foliage\n- Three vibrant variations of **Poplar Trees** with a new wood set and bouncy **Shelf Mushrooms**\n- **Abandoned Camp** structures scattered across various biomes, holding valuable loot and maps\n- Single-use **Straw Bed** items that let you pass the night without setting your spawn point\n- Craftable **Cushion** items in 16 colors that you can place down and sit on",
      },
    },
    {
      id: "minecraft-26.1.2",
      version: "26.1.2",
      kind: "release",
      title: "Minecraft 26.1.2 çıktı: hata düzeltmeleri",
      summary:
        "Bu küçük ara güncelleme, son sürümde ortaya çıkan birkaç kritik hatayı çözmeye odaklanıyor.",
      body: "Minecraft 26.1.2 ile son sürümde karşılaşılan bazı kritik hatalar giderildi. Oyunu güncelleyerek daha sorunsuz ve kararlı bir deneyim yaşayabilirsin.",
      // Published without a cover: the season's art stands in.
      coverUrl: heroArt,
      publishedAt: hoursAgo(24 * 9),
      expiresAt: hoursAgo(-24 * 30),
      url: "https://www.minecraft.net/article/minecraft-java-edition-26-1-2",
      en: {
        title: "Minecraft 26.1.2 released: bug fixes",
        summary: "This hotfix addresses a couple of critical issues found in the recent release.",
        body: "Minecraft 26.1.2 is a small hotfix that resolves several critical issues discovered in the latest update, providing a more stable gameplay experience.",
      },
    },
    {
      id: "minecraft-26.1",
      version: "26.1",
      kind: "release",
      title: "Minecraft 26.1 çıktı: Tiny Takeover",
      summary:
        "Yavru yaratıklar yenilenen görünümleri ve sesleriyle karşında. Golden Dandelion ile büyümelerini durdurabilir, Name Tag eşyasını artık kendin üretebilirsin.",
      body: "Tiny Takeover güncellemesiyle yavru yaratıklar yepyeni görünümlere kavuşuyor. Yeni eşyalar ve seslerle dünyan artık çok daha sevimli bir hal alıyor.\n\n## Öne çıkanlar\n\n- Çoğu yavru yaratığın modeli ve dokusu baştan aşağı yenilendi.\n- **Golden Dandelion** eklendi; Dandelion ve Gold Nugget ile üretilen bu çiçekle yavru yaratıkların büyümesini durdurabilir veya yeniden başlatabilirsin.\n- **Name Tag** artık 1 Paper ve 1 metal parçacığı ile üretilebiliyor.\n- Yavru kurt, kedi, domuz, at ve tavuklar için yeni sevimli sesler geldi.",
      coverUrl: cover("cover-11-scaffold"),
      coverBlurhash: "LRG?R]}p^4xE^5$$soWV9}IsEhR,",
      publishedAt: hoursAgo(24 * 24),
      expiresAt: hoursAgo(-24 * 20),
      url: "https://www.minecraft.net/article/minecraft-java-edition-26-1",
      en: {
        title: "Minecraft 26.1 released: Tiny Takeover",
        summary:
          "Baby mobs receive revamped looks and sounds, the Golden Dandelion halts baby aging, and Name Tags are now craftable.",
      },
    },
  ];
}

/**
 * A preview switch: `?event=next|soon|imminent|open|none|overlay|join|joinweek|joinopen|joinsoon`
 * (or localStorage `pl.mock.event`; the `join` ones are on the server: "Katıl"), `?notes=pack`,
 * `?banners=none|two`,
 * `?status=outage|major|degraded|notice|expiring`, `?maintenance=1`, `?projects=<count>` (keeps
 * the first Projects news), `?mc=<count>` (Minecraft updates, 3 by default), `?join=offline` (no
 * Projects instance and its pack cannot be fetched: "Katıl" fails).
 */
function mockFlag(key: string): string {
  try {
    return (
      new URLSearchParams(window.location.search).get(key) ??
      localStorage.getItem(`pl.mock.${key}`) ??
      ""
    );
  } catch {
    return "";
  }
}

/** When the preview loaded: the reminder previews start relative to it, refetch or not. */
const previewLoadedAt = Date.now();

/** Events in event card order, relative to now: a live feed event leads by default. */
function mockEvents(): HomeContent["events"] {
  const at = (hours: number) => new Date(Date.now() + hours * 3_600_000).toISOString();
  const saturday = nextSaturdayEvening();
  const pvp = {
    id: "pvp",
    title: "Cumartesi PvP turnuvası",
    startsAt: saturday.toISOString(),
    endsAt: new Date(saturday.getTime() + 3 * 3_600_000).toISOString(),
    newsId: "pvp-turnuvasi",
  };
  // A week-long event Studio lowered: whatever else runs leads the card over it.
  const beginners = {
    id: "discord-1300000000000000009",
    title: "Yeni oyuncu haftası",
    startsAt: at(-50),
    endsAt: at(24 * 5),
    priority: -1,
    fromFeed: true,
  };
  switch (mockFlag("event")) {
    case "none":
      return [];
    case "next":
      return [pvp];
    // A reminder set now rings in a minute (soon), or at once (imminent: under five minutes).
    case "soon":
    case "imminent": {
      const startsAt = previewLoadedAt + (mockFlag("event") === "soon" ? 6 : 3) * 60_000;
      return [
        {
          id: "baskin",
          title: "Ejderha baskını",
          startsAt: new Date(startsAt).toISOString(),
          endsAt: new Date(startsAt + 3_600_000).toISOString(),
          newsId: "sezon-5",
        },
        pvp,
      ];
    }
    case "overlay":
      return [
        {
          id: "discord-1300000000000000007",
          title: "Çifte XP haftası",
          startsAt: at(-1),
          endsAt: at(2.25),
          newsId: "discord-1300000000000000007",
          fromFeed: true,
        },
        beginners,
        pvp,
      ];
    case "open":
    case "joinopen":
      return [
        {
          id: "discord-1300000000000000008",
          title: "Ejderha baskını",
          startsAt: at(-0.4),
          endsAt: at(2.6),
          openEnded: true,
          newsId: "sezon-5",
          join: mockFlag("event") === "joinopen",
        },
        pvp,
      ];
    // On the server ("Katıl"): hours left, and the widest countdown (days left).
    case "join":
      return [
        {
          id: "discord-1300000000000000007",
          title: "Çifte XP haftası",
          startsAt: at(-1),
          endsAt: at(2.25),
          newsId: "discord-1300000000000000007",
          join: true,
        },
        pvp,
      ];
    case "joinweek":
      return [{ ...beginners, priority: 0, join: true }, pvp];
    // A server event that starts 8 seconds after the preview loads: the bell gives way to "Katıl".
    case "joinsoon":
      return [
        {
          id: "baskin",
          title: "Ejderha baskını",
          startsAt: new Date(previewLoadedAt + 8_000).toISOString(),
          endsAt: new Date(previewLoadedAt + 3_600_000).toISOString(),
          newsId: "sezon-5",
          join: true,
        },
        pvp,
      ];
    default:
      return [
        {
          id: "discord-1300000000000000007",
          title: "Çifte XP haftası",
          startsAt: at(-1),
          endsAt: at(2.25),
          newsId: "discord-1300000000000000007",
        },
        pvp,
      ];
  }
}

/** The status notices the bot writes (its default wording), by preview switch. */
function mockStatus(): HomeStatus | undefined {
  const inMinutes = (minutes: number) => new Date(Date.now() + minutes * 60_000).toISOString();
  const page = "https://status.projects.gg";
  switch (mockFlag("status")) {
    case "outage":
      return {
        level: "outage",
        title: "Bazı hizmetlerde sorun var",
        message: "Şu an erişilemiyor: Survival Zephyr",
        url: page,
        expiresAt: inMinutes(15),
      };
    case "major":
      return {
        level: "outage",
        title: "Hizmetlerde büyük bir kesinti var",
        message: "Şu an erişilemiyor: Ana Sunucu, Lobi, SMP ve 2 hizmet daha",
        url: page,
        expiresAt: inMinutes(15),
      };
    case "degraded":
      return {
        level: "degraded",
        title: "Bazı hizmetlerde aksaklık var",
        message: "Bağlanırken sorun yaşanabilir: SMP",
        url: page,
        expiresAt: inMinutes(15),
      };
    case "notice":
      return {
        level: "notice",
        title: "Bu gece kısa bir bakım var",
        message: "Sunucular 03.00 ile 03.30 arasında kapalı olacak.",
        url: page,
        expiresAt: inMinutes(15),
      };
    case "expiring":
      // Leaves by itself after 12 seconds (to watch the exit).
      return {
        level: "degraded",
        title: "Bazı hizmetlerde aksaklık var",
        message: "Bağlanırken sorun yaşanabilir: SMP",
        url: page,
        expiresAt: new Date(Date.now() + 12_000).toISOString(),
      };
    default:
      return undefined;
  }
}

/**
 * Studio's links as the backend shows them (`pl_content::Links::community`). `?links=studio`: every
 * platform published; `few`: only YouTube (the others hidden); `none`: every platform hidden
 * (Discord stays). Default: nothing published, the built-in addresses.
 */
function mockLinks(): ContentLinks {
  const builtIn: ContentLinks["community"] = [
    { platform: "wiki", url: "https://wiki.projects.gg" },
    { platform: "discord", url: "https://discord.gg/projects" },
    { platform: "forum", url: "https://forum.projects.gg" },
    { platform: "instagram", url: "https://www.instagram.com/projects.oyun/" },
    { platform: "tiktok", url: "https://www.tiktok.com/@projects.oyun" },
    { platform: "youtube", url: "https://www.youtube.com/@projectsoyun" },
  ];
  const discord = builtIn[1] ?? { platform: "discord", url: "https://discord.gg/projects" };
  const variants: Record<string, ContentLinks["community"]> = {
    studio: [
      ...builtIn,
      { platform: "x", url: "https://x.com/projectsoyun" },
      { platform: "twitch", url: "https://www.twitch.tv/projectsoyun" },
      { platform: "kick", url: "https://kick.com/projectsoyun" },
    ],
    few: [discord, { platform: "youtube", url: "https://www.youtube.com/@projectsoyun" }],
    none: [discord],
  };
  return {
    website: "https://projects.gg",
    discord: "https://discord.gg/projects",
    store: "https://store.projects.gg",
    custom: [{ label: "Kurallar", url: "https://projects.gg/kurallar" }],
    community: variants[mockFlag("links")] ?? builtIn,
  };
}

/** The home content as the backend merges it: the newest patch notes news leads the box. */
function mockHome(): HomeContent {
  const projects = Number.parseInt(mockFlag("projects"), 10);
  const minecraftCount = Number.parseInt(mockFlag("mc"), 10);
  const news = [...content.news, ...feedNews]
    .sort(
      (a, b) =>
        Number(b.pinned) - Number(a.pinned) ||
        b.publishedAt.localeCompare(a.publishedAt) ||
        a.id.localeCompare(b.id),
    )
    .slice(0, Number.isNaN(projects) ? undefined : projects);
  const notes = news
    .filter((item) => item.category === "patchNotes")
    .sort((a, b) => b.publishedAt.localeCompare(a.publishedAt))[0];
  const banners = mockFlag("banners");
  const maintenance = mockFlag("maintenance")
    ? {
        title: "Bakım çalışması",
        message: "Sunucular kısa bir süre bakımda olacak.",
        until: new Date(Date.now() + 45 * 60_000).toISOString(),
      }
    : content.maintenance;
  return {
    ...content,
    maintenance,
    // Like the backend: Studio's maintenance notice wins over the status notice.
    status: maintenance ? undefined : mockStatus(),
    links: mockLinks(),
    banners:
      banners === "none"
        ? []
        : banners === "two"
          ? [
              ...content.banners,
              {
                id: "gece-bakimi",
                level: "warning",
                text: "Sunucular bu gece 03.00'te 15 dakika bakımda olacak.",
                dismissible: false,
              },
            ]
          : content.banners,
    news,
    minecraft: mockMinecraft().slice(0, Number.isNaN(minecraftCount) ? undefined : minecraftCount),
    events: mockEvents(),
    patchNotes:
      notes && mockFlag("notes") !== "pack"
        ? {
            kind: "news",
            newsId: notes.id,
            title: notes.title,
            publishedAt: notes.publishedAt,
            highlights: newsHighlights(notes.body, notes.summary),
          }
        : content.patchNotes,
  };
}

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
    createdAt: hoursAgo(4200),
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
    createdAt: hoursAgo(40),
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
    group: "Arkadaşlarla",
    createdAt: hoursAgo(2900),
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
    group: "Arkadaşlarla",
    createdAt: hoursAgo(1500),
  },
  // `?vanillaPlayed=1`: a vanilla instance with play history, worlds and servers (the "Vanilla"
  // one above was never started).
  ...(mockFlag("vanillaPlayed") === "1"
    ? [
        {
          id: "hayatta-kalma",
          name: "Hayatta kalma",
          icon: "pickaxe" as const,
          mcVersion: "26.3",
          loader: "vanilla" as const,
          managed: false,
          lastPlayedAt: hoursAgo(6),
          playTimeSecs: 23 * 3600 + 40 * 60,
          state: "ready" as const,
          createdAt: hoursAgo(700),
        },
      ]
    : []),
  // `?modsets=demo`: an instance made from the PvP set (MODSET1).
  ...(modSetsFlag() === "demo" ? [DEMO_SET_INSTANCE] : []),
];
// `?join=offline`: no Projects instance yet and no connection to fetch its pack, so "Katıl"
// cannot start the game and says why (FIX1, E1).
const joinOffline = mockFlag("join") === "offline";
const projectsIndex = instances.findIndex((i) => i.managed);
if (joinOffline && projectsIndex >= 0) instances.splice(projectsIndex, 1);
// `?fresh=1`: the official instance was just added and never downloaded (mock-fresh.ts).
const freshOfficial = mockFreshOfficial() ? instances.find((i) => i.managed) : undefined;
if (freshOfficial) {
  freshOfficial.lastPlayedAt = undefined;
  freshOfficial.playTimeSecs = 0;
  freshOfficial.state = "needsInstall";
}

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
  recommended: id === "26.1.2",
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
  // Pack mods are recommended mods of the mod center.
  itemId: fromPack ? name.toLowerCase().replace(/[^a-z0-9]+/g, "-") : undefined,
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

/**
 * `?emptyContent=1`: instances other than the official one start without mods and packs (the
 * empty tabs' drop zones); mods dropped onto the window are listed.
 */
function detailFor(summary: InstanceSummary): InstanceDetail {
  const detail = fullDetailFor(summary);
  if (summary.managed && mockFreshOfficial()) {
    const own = mockFreshOwnMod()
      ? [
          mod(
            "Xaero's Minimap",
            "25.2.10",
            "xaero96",
            "Displays a minimap with waypoints and entity radar.",
            false,
          ),
        ]
      : [];
    return {
      ...detail,
      settings: { ...detail.settings, notes: "" },
      mods: [...own, ...(mockExtraMods.get(summary.id) ?? [])],
      resourcePacks: [],
      shaderPacks: [],
      worlds: [],
      screenshots: [],
    };
  }
  if (summary.managed || mockFlag("emptyContent") !== "1") return detail;
  return {
    ...detail,
    mods: mockExtraMods.get(summary.id) ?? [],
    resourcePacks: [],
    shaderPacks: [],
  };
}

function fullDetailFor(summary: InstanceSummary): InstanceDetail {
  const modded = summary.loader !== "vanilla";
  return {
    ...summary,
    systemMemoryMb: 16_384,
    autoMemoryMb: modded ? (summary.managed ? 5120 : 4096) : 4096,
    settings: {
      ...(mockCustomJava()
        ? {
            javaMode: "custom" as const,
            javaPath: "C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.12.7-hotspot\\bin\\javaw.exe",
          }
        : { javaMode: "auto" as const }),
      minMemoryMb: 2048,
      maxMemoryMb: summary.managed ? 6144 : 4096,
      memoryAuto: summary.managed || summary.id === "vanilla",
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
    mods: [
      ...(modded
        ? summary.managed
          ? projectsMods
          : // Only the official pack installs pack files.
            projectsMods.slice(0, 5).map((m) => ({ ...m, fromPack: false, itemId: undefined }))
        : []),
      ...(mockExtraMods.get(summary.id) ?? []),
    ],
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
    servers: mockServers(summary.id),
  };
}

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const running = new Map<string, { cancelled: boolean }>();

export const mockApi: LauncherApi = {
  ...mockAfk,
  ...createMockConveniences(
    (id) => instances.find((i) => i.id === id)?.loader,
    (id, files) =>
      mockExtraMods.set(id, [
        ...(mockExtraMods.get(id) ?? []),
        ...files.map((f) => ({
          fileName: f.name,
          name: f.name.replace(/\.jar$/i, ""),
          version: "1.0.0",
          authors: [],
          description: "",
          enabled: true,
          fromPack: false,
          sizeBytes: 380_000,
        })),
      ]),
  ),
  content: createMockContent({ createInstance: (spec) => mockApi.createInstance(spec) }),
  ...mockAccounts,
  skins: createMockSkins(mockAccountName),
  data: createMockData(),
  cleanup: createMockCleanup(),
  modCenter: createMockModCenter(),
  modSets: createMockModSets({
    instanceOf: (id) => instances.find((i) => i.id === id),
    createInstance: (spec) => mockApi.createInstance(spec),
  }),
  social: mockSocial,
  modHealth: mockModHealth,
  java: mockJava,
  importer: createMockImport(),
  library: createMockLibrary({
    setGroup: (id, group) => {
      const instance = instances.find((i) => i.id === id);
      if (instance) instance.group = group ?? undefined;
    },
  }),
  async appInfo() {
    return { version: "2.0.0", os: "windows", arch: "x86_64", debug: true };
  },
  async homeContent() {
    await delay(250);
    return mockHome();
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
    if (modSetsFlag() === "demo") await loadMockSets();
    const detail = detailFor(summary);
    // Files a ready-made set put in place carry its badge; the set's own preview instance has
    // only those and one mod of the player's.
    const fromSets = mockSetFiles(id);
    const demo = id === DEMO_SET_INSTANCE.id;
    return {
      ...detail,
      mods: [...(demo ? projectsMods.slice(-1) : detail.mods), ...fromSets.mods],
      resourcePacks: [...(demo ? [] : detail.resourcePacks), ...fromSets.resourcePacks],
      shaderPacks: [...(demo ? [] : detail.shaderPacks), ...fromSets.shaderPacks],
    };
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
      createdAt: new Date().toISOString(),
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
      // Named like the backend does, in the interface's language.
      name: pick(`${source.name} (kopya)`, `${source.name} (copy)`),
      managed: false,
      createdAt: new Date().toISOString(),
    };
    instances.push(copy);
    return copy;
  },
  async deleteInstance(id: string) {
    await delay(160);
    const index = instances.findIndex((i) => i.id === id);
    const [gone] = index >= 0 ? instances.splice(index, 1) : [];
    const trashId = `trash-${id}-${Date.now()}`;
    if (gone) {
      mockTrashedInstances.set(trashId, {
        instance: gone,
        deletedAt: Date.now(),
        sizeBytes: 1_288_490_189,
      });
    }
    return trashId;
  },
  async restoreInstance(trashId: string) {
    await delay(200);
    const back = mockTrashedInstances.get(trashId)?.instance;
    if (!back) throw new Error("not in the trash");
    mockTrashedInstances.delete(trashId);
    const taken = (name: string) => instances.some((i) => i.name === name);
    let name = back.name;
    for (let n = 2; taken(name); n++) name = `${back.name} (${n})`;
    const restored = { ...back, name };
    instances.push(restored);
    return restored;
  },
  async minecraftVersions() {
    await delay(200);
    return versions;
  },
  async loaderVersions(loader: LoaderKind) {
    await delay(200);
    return loader === "vanilla" ? [] : loaderVersionTable[loader];
  },
  async launch(instanceId: string, onEvent: (e: LaunchEvent) => void, options?: LaunchOptions) {
    const job = { cancelled: false };
    running.set(instanceId, job);
    const instance = instances.find((i) => i.id === instanceId);
    // Preview switch (mock-java.ts): the custom Java does not suit the game.
    const javaQuestion = options?.java ? null : mockJavaPrompt(instanceId, instance?.name ?? "");
    if (javaQuestion) {
      await delay(450);
      onEvent({ type: "javaMismatch", ...javaQuestion });
      return;
    }
    // Preview switches (mock-launch-stop.ts): no room on the disk, the launch errors' texts.
    if (await mockLaunchStop(onEvent, job)) return;
    // "Kur ve oyna" downloads Java first.
    const heavy = instance?.state !== "ready" || options?.java === "install";
    const modded = instance?.loader !== "vanilla";
    // One bar for the whole launch, like the backend's `LaunchBar` (pl_mc::install::bar): Java 10
    // (when it is installed first), mod loader 12, game files 70, pack files 18. A part that does
    // not run leaves its share to the others, and the bar never goes back.
    const parts: Array<[LaunchStage[], number, number]> = [];
    if (options?.java === "install") parts.push([["java"], 10, 0]);
    if (heavy && modded) parts.push([["loader"], 12, 0]);
    parts.push([
      heavy ? ["metadata", "libraries", "assets", "natives"] : ["metadata"],
      70,
      heavy ? 3204 : 0,
    ]);
    parts.push([["pack"], 18, 48]);
    const sum = parts.reduce((total, [, share]) => total + share, 0);
    let from = 0;
    for (const [stages, share, totalFiles] of parts) {
      const span = (share / sum) * 100;
      const steps = heavy ? Math.round(share * 1.4) : 4;
      for (let step = 1; step <= steps; step++) {
        if (job.cancelled) throw new Error("cancelled");
        await delay(110);
        const fraction = step / steps;
        const stage =
          stages[Math.min(stages.length - 1, Math.floor(fraction * stages.length * 0.999))];
        onEvent({
          type: "progress",
          stage: stage ?? "metadata",
          percent: from + span * fraction,
          doneFiles: Math.round(fraction * totalFiles),
          totalFiles,
          bytesPerSec:
            stage === "loader" && fraction > 0.4 ? 0 : 9_400_000 + Math.random() * 6_000_000,
          etaSecs: heavy ? Math.round((1 - fraction) * steps * 0.11 + (100 - from - span) / 14) : 1,
        });
      }
      from += span;
    }
    // Preview switches (mock-modhealth.ts): mods that stop the game, warnings, a crash.
    if (instance?.loader !== "vanilla") {
      const check = await mockModHealth.check(instanceId);
      if (check.blocking > 0 && !options?.ignoreModIssues) {
        onEvent({ type: "modsBlocked", check });
        return;
      }
      if (mockAskKeys(check, Boolean(options?.ignoreModIssues))) {
        onEvent({ type: "keyClash", check });
        return;
      }
      if (check.issues.some((i) => i.severity === "warning" && i.kind !== "keyClash")) {
        onEvent({ type: "modsWarning", check });
      }
    }
    onEvent({ type: "launching" });
    await delay(1600);
    // Preview switches (mock-opening.ts): how long the game's window takes, a first start.
    const opening = mockOpening();
    onEvent({ type: "started", pid: 4242, firstStart: opening.firstStart });
    if (instance) instance.state = "ready";
    const crash = mockCrashScenario();
    if (crash) {
      // The game stops while it opens: its window never shows.
      await delay(2600);
      onEvent({ type: "crashed", ...mockCrash(instanceId, crash) });
      onEvent({ type: "exited", code: 1, runtimeSecs: 4 });
      return;
    }
    // "Açılıyor" until the game's window shows; "İptal" closes the game meanwhile.
    if (!(await mockWindowShown(opening, onEvent, job))) return;
    // `?shotcopy=fail`: a screenshot taken during the game could not go to the clipboard.
    if (mockFlag("shotcopy") === "fail") {
      await delay(1200);
      onEvent({ type: "screenshotNotCopied" });
    }
    const session = mockSessionMs();
    await delay(session);
    const shots = mockNewScreenshots();
    if (shots) onEvent({ type: "newScreenshots", ...shots });
    onEvent({ type: "exited", code: 0, runtimeSecs: Math.round(session / 1000) });
  },
  async cancelLaunch(instanceId: string) {
    const job = running.get(instanceId);
    if (job) job.cancelled = true;
  },
  async setScreenshotCopy() {},
  async updatePresence() {},
  async setPresenceEnabled() {},
  async setLanguage() {},
  async legacyScan() {
    // `?legacy=noWorlds`: an old launcher without worlds; `?legacy=empty`: none of the kinds the
    // welcome lists (its sentence names only what was found); `?legacy=none`: no old launcher.
    const legacy = mockFlag("legacy");
    const empty = legacy === "empty";
    return {
      found: legacy !== "none",
      hasPremium: false,
      nickname: empty ? null : "EjderAvcisi",
      afkAccounts: empty ? 0 : 3,
      hasAfkClient: !empty,
      worlds: legacy ? 0 : 4,
      worldsBytes: legacy ? 0 : 812_000_000,
      screenshots: empty ? 0 : 37,
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
  async officialStatus() {
    await delay(150);
    return {
      pack: {
        id: "projects",
        name: "Projects",
        version: "v13",
        mcVersion: "26.1.2",
        loader: "fabric",
        summary: "Sunucu için hazırlanmış performans ve görünüm paketi.",
      },
      instanceId: instances.find((i) => i.managed)?.id,
    };
  },
  async ensureOfficialInstance() {
    await delay(500);
    const existing = instances.find((i) => i.managed);
    if (existing) return existing;
    if (joinOffline) {
      throw Object.assign(
        new Error(
          pick(
            "Projects paketi indirilemedi. İnternet bağlantını kontrol edip tekrar dene.",
            "Couldn't download the Projects pack. Check your internet connection and try again.",
          ),
        ),
        { code: "PAKET-AG", details: "Önizleme verisi" },
      );
    }
    const created: InstanceSummary = {
      id: "projects",
      name: "Projects",
      icon: "dragon-egg",
      mcVersion: "26.1.2",
      loader: "fabric",
      loaderVersion: "0.19.5",
      managed: true,
      playTimeSecs: 0,
      state: "needsInstall",
      autoJoin: "play.projects.gg",
    };
    instances.unshift(created);
    return created;
  },
  onContentChanged(listener) {
    // Preview: `dispatchEvent(new Event("pl-mock-content-changed"))` acts like newly published
    // content (e.g. after changing localStorage `pl.mock.status`), to watch the home transitions.
    const handler = () => listener();
    window.addEventListener("pl-mock-content-changed", handler);
    return () => window.removeEventListener("pl-mock-content-changed", handler);
  },
  // Previews show the offline strip with `?offline` in the address.
  async contentStatus() {
    return { offline: new URLSearchParams(window.location.search).has("offline") };
  },
  onContentStatus() {
    return () => {};
  },
  async contentRecheck() {},
  onDeepLink() {
    return () => {};
  },
  async takePendingDeepLink() {
    return null;
  },
  async quitApp() {},
};
