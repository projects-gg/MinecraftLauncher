import { describe, expect, it } from "vitest";
import type { AfkAccount, AfkState } from "@/lib/api";
import { tr } from "@/locales/tr";
import {
  afkPaletteActions,
  PALETTE_WORDS,
  type PaletteWords,
  paletteFilter,
  paletteKeywords,
  rankGroups,
  updateReport,
} from "./palette-actions";

interface Candidate {
  value: string;
  keywords: string[];
}

const item = (value: string, label: string, words?: PaletteWords, extra?: string[]): Candidate => ({
  value,
  keywords: paletteKeywords(label, words, extra),
});

// The palette's groups while searching, in their order (a Microsoft account is active).
const palette: { id: string; entries: Candidate[] }[] = [
  {
    id: "play",
    entries: [
      item("play:projects", "Projects ile oyna", undefined, ["1.21.4", "Fabric"]),
      item("play:survival", "Survival ile oyna", undefined, ["1.20.1", "Vanilla"]),
    ],
  },
  {
    id: "go",
    entries: [
      item("go:/", tr.nav.home, PALETTE_WORDS.home),
      item("go:/library", tr.nav.library, PALETTE_WORDS.library),
      item("go:/news", tr.nav.news, PALETTE_WORDS.news),
      item("go:/afk", tr.nav.afk, PALETTE_WORDS.afk),
      item("go:/settings", tr.nav.settings, PALETTE_WORDS.settings),
      item("open:projects", "Projects", undefined, [tr.palette.instances]),
      item("open:survival", "Survival", undefined, [tr.palette.instances]),
    ],
  },
  {
    id: "instance",
    entries: [
      item("instance:graphics", tr.graphics.palette, PALETTE_WORDS.graphics),
      item("instance:memory", tr.tuning.palette.memory, PALETTE_WORDS.memory),
      item("instance:mods", tr.instance.tabs.mods, PALETTE_WORDS.mods),
      item("instance:resource-packs", tr.instance.tabs.resourcePacks, PALETTE_WORDS.resourcePacks),
      item("instance:shaders", tr.instance.tabs.shaders, PALETTE_WORDS.shaders),
      item("instance:worlds", tr.instance.tabs.worlds, PALETTE_WORDS.worlds),
      item("instance:screenshots", tr.instance.tabs.screenshots, PALETTE_WORDS.screenshots),
      item("instance:logs", tr.instance.tabs.logs, PALETTE_WORDS.logs),
      item("instance:folder", tr.tuning.palette.openFolder, PALETTE_WORDS.folder),
    ],
  },
  {
    id: "accounts",
    entries: [
      item("account:steve", "Hesap değiştir: Steve", undefined, ["Çevrimdışı"]),
      item("action:appearance", tr.tuning.palette.appearance, PALETTE_WORDS.appearance),
      item("action:add-account", tr.palette.addAccount, PALETTE_WORDS.addAccount, ["Microsoft"]),
      item("action:account-center", tr.palette.openAccountCenter, PALETTE_WORDS.accountCenter, [
        tr.nav.accounts,
      ]),
    ],
  },
  {
    id: "settings",
    entries: [
      item("setting:performance", tr.palette.performanceOn, PALETTE_WORDS.performance, [
        tr.palette.performanceOn,
        tr.palette.performanceOff,
      ]),
      item("setting:java", tr.tuning.palette.java, PALETTE_WORDS.java),
      item("setting:cleanup", tr.tuning.palette.cleanup, PALETTE_WORDS.cleanup),
      item("setting:support", tr.tuning.palette.support, PALETTE_WORDS.support),
    ],
  },
  {
    id: "actions",
    entries: [
      item("action:new-instance", tr.palette.newInstance, PALETTE_WORDS.newInstance),
      item("action:copy-address", tr.tuning.palette.copyAddress, PALETTE_WORDS.copyAddress),
      item("action:afk-all", tr.palette.afkConnectAll, PALETTE_WORDS.afkConnectAll),
      item("action:updates", tr.palette.checkUpdates, PALETTE_WORDS.updates),
      item("action:shortcuts", tr.shortcuts.title, PALETTE_WORDS.shortcuts),
    ],
  },
  {
    id: "news",
    entries: [
      item("news:fps", "Yeni sezonda daha fazla FPS", {
        keywords: ["Duyuru"],
        related: ["Sunucu tarafında iyileştirmeler yaptık, kasma ve FPS düşüşü azaldı."],
      }),
      item("news:ramazan", "Ramazan etkinliği başladı", {
        keywords: ["Etkinlik"],
        related: ["Bayram boyunca çifte XP."],
      }),
    ],
  },
];

