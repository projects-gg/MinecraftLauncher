// Ctrl+K palette search and the small rules behind its actions. Players type the problem in their
// own words ("kasıyor", "ram", "ip", "skinimi değiştir"), so every item carries search words
// beside its label: Turkish first, plus the English words gamers use. They are search data, not
// shown text, so they live here rather than in the locale files and work in either language.
// Pure: palette-actions.test.ts checks the ranking.
import { fold } from "@pl/ui/search";
import { isAfkIdle, isAfkRunning } from "@/features/accounts/afk-match";
import type { AfkAccount } from "@/lib/api";
import type { CheckResult } from "@/stores/update";

/** Search words of a palette item. `related` words find it too, ranked below items they name. */
export interface PaletteWords {
  keywords: readonly string[];
  related?: readonly string[];
}

export const PALETTE_WORDS = {
  // Pages
  home: { keywords: ["ana sayfa", "anasayfa", "başlangıç", "home"] },
  library: { keywords: ["kurulumlar", "kurulum", "profiller", "library", "instances"] },
  news: { keywords: ["duyuru", "duyurular", "yenilikler", "etkinlik", "etkinlikler", "news"] },
  afk: { keywords: ["afk", "bot", "botlar"] },
  settings: {
    keywords: ["seçenekler", "ayar", "settings", "options"],
    // Settings opens on Genel, where the language is chosen.
    related: ["dil", "language", "ingilizce", "english", "türkçe", "turkish"],
  },

  // In-game lag is what the graphics preset fixes, so the lag words lead to Settings → Grafik ve
  // FPS (the preset of the instance Play launches, one tap; HOME1); the launcher's own
  // Performance mode keeps "launcher", effects and blur.
  graphics: {
    keywords: [
      "grafik",
      "grafikler",
      "görüntü ayarı",
      "görüntü ayarları",
      "video ayarları",
      "görüş mesafesi",
      "render",
      "render distance",
      "chunk",
      "kalite",
      "ultra",
      "akıcı",
      "dengeli",
      "güzel",
      "bulut",
      "parçacık",
      "fps",
      "kasma",
      "kasıyor",
      "kasılma",
      "lag",
      "donma",
      "donuyor",
      "takılma",
      "takılıyor",
      "yavaş",
      "düşük",
      "graphics",
      "quality",
      "stutter",
      "boost",
      // "fps arttırma": the words players put after "fps" (RAM has them too).
      "arttır",
      "artır",
      "yükselt",
    ],
  },

  // The selected instance (shown only while searching)
  memory: {
    keywords: [
      "ram",
      "bellek",
      "hafıza",
      "memory",
      "gb",
      "xmx",
      "ayarla",
      "arttır",
      "artır",
      "yükselt",
      "ver",
    ],
    related: ["kasıyor", "kasma", "donuyor", "lag", "fps", "çöküyor", "çökme", "crash", "yetersiz"],
  },
  mods: {
    keywords: [
      "mod",
      "modlar",
      "eklenti",
      "optifine",
      "sodium",
      "iris",
      "forge",
      "fabric",
      "quilt",
      "neoforge",
      "modrinth",
      // The mod center became the Modlar tab's sections (MODHUB2): its words still find it.
      "mod merkezi",
      "önerilen",
      "keşfet",
      "mod center",
    ],
  },
  resourcePacks: {
    keywords: ["kaynak paketi", "doku", "doku paketi", "texture", "texture pack", "resource pack"],
  },
  shaders: {
    keywords: ["shader", "gölgelendirici", "gölge", "ışık", "rtx", "bsl", "complementary"],
  },
  worlds: {
    keywords: ["dünya", "world", "harita", "map", "save", "yedek", "yedeği", "yedekle", "backup"],
  },
  screenshots: {
    keywords: ["ekran görüntüsü", "ss", "foto", "fotoğraf", "resim", "screenshot", "görüntü"],
  },
  logs: {
    keywords: ["log", "loglar", "kayıt", "crash", "error", "rapor"],
    // A crash or an error is the troubleshooter's first (HELP1); the logs come right after.
    related: ["çökme", "çöküyor", "hata", "açılmıyor", "başlamıyor", "kapanıyor", "atıyor"],
  },
  folder: {
    keywords: ["klasör", "dosya", "dosyalar", "folder", ".minecraft", "dizin", "konum"],
  },

  // Accounts
  appearance: {
    keywords: [
      "skin",
      "görünüm",
      "kostüm",
      "pelerin",
      "cape",
      "karakter",
      "kıyafet",
      "kafa",
      "dış görünüş",
    ],
  },
  avatar: {
    keywords: [
      "avatar",
      "profil resmi",
      "resim",
      "fotoğraf",
      "kafa",
      "görünüm",
      "skin",
      "kostüm",
      "karakter",
    ],
  },
  addAccount: {
    keywords: ["giriş", "giriş yap", "oturum aç", "login", "microsoft", "premium"],
    // "Oyuncu adıyla oyna" (KEYS2): the old word and the English ones still find it.
    related: ["kullanıcı adı", "çevrimdışı", "offline", "username", "cracked"],
  },
  accountCenter: { keywords: ["hesap", "hesaplar", "çıkış yap", "logout", "accounts"] },
  // SKIN1: the Projects skin of a username account, and the game account's link on Projects.
  projectsSkin: {
    keywords: [
      "skin",
      "görünüm",
      "projects skini",
      "skin değiştir",
      "kostüm",
      "karakter",
      "kıyafet",
      "dış görünüş",
    ],
  },
  projectsLink: {
    keywords: ["eşle", "eşleme", "discord eşle", "hesabı eşle", "doğrula", "doğrulama", "link"],
    related: ["discord", "doğrulanmış rol", "verified"],
  },
  friends: { keywords: ["arkadaş", "arkadaşlar", "discord", "friends"] },
  addFriend: { keywords: ["arkadaş ekle", "kod", "davet", "PRJ"] },
  // World sharing (HOST1): what players type for playing their own world together, the game's
  // own "LAN'a aç" among them.
  hostWorld: {
    keywords: [
      "arkadaşlarla oyna",
      "arkadaşlarınla oyna",
      "dünyamı aç",
      "dünyayı aç",
      "dünya paylaş",
      "dünyamı paylaş",
      "lan",
      "lana aç",
      "co-op",
      "coop",
      "host",
    ],
    related: ["birlikte", "multiplayer", "çok oyunculu"],
  },

  // Settings
  performance: {
    keywords: [
      "performans",
      "fps",
      "kasma",
      "kasıyor",
      "kasılma",
      "lag",
      "laggy",
      "donma",
      "donuyor",
      "takılma",
      "takılıyor",
      "yavaş",
      "düşük",
      "düşüyor",
      "akıcı",
      "akıcılık",
      "launcher",
      "efekt",
      "bulanıklık",
      "kıvılcım",
      "performance",
      "stutter",
      "slow",
      "smooth",
    ],
  },
  // Settings → Görünüm "Animasyonları azalt" (BETA-FB), by the words a player looks for it with.
  reduceMotion: {
    keywords: [
      "animasyon",
      "animasyonlar",
      "animasyonları azalt",
      "hareket",
      "hareketi azalt",
      "geçiş",
      "geçişler",
      "kayma",
      "baş dönmesi",
      "animation",
      "animations",
      "reduce motion",
      "motion",
    ],
  },
  java: {
    keywords: ["java", "jre", "jdk", "jvm", "javaw", "runtime"],
    related: ["açılmıyor", "başlamıyor"],
  },
  cleanup: {
    keywords: [
      "temizle",
      "temizlik",
      "disk",
      "yer",
      "çöp",
      "çöp kutusu",
      "alan",
      "depolama",
      "boşalt",
      "dolu",
      "cleanup",
      "storage",
      "space",
      "trash",
    ],
  },
  // "Sorun yardımcısı" (HELP1): the game that won't start or connect, a crash, an error. The
  // problem words lead here; the support bundle and the logs follow.
  help: {
    keywords: [
      "sorun",
      "sorun yardımcısı",
      "yardımcı",
      "yardım",
      "problem",
      "açılmıyor",
      "açılmadı",
      "başlamıyor",
      "girmiyor",
      "giremiyorum",
      "bağlanamıyorum",
      "bağlanamıyor",
      "bağlanmıyor",
      "bağlantı",
      "internet",
      "hata",
      "çöküyor",
      "çöktü",
      "çökme",
      "çalışmıyor",
      "troubleshoot",
      "troubleshooter",
      "help",
    ],
    related: ["crash", "error", "ping", "sunucu", "server", "destek", "support"],
  },
  support: {
    keywords: ["destek", "rapor", "support"],
    related: ["yardım", "sorun", "problem", "help", "hata", "log", "çökme"],
  },

  // Actions
  newInstance: {
    keywords: ["kurulum", "yeni", "oluştur", "ekle", "profil", "instance", "modpack"],
  },
  copyAddress: {
    keywords: [
      "ip",
      "ip adresi",
      "adres",
      "sunucu",
      "server",
      "play.projects.gg",
      "kopyala",
      "copy",
    ],
  },
  afkConnectAll: { keywords: ["afk", "bot", "bağla", "başlat", "connect"] },
  afkStopAll: { keywords: ["afk", "bot", "durdur", "kapat", "stop"] },
  updates: { keywords: ["güncelleme", "güncelle", "update", "yeni sürüm", "sürüm", "launcher"] },
  // The palette's own list of keyboard shortcuts.
  shortcuts: {
    keywords: [
      "kısayol",
      "kısayollar",
      "klavye",
      "tuş",
      "tuşlar",
      "tuş kombinasyonu",
      "ctrl",
      "shortcut",
      "shortcuts",
      "keyboard",
      "hotkey",
      "keybind",
    ],
  },
} satisfies Record<string, PaletteWords>;

