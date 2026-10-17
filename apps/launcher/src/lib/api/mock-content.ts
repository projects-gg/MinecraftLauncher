// Simulated Modrinth browser, installs and modpacks for browser previews. Search for
// "çevrimdışı" to preview the offline state.
import type {
  ContentApi,
  ContentCategory,
  ContentHit,
  ContentInstallResult,
  ContentKind,
  ContentProject,
  ContentProjectType,
  ContentVersion,
  Datapack,
  FileDropEvent,
  InstalledContent,
  JobProgress,
  ModpackInfo,
  ModpackUpdatePlan,
  PackPreview,
} from "./content-types";
import type { InstanceSummary, ModEntry, NewInstance } from "./types";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const daysAgo = (d: number) => new Date(Date.now() - d * 86_400_000).toISOString();
const cdn = (id: string, file: string) => `https://cdn.modrinth.com/data/${id}/${file}`;

/** Mirrors a command error: friendly message, code and details for "copy details". */
class MockCommandError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.details = `${code}\nmock`;
  }
}

type Row = [string, string, string, string, string, string, number, number, number, string];

const rows: Record<ContentProjectType, Row[]> = {
  datapack: [
    [
      "8oi3bsk5",
      "terralith",
      "Terralith",
      "Stardust Labs",
      "Explore almost 100 new biomes consisting of both realism and light fantasy, using just Vanilla blocks.",
      "icon.png",
      27_400_000,
      9_200,
      12,
      "worldgen",
    ],
    [
      "ZVzW5oNS",
      "incendium",
      "Incendium",
      "Stardust Labs",
      "A Nether overhaul with new biomes, structures, bosses and loot, built from Vanilla blocks.",
      "icon.png",
      9_800_000,
      4_100,
      20,
      "worldgen,adventure",
    ],
    [
      "aKCwCg3P",
      "structory",
      "Structory",
      "Stardust Labs",
      "Small, atmospheric structures scattered across every biome of the Overworld.",
      "icon.png",
      6_100_000,
      2_300,
      35,
      "worldgen",
    ],
    [
      "HSfsxuTo",
      "afk-display",
      "AFK Display",
      "Plexion",
      "Shows a tag above players who are away from the keyboard. Works on servers too.",
      "icon.png",
      1_200_000,
      640,
      60,
      "utility",
    ],
  ],
  mod: [
    [
      "P7dR8mSH",
      "fabric-api",
      "Fabric API",
      "modmuss50",
      "Lightweight and modular API providing common hooks and intercompatibility measures utilized by mods using the Fabric toolchain.",
      "icon.png",
      278_468_409,
      36_919,
      1,
      "library",
    ],
    [
      "AANobbMI",
      "sodium",
      "Sodium",
      "jellysquid3",
      "A high-performance rendering engine replacement for Minecraft, which greatly improves frame rates and reduces micro-stutter.",
      "295862f4724dc3f78df3447ad6072b2dcd3ef0c9_96.webp",
      245_166_038,
      41_770,
      25,
      "optimization",
    ],
    [
      "YL57xq9U",
      "iris",
      "Iris Shaders",
      "coderbot",
      "A modern shader pack loader for Minecraft intended to be compatible with existing OptiFine shader packs.",
      "18d0e7f076d3d6ed5bedd472b853909aac5da202_96.webp",
      190_397_437,
      30_002,
      15,
      "decoration,optimization",
    ],
    [
      "NNAgCjsB",
      "entityculling",
      "Entity Culling",
      "tr7zw",
      "Using async path-tracing to hide block entities and entities that are not visible.",
      "7873452d6cede4daed12da3d7d8c193ab88b4fd6_96.webp",
      182_402_715,
      17_827,
      7,
      "optimization",
    ],
    [
      "9s6osm5g",
      "cloth-config",
      "Cloth Config API",
      "shedaniel",
      "Configuration library for Minecraft mods.",
      "ed8a2316cbb6f4fc5f510e8e13a59a85cbbbff4d_96.webp",
      179_773_032,
      16_894,
      22,
      "library",
    ],
    [
      "uXXizFIs",
      "ferrite-core",
      "FerriteCore",
      "malte0811",
      "Memory usage optimizations.",
      "222a126f26f8f9ae1eb339f3b767677f18bff31f_96.webp",
      161_330_380,
      16_614,
      205,
      "optimization,utility",
    ],
    [
      "mOgUt4GM",
      "modmenu",
      "Mod Menu",
      "Prospector",
      "Adds a mod menu to view the list of mods you have installed.",
      "5a20ed1450a0e1e79a1fe04e61bb4e5878bf1d20.png",
      155_735_884,
      27_639,
      3,
      "utility",
    ],
    [
      "gvQqBUqZ",
      "lithium",
      "Lithium",
      "jellysquid3",
      "No-compromises game logic optimization mod, useful for both single-player games and multi-player servers.",
      "bcc8686c13af0143adf4285d741256af824f70b7_96.webp",
      137_347_159,
      23_920,
      17,
      "optimization",
    ],
    [
      "5ZwdcRci",
      "immediatelyfast",
      "ImmediatelyFast",
      "RaphiMC",
      "Speed up immediate mode rendering in Minecraft.",
      "e57b6b451425692ac17ad322d5e14bea686a383a_96.webp",
      132_935_968,
      11_677,
      2,
      "optimization",
    ],
    [
      "1eAoo2KR",
      "yacl",
      "YetAnotherConfigLib (YACL)",
      "isxander",
      "A builder-based configuration library for Minecraft!",
      "08c0cd32515e260f4bb20bbc0696510041523f9a_96.webp",
      131_382_202,
      9_755,
      9,
      "library,utility",
    ],
    [
      "Ha28R6CL",
      "fabric-language-kotlin",
      "Fabric Language Kotlin",
      "modmuss50",
      "Fabric language module for Kotlin.",
      "72c3d74aeb665e45aea93a945a01474cbce3b7da_96.webp",
      127_896_704,
      6_120,
      30,
      "library",
    ],
    [
      "1bokaNcj",
      "xaeros-minimap",
      "Xaero's Minimap",
      "thexaero",
      "Displays the map of the area around you, with waypoints and entity radar.",
      "354080f65407e49f486fcf9c4580e82c45ae63b8_96.webp",
      117_776_860,
      21_480,
      6,
      "adventure,transportation,utility",
    ],
  ],
  resourcepack: [
    [
      "50dA9Sha",
      "fresh-animations",
      "Fresh Animations",
      "FreshLX",
      "Make your game like the trailers! Dynamic animated entities to freshen your Minecraft experience.",
      "3132c10e9e3c73fde9799720fd3da5561071708c_96.webp",
      50_079_903,
      14_511,
      197,
      "entities,tweaks,vanilla-like",
    ],
    [
      "uvpymuxq",
      "better-leaves",
      "Motschen's Better Leaves",
      "Motschen",
      "Improves the appearance of leaves with high mod compatibility and performance!",
      "fe1a61998ae57dc6ad1a4bb028334c3c3925d22f_96.webp",
      21_141_823,
      5_354,
      31,
      "models,realistic,vanilla-like",
    ],
    [
      "RRxvWKNC",
      "low-on-fire",
      "Low On Fire",
      "Haikis",
      "Low fire on your screen! Vanilla friendly.",
      "06a7e7691a6da41798c255108a8563d7ffea171d.png",
      16_812_200,
      3_788,
      118,
      "16x,tweaks,utility",
    ],
    [
      "dspVZXKP",
      "fast-better-grass",
      "Fast Better Grass",
      "robotkoer",
      "Makes grass and related blocks use the top texture on the sides.",
      "5de0774f423d220fcd4e635b91f2bdfcb8a2b910.png",
      13_711_909,
      2_684,
      30,
      "decoration,models,tweaks",
    ],
    [
      "ItHr72Fy",
      "fullbright-ub",
      "Fullbright UB",
      "worldresourcepack",
      "Experience Fullbright UB in Sodium, OptiFine and Vanilla.",
      "c28c5e4ae4e1e5ac726ff192855d16d2c15c83e2_96.webp",
      11_932_437,
      2_667,
      26,
      "core-shaders,vanilla-like",
    ],
    [
      "2YyNMled",
      "dramatic-skys",
      "Dramatic Skys",
      "thebaum64",
      "HD skies with realistic clouds and lighting. Works with any other resource pack.",
      "75af81c8e31ad9b2848bff6fc40c9d7232f3c3b0_96.webp",
      11_262_986,
      3_745,
      27,
      "environment,realistic,tweaks",
    ],
  ],
  shader: [
    [
      "HVnmMxH1",
      "complementary-reimagined",
      "Complementary Shaders - Reimagined",
      "EminGT",
      "Preserving the elements of Minecraft with exceptional quality, detail, and performance.",
      "79cb7c8123bbc54945305b2ebad6b8881efdf5f8_96.webp",
      70_637_269,
      11_200,
      30,
      "colored-lighting,vanilla-like",
    ],
    [
      "R6NEzAwj",
      "complementary-unbound",
      "Complementary Shaders - Unbound",
      "EminGT",
      "Transforming the visuals of Minecraft with exceptional quality, detail, and performance.",
      "c85ce4049aac76360d2cd24fd9a7003de01ef312_96.webp",
      45_894_516,
      5_985,
      30,
      "colored-lighting,fantasy",
    ],
    [
      "Q1vvjJYV",
      "bsl-shaders",
      "BSL Shaders",
      "CaptTatsu",
      "Shaderpack for Minecraft: Java Edition. It's bright, colorful, and distinct.",
      "2a611a3cb434fb52fb81fa5dace13c5d8b67e55d_96.webp",
      30_994_223,
      6_231,
      24,
      "bloom,cartoon,low",
    ],
    [
      "lLqFfGNs",
      "photon-shader",
      "Photon Shaders",
      "sixthsurge",
      "A gameplay-focused shader pack with a semi-realistic style.",
      "39cb5f12e7dcc68d6cb666f225fcb2b801dd70fb_96.webp",
      28_538_301,
      4_282,
      184,
      "atmosphere,colored-lighting,semi-realistic",
    ],
    [
      "izsIPI7a",
      "makeup-ultra-fast-shaders",
      "MakeUp - Ultra Fast",
      "KDXavier",
      "The best quality and performance ratio, adaptable to any computer.",
      "a08432baa86b8ffd58c08f4b3a001ef976ff764d_96.webp",
      13_691_802,
      2_390,
      11,
      "potato,semi-realistic,shadows",
    ],
  ],
  modpack: [
    [
      "1KVo5zza",
      "fabulously-optimized",
      "Fabulously Optimized",
      "robotkoer",
      "Beautiful graphics, speedy performance and familiar features in a simple package.",
      "d8152911f8fd5d7e9a8c499fe89045af81fe816e_96.webp",
      18_632_180,
      4_961,
      4,
      "lightweight,multiplayer,optimization",
    ],
    [
      "5FFgwNNP",
      "cobblemon-fabric",
      "Cobblemon Official Modpack [Fabric]",
      "CobbledStudios",
      "The official modpack of the Cobblemon mod, for Fabric!",
      "e7f9ee2e9d361623847853fe2ddce42f519ee64f.png",
      11_280_915,
      2_700,
      4,
      "adventure,lightweight,multiplayer",
    ],
    [
      "ch7UHY2J",
      "sodiumplus",
      "Sodium Plus",
      "NoSadBeHappy",
      "A client-side optimization modpack with a few extra tweaks.",
      "cf0150ba8d6c01144974709a27b23bba93c0fc9e.png",
      2_841_258,
      639,
      26,
      "lightweight,multiplayer,optimization",
    ],
    [
      "1ocGzRHv",
      "vanilla-perfected",
      "Vanilla Perfected",
      "demonjoeTV",
      "A compilation of Vanilla Plus mods and packs to perfect the Minecraft experience.",
      "ebec737126db7637001194ca5560ae58413d338f_96.webp",
      2_633_012,
      2_543,
      7,
      "adventure,lightweight,optimization",
    ],
    [
      "shFhR8Vx",
      "better-mc-fabric-bmc2",
      "Better MC [FABRIC] - BMC2",
      "SHXRKIE",
      "A proper Vanilla+ modpack. Don't play Vanilla, play this!",
      "a19c2bcb51d38f32f138d3607e91cb2b7b8e387f_96.webp",
      3_642_654,
      1_475,
      92,
      "adventure,combat,optimization",
    ],
  ],
};