/**
 * What the palette shows for `query`, top to bottom: the groups in `rankGroups` order, each
 * group's items by score as cmdk sorts them (ties keep their order), unmatched items hidden.
 */
function rank(query: string): string[] {
  return rankGroups(palette, query).flatMap((group) =>
    group.entries
      .map((entry, index) => ({
        entry,
        index,
        score: paletteFilter(entry.value, query, entry.keywords),
      }))
      .filter((row) => row.score > 0)
      .sort((a, b) => b.score - a.score || a.index - b.index)
      .map((row) => row.entry.value),
  );
}

describe("palette search", () => {
  it.each([
    // In-game lag: the graphics preset is the fix; the launcher's Performance mode follows.
    ["kasiyor", "instance:graphics"],
    ["kasıyor", "instance:graphics"],
    ["KASIYOR", "instance:graphics"],
    ["fps", "instance:graphics"],
    ["fps düşük", "instance:graphics"],
    ["lag", "instance:graphics"],
    ["grafik", "instance:graphics"],
    ["görüş mesafesi", "instance:graphics"],
    ["render", "instance:graphics"],
    ["akıcı", "instance:graphics"],
    ["launcher kasıyor", "setting:performance"],
    ["performans modu", "setting:performance"],
    ["ip", "action:copy-address"],
    ["İP", "action:copy-address"],
    ["sunucu ip", "action:copy-address"],
    ["ram", "instance:memory"],
    ["RAM arttırma", "instance:memory"],
    ["bellek", "instance:memory"],
    ["java", "setting:java"],
    ["jdk", "setting:java"],
    ["çöp", "setting:cleanup"],
    ["disk dolu", "setting:cleanup"],
    ["skin", "action:appearance"],
    ["pelerin", "action:appearance"],
    ["ss", "instance:screenshots"],
    ["ekran görüntüsü", "instance:screenshots"],
    ["dünya yedeği", "instance:worlds"],
    ["crash", "instance:logs"],
    ["log", "instance:logs"],
    ["destek", "setting:support"],
    ["klasör", "instance:folder"],
    ["shader", "instance:shaders"],
    ["optifine", "instance:mods"],
    ["hesap ekle", "action:add-account"],
    ["güncelle", "action:updates"],
    ["kısayol", "action:shortcuts"],
    ["kisayollar", "action:shortcuts"],
    ["klavye kısayolları", "action:shortcuts"],
    ["tuşlar", "action:shortcuts"],
    ["ctrl", "action:shortcuts"],
    ["shortcuts", "action:shortcuts"],
  ])("%s finds %s first", (query, expected) => {
    expect(rank(query)[0]).toBe(expected);
  });

  it("keeps the other fixes for a problem below the main one", () => {
    const results = rank("kasıyor");
    expect(results.indexOf("instance:memory")).toBeGreaterThan(0);
    expect(results.indexOf("setting:performance")).toBeGreaterThan(0);
    expect(rank("çöküyor").slice(0, 2)).toEqual(["instance:logs", "instance:memory"]);
    // Screenshots stay screenshots, though the graphics item knows "görüntü".
    expect(rank("ekran görüntüleri")[0]).toBe("instance:screenshots");
  });

  it("reads past the filler and the Turkish endings players type", () => {
    expect(rank("oyun neden kasıyor")[0]).toBe("instance:graphics");
    expect(rank("ram nasıl arttırılır")[0]).toBe("instance:memory");
    expect(rank("skinimi değiştir")[0]).toBe("action:appearance");
    expect(rank("loglar")[0]).toBe("instance:logs");
    expect(rank("shader yükle")[0]).toBe("instance:shaders");
    expect(rank("bilgisayar donuyor")[0]).toBe("instance:graphics");
    expect(rank("grafik ayarları")[0]).toBe("instance:graphics");
  });

  it("folds Turkish letters both ways", () => {
    expect(rank("ışık")[0]).toBe("instance:shaders");
    expect(rank("isik")[0]).toBe("instance:shaders");
    expect(rank("DÜNYA")[0]).toBe("instance:worlds");
    expect(rank("gorunumu degistir")[0]).toBe("action:appearance");
  });

  it("puts settings above a news story that names the same thing", () => {
    expect(rank("fps").indexOf("news:fps")).toBeGreaterThan(0);
    expect(rank("ram").indexOf("news:ramazan")).toBeGreaterThan(0);
  });

  it("never matches internal ids or nonsense", () => {
    expect(rank("action")).toEqual([]);
    expect(rank("open")).toEqual([]);
    expect(rank("zzqx")).toEqual([]);
  });

  it("shows everything for an empty search", () => {
    expect(paletteFilter("x", "  ", ["Ana Sayfa"])).toBe(1);
  });

  it("finds a stop word typed alone", () => {
    expect(rank("oyun")[0]).toBe("instance:folder");
  });
});