export type PaletteWordsId = keyof typeof PALETTE_WORDS;

/** Marks a related word inside cmdk's flat keyword list (never typed: a control character). */
const RELATED = "\u0001";
const RELATED_WEIGHT = 0.5;

/** cmdk keywords of an item: its label, any extra words, then its search words. */
export function paletteKeywords(
  label: string,
  words?: PaletteWords,
  extra: readonly string[] = [],
): string[] {
  return [
    label,
    ...extra,
    ...(words?.keywords ?? []),
    ...(words?.related ?? []).map((word) => RELATED + word),
  ];
}

/**
 * Filler a player adds around the problem ("oyun neden kasıyor", "ram nasıl arttırılır"),
 * folded. Dropped only beside other words: typed alone, they still search.
 */
const STOP_WORDS = new Set([
  "nasil",
  "ne",
  "neden",
  "niye",
  "nerede",
  "nerde",
  "nereden",
  "nereye",
  "cok",
  "bir",
  "bu",
  "su",
  "o",
  "ve",
  "veya",
  "ile",
  "mi",
  "mu",
  "da",
  "de",
  "ki",
  "icin",
  "gibi",
  "kadar",
  "daha",
  "en",
  "hic",
  "her",
  "ben",
  "benim",
  "bana",
  "beni",
  "sen",
  "senin",
  "lutfen",
  "yap",
  "yapmak",
  "yapilir",
  "yapabilirim",
  "yapiyorum",
  "olur",
  "oluyor",
  "olmuyor",
  "var",
  "yok",
  "istiyorum",
  "lazim",
  "gerek",
  "acaba",
  "oyun",
  "oyunum",
  "oyunu",
  "oyunda",
  "pc",
  "bilgisayar",
  "bilgisayarim",
  "laptop",
  "how",
  "to",
  "the",
  "a",
  "an",
  "my",
  "is",
  "it",
  "i",
  "in",
  "on",
  "of",
  "for",
  "fix",
  "why",
  "what",
  "game",
]);