const hits: Record<ContentProjectType, ContentHit[]> = Object.fromEntries(
  Object.entries(rows).map(([type, list]) => [
    type,
    list.map(([id, slug, title, author, description, icon, downloads, follows, age, cats]) => ({
      projectId: id,
      slug,
      title,
      author,
      description,
      iconUrl: cdn(id, icon),
      downloads,
      follows,
      dateModified: daysAgo(age),
      categories: cats.split(","),
    })),
  ]),
) as Record<ContentProjectType, ContentHit[]>;

const allHits = Object.entries(hits).flatMap(([type, list]) =>
  list.map((h) => ({ ...h, type: type as ContentProjectType })),
);

const categoryNames: Record<ContentProjectType, Array<[string, string]>> = {
  datapack: [
    ["adventure", "categories"],
    ["utility", "categories"],
    ["worldgen", "categories"],
  ],
  mod: [
    ["adventure", "categories"],
    ["decoration", "categories"],
    ["equipment", "categories"],
    ["food", "categories"],
    ["game-mechanics", "categories"],
    ["library", "categories"],
    ["magic", "categories"],
    ["management", "categories"],
    ["mobs", "categories"],
    ["optimization", "categories"],
    ["social", "categories"],
    ["storage", "categories"],
    ["technology", "categories"],
    ["transportation", "categories"],
    ["utility", "categories"],
    ["worldgen", "categories"],
  ],
  resourcepack: [
    ["decoration", "categories"],
    ["realistic", "categories"],
    ["simplistic", "categories"],
    ["themed", "categories"],
    ["tweaks", "categories"],
    ["utility", "categories"],
    ["vanilla-like", "categories"],
    ["16x", "resolutions"],
    ["32x", "resolutions"],
  ],
  shader: [
    ["cartoon", "categories"],
    ["fantasy", "categories"],
    ["realistic", "categories"],
    ["semi-realistic", "categories"],
    ["vanilla-like", "categories"],
    ["potato", "performance impact"],
    ["low", "performance impact"],
    ["medium", "performance impact"],
    ["high", "performance impact"],
  ],
  modpack: [
    ["adventure", "categories"],
    ["challenging", "categories"],
    ["combat", "categories"],
    ["kitchen-sink", "categories"],
    ["lightweight", "categories"],
    ["magic", "categories"],
    ["multiplayer", "categories"],
    ["optimization", "categories"],
    ["quests", "categories"],
    ["technology", "categories"],
  ],
};