describe("rankGroups", () => {
  it("leads with the group holding the best match, not the first one that matches", () => {
    expect(
      rankGroups(palette, "launcher kasıyor")
        .map((group) => group.id)
        .slice(0, 2),
    ).toEqual(["settings", "instance"]);
    expect(rankGroups(palette, "ip")[0]?.id).toBe("actions");
  });

  it("keeps the order while nothing is typed", () => {
    expect(rankGroups(palette, " ").map((group) => group.id)).toEqual(
      palette.map((group) => group.id),
    );
  });
});

const afkAccount = (id: string, state: AfkState): AfkAccount => ({
  id,
  nickname: id,
  server: "play.projects.gg",
  autoStart: false,
  mcVersion: "auto",
  mcVersionRecommended: true,
  state,
});

describe("afkPaletteActions", () => {
  it("offers nothing without AFK accounts", () => {
    expect(afkPaletteActions([])).toEqual({ connectAll: false, stopAll: false });
  });

  it("offers connecting while one is idle and stopping while one runs", () => {
    expect(afkPaletteActions([afkAccount("a", { kind: "stopped" })])).toEqual({
      connectAll: true,
      stopAll: false,
    });
    expect(
      afkPaletteActions([
        afkAccount("a", { kind: "connected", since: "2026-10-17T10:00:00Z" }),
        afkAccount("b", { kind: "error", reason: "banned" }),
      ]),
    ).toEqual({ connectAll: true, stopAll: true });
    expect(
      afkPaletteActions([
        afkAccount("a", { kind: "queued", inSecs: 5 }),
        afkAccount("b", {
          kind: "reconnecting",
          attempt: 2,
          nextAttemptAt: "2026-10-17T10:00:10Z",
        }),
      ]),
    ).toEqual({ connectAll: false, stopAll: true });
  });

  it("waits while the last one is stopping", () => {
    expect(afkPaletteActions([afkAccount("a", { kind: "stopping" })])).toEqual({
      connectAll: false,
      stopAll: false,
    });
  });
});

/** Reads a dotted key from the Turkish strings. */
function lookup(key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
      tr,
    );
}

describe("updateReport", () => {
  it("answers like Settings does", () => {
    expect(updateReport("upToDate")).toEqual({ tone: "success", key: "settings.updates.upToDate" });
    expect(updateReport("ready")).toEqual({ tone: "success", key: "settings.updates.ready" });
    expect(updateReport("unavailable")).toEqual({
      tone: "error",
      key: "settings.updates.unavailable",
    });
    expect(updateReport("downloading")?.tone).toBe("info");
  });

  it("stays quiet while a check already runs", () => {
    expect(updateReport("busy")).toBeNull();
    expect(updateReport("available")).toBeNull();
  });

  it("names strings that exist", () => {
    for (const result of ["upToDate", "ready", "unavailable", "downloading"] as const) {
      const report = updateReport(result);
      expect(typeof lookup(report?.key ?? "")).toBe("string");
    }
  });
});