const tokenCache = new Map<string, string[]>();

/** Folded words (`@pl/ui/search`): "Shader'lar" → shader, lar; "play.projects.gg" → play, … */
function words(text: string): string[] {
  const cached = tokenCache.get(text);
  if (cached) return cached;
  const result = fold(text)
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);
  if (tokenCache.size > 2000) tokenCache.clear();
  tokenCache.set(text, result);
  return result;
}

function commonPrefix(a: string, b: string): number {
  const max = Math.min(a.length, b.length);
  let i = 0;
  while (i < max && a[i] === b[i]) i++;
  return i;
}

const EXACT = 3;

/**
 * How well one typed word matches an item's words: the same word, the start of one ("perf"),
 * one plus a Turkish ending ("ramim", "skinimi", "loglar"), the same stem with another ending
 * ("görüntüsü" for "görüntüleri"; six letters, so "görünüm" stays apart), or a piece inside a
 * word ("block" in "skyblock").
 */
function termScore(term: string, itemWords: readonly string[]): number {
  let best = 0;
  for (const word of itemWords) {
    let score = 0;
    if (word === term) score = EXACT;
    else if (word.startsWith(term)) score = 2;
    else if (word.length >= 3 && term.startsWith(word)) score = 1.5;
    else if (commonPrefix(word, term) >= 6) score = 1.25;
    else if (term.length >= 2 && word.includes(term)) score = 1;
    if (score > best) {
      best = score;
      if (best === EXACT) break;
    }
  }
  return best;
}