const sodiumBody = `**Sodium**, Minecraft istemcisi için güçlü bir optimizasyon modudur. Kare hızını büyük ölçüde artırır, takılmaları azaltır ve birçok görsel hatayı düzeltir.

<center>
 <img src="https://cdn.modrinth.com/data/AANobbMI/images/5ca2e8f6c72f3d543dd3dc95b706964c5678d80e.png" alt="Sodium karşılaştırması">
</center>

<h2 id="kurulum">Kurulum</h2>

1. Kurulumunda **Fabric** ya da **NeoForge** olduğundan emin ol.
2. Sodium'u yükle, gerisini launcher halleder.

> Sodium, OptiFine ile birlikte çalışmaz. Shader için **Iris** kullan.

| Ayar | Önerilen |
| --- | --- |
| Görüş mesafesi | 12 |
| Yumuşak ışık | Açık |

<script>alert("bu satır hiçbir zaman çalışmaz")</script>
<img src="https://example.com/izlenen-gorsel.png" alt="izin verilmeyen sunucudan görsel">

[Kaynak kod](https://github.com/CaffeineMC/sodium) ve [Discord](https://caffeinemc.net/discord) bağlantıları sistem tarayıcısında açılır.`;

const sodiumGallery = [
  ["d84313e6f57dc9e7896961dbd2dfc2689d482758_350.webp", "Su altı ışıklandırması"],
  ["6b0e58705156ba67a6d97a74b9f9ac05da69f502_350.webp", "Biyom geçişleri"],
  ["b681a9e87daa53a0e85336a894db70427007149b_350.webp", "Sıvı görüntüleme"],
  ["56c4599efcb529a19aed0e118caad4178d7fd540.webp", "Blok gölgelendirme"],
].map(([file, title], i) => ({
  url: cdn("AANobbMI", `images/${file}`),
  title,
  description: "Sodium birçok görsel hatayı düzeltirken oyunu hızlandırır.",
  featured: i === 0,
}));

// --- Installed add-ons per instance -----------------------------------------------------------

/** Mods the base mock instances have (same file names as mock.ts). */
const baseMods: Array<[string, string, string, boolean]> = [
  ["Fabric API", "0.119.2", "P7dR8mSH", true],
  ["Sodium", "0.6.13", "AANobbMI", true],
  ["Lithium", "0.15.1", "gvQqBUqZ", true],
  ["Iris Shaders", "1.8.11", "YL57xq9U", true],
  ["Xaero's Minimap", "25.2.10", "1bokaNcj", true],
  ["Mod Menu", "13.0.3", "mOgUt4GM", true],
  ["Entity Culling", "1.7.4", "NNAgCjsB", true],
  ["FerriteCore", "7.0.3", "uXXizFIs", true],
  ["ImmediatelyFast", "1.6.5", "5ZwdcRci", true],
];

const fileNameOf = (name: string, version: string) =>
  `${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-${version}.jar`;

/** Mods installed through the mock browser, shown by mock.ts in the mods tab. */
export const mockExtraMods = new Map<string, ModEntry[]>();
const installed = new Map<string, InstalledContent[]>();
const key = (instanceId: string, kind: ContentKind) => `${instanceId}:${kind}`;

function hitById(projectId: string) {
  return allHits.find((h) => h.projectId === projectId);
}

function initialEntries(instanceId: string, kind: ContentKind): InstalledContent[] {
  if (kind !== "mod") return [];
  const count = instanceId === "projects" ? baseMods.length : instanceId === "yaratici" ? 5 : 0;
  return baseMods.slice(0, count).map(([name, version, projectId, packOwned]) => {
    const hit = hitById(projectId);
    const outdated =
      instanceId === "yaratici" && (projectId === "AANobbMI" || projectId === "gvQqBUqZ");
    return {
      fileName: fileNameOf(name, version),
      enabled: true,
      sizeBytes: 180_000 + name.length * 41_000,
      projectId,
      versionId: `${projectId}-current`,
      versionNumber: version,
      title: hit?.title ?? name,
      iconUrl: hit?.iconUrl,
      fromPack: instanceId === "projects" && packOwned,
      update: outdated
        ? {
            versionId: `${projectId}-new`,
            versionNumber: projectId === "AANobbMI" ? "0.6.14" : "0.15.2",
            versionType: "release",
            datePublished: daysAgo(3),
          }
        : undefined,
    };
  });
}

function entries(instanceId: string, kind: ContentKind): InstalledContent[] {
  const k = key(instanceId, kind);
  let list = installed.get(k);
  if (!list) {
    list = initialEntries(instanceId, kind);
    installed.set(k, list);
  }
  return list;
}

const dependencies: Record<string, string[]> = {
  mOgUt4GM: ["P7dR8mSH"],
  YL57xq9U: ["AANobbMI"],
  "1eAoo2KR": ["P7dR8mSH"],
};