/**
 * cmdk filter (0 hides an item, higher ranks first) over the item's label and search words; the
 * internal `value` is ignored, so ids never match by chance. A phrase does not have to match
 * word for word ("shader yükle" finds the shaders), but an item matching all of its meaningful
 * words always ranks above one matching only some.
 */
export function paletteFilter(
  _value: string,
  search: string,
  keywords: readonly string[] = [],
): number {
  const typed = words(search);
  if (typed.length === 0) return 1;
  const meaningful = typed.filter((term) => !STOP_WORDS.has(term));
  const terms = meaningful.length > 0 ? meaningful : typed;
  const own: string[] = [];
  const related: string[] = [];
  for (const keyword of keywords) {
    if (keyword.startsWith(RELATED)) related.push(...words(keyword.slice(RELATED.length)));
    else own.push(...words(keyword));
  }
  let total = 0;
  let matched = 0;
  for (const term of terms) {
    const score = Math.max(termScore(term, own), termScore(term, related) * RELATED_WEIGHT);
    if (score > 0) {
      total += score;
      matched++;
    }
  }
  if (matched === 0) return 0;
  const quality = total / (terms.length * EXACT);
  return matched === terms.length ? 0.5 + quality / 2 : (quality / 2) * (matched / terms.length);
}

export interface RankedEntry {
  value: string;
  keywords: readonly string[];
}

/**
 * The palette's groups for `search`, best match first (ties keep the given order). cmdk sorts
 * the items inside a group but not the groups (cmdk 1.1.1 looks groups up by an attribute it
 * never sets), so "kasıyor" would open on "Bellek ayarı" just because its group comes first.
 */
export function rankGroups<G extends { entries: readonly RankedEntry[] }>(
  groups: readonly G[],
  search: string,
): G[] {
  if (search.trim() === "") return [...groups];
  return groups
    .map((group, index) => ({
      group,
      index,
      best: Math.max(0, ...group.entries.map((e) => paletteFilter(e.value, search, e.keywords))),
    }))
    .sort((a, b) => b.best - a.best || a.index - b.index)
    .map((row) => row.group);
}

/**
 * Which AFK actions make sense now, by the AFK page's "Buradakileri bağla/durdur" (AFK-CLEAR): none
 * without AFK accounts. Connecting takes the accounts the "Bu bilgisayarda" list shows (`folded`:
 * those whose card is their 7/24 Bulut twin's) that are idle and not turned away by the server;
 * stopping takes every client that runs here.
 */
export function afkPaletteActions(
  accounts: readonly AfkAccount[],
  folded: ReadonlySet<string> = new Set(),
): {
  connectAll: boolean;
  stopAll: boolean;
} {
  const refused = (a: AfkAccount) => a.state.kind === "error" && Boolean(a.state.cause);
  return {
    connectAll: accounts.some((a) => !folded.has(a.id) && isAfkIdle(a.state) && !refused(a)),
    stopAll: accounts.some((a) => isAfkRunning(a.state)),
  };
}

/** The toast after "Güncellemeleri denetle", worded like Settings → Güncellemeler. */
export type UpdateReport = { tone: "success" | "error" | "info"; key: string } | null;

export function updateReport(result: CheckResult): UpdateReport {
  switch (result) {
    case "upToDate":
      return { tone: "success", key: "settings.updates.upToDate" };
    case "ready":
      return { tone: "success", key: "settings.updates.ready" };
    case "unavailable":
      return { tone: "error", key: "settings.updates.unavailable" };
    case "downloading":
      return { tone: "info", key: "tuning.palette.updateDownloading" };
    default:
      // "busy": a check already runs, so "Güncellemeler denetleniyor" stays as it is;
      // "available" only comes from background checks (a manual one downloads).
      return null;
  }
}

/** The address the title bar copies until the server status names another one. */
export const DEFAULT_SERVER_ADDRESS = "play.projects.gg";