async function simulate(onProgress: (p: JobProgress) => void, totalFiles: number, ms = 1600) {
  onProgress({ stage: "resolving", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
  await delay(450);
  const steps = 16;
  for (let i = 1; i <= steps; i++) {
    await delay(ms / steps);
    onProgress({
      stage: "downloading",
      percent: (i / steps) * 100,
      doneFiles: Math.round((i / steps) * totalFiles),
      totalFiles,
      bytesPerSec: 3_800_000 + Math.random() * 2_000_000,
    });
  }
  onProgress({ stage: "finishing", doneFiles: totalFiles, totalFiles, bytesPerSec: 0 });
  await delay(300);
}

const cancelled = new Set<string>();

function addEntry(instanceId: string, kind: ContentKind, projectId: string, dependency: boolean) {
  const list = entries(instanceId, kind);
  const hit = hitById(projectId);
  const title = hit?.title ?? projectId;
  const version = "1.0.0";
  const fileName =
    kind === "mod" ? fileNameOf(title, version) : `${title.replace(/[^A-Za-z0-9]+/g, "_")}.zip`;
  const old = list.findIndex((e) => e.projectId === projectId);
  if (old >= 0) list.splice(old, 1);
  list.push({
    fileName,
    enabled: true,
    sizeBytes: 420_000,
    projectId,
    versionId: `${projectId}-new`,
    versionNumber: version,
    title,
    iconUrl: hit?.iconUrl,
    fromPack: false,
  });
  if (kind === "mod") {
    const extra = mockExtraMods.get(instanceId) ?? [];
    mockExtraMods.set(instanceId, [
      ...extra.filter((m) => m.projectId !== projectId),
      {
        fileName,
        name: title,
        version,
        authors: [hit?.author ?? ""],
        description: hit?.description ?? "",
        enabled: true,
        iconUrl: hit?.iconUrl,
        fromPack: false,
        sizeBytes: 420_000,
        projectId,
      },
    ]);
  }
  return { projectId, title, versionNumber: version, fileName, dependency };
}

function versionsFor(projectId: string): ContentVersion[] {
  const base = projectId === "AANobbMI" ? [0, 6, 14] : [1, 4, 2];
  return Array.from({ length: 6 }, (_, i) => {
    const patch = (base[2] ?? 0) - i;
    const number = `${base[0]}.${base[1]}.${Math.max(0, patch)}`;
    return {
      id: i === 0 ? `${projectId}-new` : `${projectId}-v${i}`,
      projectId,
      name: number,
      versionNumber: number,
      versionType: i === 1 ? ("beta" as const) : ("release" as const),
      datePublished: daysAgo(3 + i * 19),
      downloads: 900_000 - i * 120_000,
      gameVersions: ["1.21.1"],
      loaders: ["fabric", "quilt"],
      fileName: `${projectId.toLowerCase()}-${number}.jar`,
      fileSize: 1_200_000 - i * 9_000,
      requiredDependencies: dependencies[projectId]?.length ?? 0,
    };
  });
}

const packPreview: PackPreview = {
  format: "mrpack",
  name: "Fabulously Optimized",
  packVersion: "6.4.0",
  summary: "Beautiful graphics, speedy performance and familiar features in a simple package.",
  mcVersion: "1.21.1",
  loader: "fabric",
  loaderVersion: "0.16.14",
  downloadFiles: 64,
  downloadBytes: 48_200_000,
  bundledFiles: 23,
  manualFiles: 0,
  unsupported: [],
  optional: [
    {
      path: "mods/distanthorizons-2.3.0-b-1.21.1.jar",
      fileName: "distanthorizons-2.3.0-b-1.21.1.jar",
      folder: "mods",
      size: 18_400_000,
      projectId: "uCdwusMi",
    },
    {
      path: "mods/zoomify-2.14.1.jar",
      fileName: "zoomify-2.14.1.jar",
      folder: "mods",
      size: 410_000,
      projectId: "w7ThoJFB",
    },
    {
      path: "shaderpacks/ComplementaryReimagined_r5.4.zip",
      fileName: "ComplementaryReimagined_r5.4.zip",
      folder: "shaderpacks",
      size: 1_900_000,
      projectId: "HVnmMxH1",
    },
  ],
};

/** Mock instances made from a Modrinth pack (the creative sample has one). */
const packOf = new Map<string, ModpackInfo>([
  [
    "yaratici",
    {
      projectId: "1KVo5zza",
      versionId: "fo-630",
      versionNumber: "6.3.0",
      title: "Fabulously Optimized",
      iconUrl: cdn("1KVo5zza", "icon.png"),
    },
  ],
]);

const updatePlan: ModpackUpdatePlan = {
  fromVersion: "6.3.0",
  toVersion: "6.4.0",
  added: [
    { path: "mods/sodium-extra-0.6.0.jar", name: "Sodium Extra", folder: "mods" },
    { path: "mods/language-reload-1.7.0.jar", name: "Language Reload", folder: "mods" },
    { path: "config/sodium-extra.json", name: "sodium-extra.json", folder: "config" },
  ],
  updated: [
    { path: "mods/sodium-0.6.5.jar", name: "Sodium", folder: "mods" },
    { path: "mods/iris-1.8.1.jar", name: "Iris Shaders", folder: "mods" },
    { path: "mods/fabric-api-0.110.0.jar", name: "Fabric API", folder: "mods" },
    { path: "mods/modmenu-11.0.3.jar", name: "Mod Menu", folder: "mods" },
    { path: "config/yosbr/options.txt", name: "options.txt", folder: "config" },
  ],
  removed: [{ path: "mods/lazydfu-0.1.3.jar", name: "LazyDFU", folder: "mods" }],
  kept: [
    {
      item: { path: "mods/zoomify-2.14.1.jar", name: "Zoomify", folder: "mods" },
      reason: "disabled",
    },
    {
      item: { path: "config/iris.properties", name: "iris.properties", folder: "config" },
      reason: "changed",
    },
  ],
  downloadBytes: 9_600_000,
  loaderFrom: "0.16.9",
  loaderTo: "0.16.14",
};

const worldPacks = new Map<string, Datapack[]>();

export function createMockContent(deps: {
  createInstance: (spec: NewInstance) => Promise<InstanceSummary>;
}): ContentApi {
  const api: ContentApi = {
    async search(params) {
      await delay(380);
      const q = params.query.trim().toLocaleLowerCase("tr");
      if (q === "çevrimdışı" || q === "offline") {
        throw new MockCommandError(
          "MODRINTH-AG",
          "Modrinth'e ulaşılamadı. İnternet bağlantını kontrol edip tekrar dene.",
        );
      }
      let list = hits[params.projectType].filter(
        (h) =>
          (!q ||
            h.title.toLocaleLowerCase("tr").includes(q) ||
            h.description.toLocaleLowerCase("tr").includes(q)) &&
          (!params.category || h.categories.includes(params.category)),
      );
      if (params.sort === "downloads") list = [...list].sort((a, b) => b.downloads - a.downloads);
      if (params.sort === "updated")
        list = [...list].sort((a, b) => b.dateModified.localeCompare(a.dateModified));
      return {
        hits: list.slice(params.offset, params.offset + params.limit),
        offset: params.offset,
        total: list.length,
      };
    },
    async categories() {
      await delay(120);
      return Object.entries(categoryNames).flatMap(([type, list]) =>
        list.map(
          ([name, header]): ContentCategory => ({
            name,
            projectType: type as ContentProjectType,
            header,
          }),
        ),
      );
    },
    async project(id): Promise<ContentProject> {
      await delay(320);
      const hit = allHits.find((h) => h.projectId === id || h.slug === id);
      if (!hit) throw new MockCommandError("BULUNAMADI", "Bu içerik Modrinth'te bulunamadı.");
      const sodium = hit.projectId === "AANobbMI";
      return {
        id: hit.projectId,
        slug: hit.slug,
        projectType: hit.type,
        title: hit.title,
        description: hit.description,
        body: sodium
          ? sodiumBody
          : `${hit.description}\n\n## Özellikler\n\n- Hafif ve hızlı\n- Diğer modlarla uyumlu\n- Ayarları oyun içinden değiştirilebilir`,
        iconUrl: hit.iconUrl,
        downloads: hit.downloads,
        followers: hit.follows,
        published: daysAgo(1400),
        updated: hit.dateModified,
        categories: hit.categories,
        gameVersions: ["1.20.1", "1.21.1", "26.3"],
        loaders: ["fabric", "quilt", "neoforge"],
        license: sodium ? "Polyform-Shield-1.0.0" : "MIT",
        links: [
          { kind: "source", url: "https://github.com/CaffeineMC/sodium" },
          { kind: "issues", url: "https://github.com/CaffeineMC/sodium/issues" },
          { kind: "discord", url: "https://caffeinemc.net/discord" },
        ],
        gallery: sodium ? sodiumGallery : [],
      };
    },
    async versions(projectId) {
      await delay(260);
      return versionsFor(projectId);
    },
    async scan(instanceId, kind, checkUpdates) {
      await delay(checkUpdates ? 900 : 220);
      return {
        entries: entries(instanceId, kind).map((e) => ({
          ...e,
          update: checkUpdates ? e.update : undefined,
        })),
        offline: false,
      };
    },
    async install(job, onProgress): Promise<ContentInstallResult> {
      cancelled.delete(job.jobId);
      const deps = (dependencies[job.projectId] ?? []).filter(
        (dep) => !entries(job.instanceId, "mod").some((e) => e.projectId === dep),
      );
      await simulate(onProgress, 1 + deps.length);
      if (cancelled.has(job.jobId)) throw new MockCommandError("IPTAL", "İşlem iptal edildi.");
      const result: ContentInstallResult = {
        installed: [addEntry(job.instanceId, job.kind, job.projectId, false)],
        alreadyInstalled: [],
        replaced: [],
        unresolved: [],
        conflicts: [],
      };
      for (const dep of deps) result.installed.push(addEntry(job.instanceId, "mod", dep, true));
      return result;
    },
    async update(job, onProgress): Promise<ContentInstallResult> {
      const list = entries(job.instanceId, job.kind).filter(
        (e) => e.update && (job.fileNames.length === 0 || job.fileNames.includes(e.fileName)),
      );
      await simulate(onProgress, list.length || 1, 1200);
      const result: ContentInstallResult = {
        installed: [],
        alreadyInstalled: [],
        replaced: [],
        unresolved: [],
        conflicts: [],
      };
      for (const entry of list) {
        if (!entry.update) continue;
        result.replaced.push(entry.fileName);
        entry.versionNumber = entry.update.versionNumber;
        entry.versionId = entry.update.versionId;
        result.installed.push({
          projectId: entry.projectId ?? "",
          title: entry.title,
          versionNumber: entry.update.versionNumber,
          fileName: entry.fileName,
          dependency: false,
        });
        entry.update = undefined;
      }
      return result;
    },
    async remove(instanceId, kind, fileName) {
      await delay(200);
      const list = entries(instanceId, kind);
      const index = list.findIndex((e) => e.fileName === fileName);
      if (index >= 0) list.splice(index, 1);
      const extra = mockExtraMods.get(instanceId);
      if (extra)
        mockExtraMods.set(
          instanceId,
          extra.filter((m) => m.fileName !== fileName),
        );
    },
    async addFiles(_instanceId, _kind, paths) {
      await delay(300);
      return { added: paths.map((p) => p.split(/[\\/]/).pop() ?? p), skipped: [] };
    },
    async cancel(jobId) {
      cancelled.add(jobId);
    },
    async previewPack(path) {
      await delay(500);
      if (!/\.(mrpack|zip)$/i.test(path)) {
        throw new MockCommandError(
          "PAKET-TANINMADI",
          "Bu dosya tanınan bir mod paketi değil. .mrpack, Prism/MultiMC ya da CurseForge zip dosyası seç.",
        );
      }
      if (/curse/i.test(path)) {
        return {
          ...packPreview,
          format: "curseForge",
          name: "All the Mods 9",
          packVersion: "0.3.2",
          summary: undefined,
          author: "ATMTeam",
          mcVersion: "1.20.1",
          loader: "forge",
          loaderVersion: "47.3.0",
          downloadFiles: 0,
          downloadBytes: 0,
          bundledFiles: 412,
          manualFiles: 396,
        };
      }
      return packPreview;
    },
    async importPack(job, onProgress) {
      cancelled.delete(job.jobId);
      onProgress({ stage: "extracting", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
      await delay(900);
      const steps = 20;
      for (let i = 1; i <= steps; i++) {
        if (cancelled.has(job.jobId)) throw new MockCommandError("IPTAL", "İşlem iptal edildi.");
        await delay(110);
        onProgress({
          stage: "downloading",
          percent: (i / steps) * 100,
          doneFiles: Math.round((i / steps) * 64),
          totalFiles: 64,
          bytesPerSec: 6_200_000,
        });
      }
      onProgress({ stage: "finishing", doneFiles: 64, totalFiles: 64, bytesPerSec: 0 });
      const instance = await deps.createInstance({
        name: job.name || packPreview.name,
        icon: job.icon,
        mcVersion: packPreview.mcVersion,
        loader: packPreview.loader,
        loaderVersion: packPreview.loaderVersion,
      });
      return {
        instance,
        manualMods: [],
        manualCount: 0,
        files: 87,
        skipped: 0,
        unsupported: [],
      };
    },
    async installModpack(job, onProgress) {
      cancelled.delete(job.jobId);
      for (let i = 1; i <= 8; i++) {
        await delay(90);
        onProgress({
          stage: "pack",
          percent: (i / 8) * 100,
          doneFiles: 0,
          totalFiles: 1,
          bytesPerSec: 5_000_000,
        });
      }
      const hit = hitById(job.projectId);
      return api.importPack(
        { path: "pack.mrpack", name: job.name || hit?.title, icon: job.icon, jobId: job.jobId },
        onProgress,
      );
    },
    async exportPack(_instanceId, options, jobId, onProgress) {
      cancelled.delete(jobId);
      onProgress({ stage: "identifying", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
      await delay(900);
      onProgress({ stage: "writing", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
      await delay(700);
      if (cancelled.has(jobId)) throw new MockCommandError("IPTAL", "İşlem iptal edildi.");
      return {
        path: options.path,
        hostedFiles: 11,
        bundledFiles: options.includeWorlds ? 214 : 38,
        bundledBytes: options.includeWorlds ? 182_000_000 : 1_400_000,
        offline: false,
      };
    },
    async modpackInfo(instanceId) {
      await delay(120);
      return packOf.get(instanceId) ?? null;
    },
    async modpackUpdateCheck(instanceId) {
      await delay(700);
      const info = packOf.get(instanceId);
      if (!info || info.versionNumber === updatePlan.toVersion) return null;
      return {
        versionId: "fo-640",
        versionNumber: updatePlan.toVersion,
        name: `Fabulously Optimized ${updatePlan.toVersion}`,
        publishedAt: daysAgo(3),
      };
    },
    async modpackUpdatePlan(_instanceId, _versionId, jobId, onProgress) {
      cancelled.delete(jobId);
      for (let i = 1; i <= 6; i++) {
        await delay(120);
        onProgress({
          stage: "pack",
          percent: (i / 6) * 100,
          doneFiles: 0,
          totalFiles: 1,
          bytesPerSec: 4_000_000,
        });
      }
      onProgress({ stage: "checking", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
      await delay(600);
      if (cancelled.has(jobId)) throw new MockCommandError("IPTAL", "İşlem iptal edildi.");
      return structuredClone(updatePlan);
    },
    async modpackUpdateApply(instanceId, _versionId, jobId, onProgress) {
      cancelled.delete(jobId);
      await simulate(onProgress, 4, 1600);
      onProgress({ stage: "finishing", doneFiles: 4, totalFiles: 4, bytesPerSec: 0 });
      await delay(500);
      const info = packOf.get(instanceId);
      if (info) {
        packOf.set(instanceId, {
          ...info,
          versionNumber: updatePlan.toVersion,
          versionId: "fo-640",
        });
      }
      return {
        versionNumber: updatePlan.toVersion,
        added: updatePlan.added.length,
        updated: updatePlan.updated.length,
        removed: updatePlan.removed.length,
        kept: updatePlan.kept.length,
      };
    },
    async projectBriefs(ids) {
      await delay(260);
      const titles: Record<string, string> = {
        uCdwusMi: "Distant Horizons",
        w7ThoJFB: "Zoomify",
        HVnmMxH1: "Complementary Shaders - Reimagined",
      };
      return ids
        .map((id) => ({ id, title: titles[id] ?? hitById(id)?.title ?? "" }))
        .filter((p) => p.title);
    },
    async datapacks(instanceId, world) {
      await delay(200);
      const key = `${instanceId}/${world}`;
      if (!worldPacks.has(key)) {
        worldPacks.set(
          key,
          world.toLocaleLowerCase("tr").includes("ejderha")
            ? [
                {
                  fileName: "Incendium_1.21.x_v5.4.4.zip",
                  name: "Incendium_1.21.x_v5.4.4",
                  description: "Nether overhaul",
                  sizeBytes: 3_400_000,
                  folder: false,
                },
                {
                  fileName: "spawn-kurallari",
                  name: "spawn-kurallari",
                  description: "Sunucu kuralları",
                  sizeBytes: 12_000,
                  folder: true,
                },
              ]
            : [],
        );
      }
      return structuredClone(worldPacks.get(key) ?? []);
    },
    async installDatapack(job, onProgress) {
      cancelled.delete(job.jobId);
      onProgress({ stage: "resolving", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
      await delay(350);
      await simulate(onProgress, 1, 900);
      if (cancelled.has(job.jobId)) throw new MockCommandError("IPTAL", "İşlem iptal edildi.");
      const hit = hitById(job.projectId);
      const name = `${hit?.title ?? "Datapack"}_1.21.1`;
      const pack: Datapack = {
        fileName: `${name}.zip`,
        name,
        description: hit?.description,
        sizeBytes: 1_800_000,
        folder: false,
      };
      const key = `${job.instanceId}/${job.world}`;
      worldPacks.set(key, [...(worldPacks.get(key) ?? []), pack]);
      return pack;
    },
    async removeDatapack(instanceId, world, fileName) {
      await delay(180);
      const key = `${instanceId}/${world}`;
      worldPacks.set(
        key,
        (worldPacks.get(key) ?? []).filter((p) => p.fileName !== fileName),
      );
    },
    async pickFiles(kind) {
      await delay(150);
      if (kind === "modpack")
        return ["C:/Users/Oyuncu/Downloads/Fabulously Optimized 6.4.0.mrpack"];
      if (kind === "mod") return ["C:/Users/Oyuncu/Downloads/appleskin-fabric-3.0.6.jar"];
      return ["C:/Users/Oyuncu/Downloads/Pack.zip"];
    },
    async pickSavePath(defaultName) {
      await delay(150);
      return `C:/Users/Oyuncu/Desktop/${defaultName}`;
    },
    onFileDrop(listener: (event: FileDropEvent) => void) {
      const over = (e: DragEvent) => {
        e.preventDefault();
        listener({ type: "over" });
      };
      const leave = (e: DragEvent) => {
        if (e.relatedTarget === null) listener({ type: "leave" });
      };
      const drop = (e: DragEvent) => {
        e.preventDefault();
        const names = Array.from(e.dataTransfer?.files ?? []).map((f) => `C:/Mock/${f.name}`);
        listener({ type: "drop", paths: names });
      };
      window.addEventListener("dragover", over);
      window.addEventListener("dragleave", leave);
      window.addEventListener("drop", drop);
      return () => {
        window.removeEventListener("dragover", over);
        window.removeEventListener("dragleave", leave);
        window.removeEventListener("drop", drop);
      };
    },
  };
  return api;
}
