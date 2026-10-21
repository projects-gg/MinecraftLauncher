import i18next, { type TFunction } from "i18next";
import { beforeAll, describe, expect, it } from "vitest";
import type { HelpSystem } from "@/lib/api";
import { en } from "@/locales/en";
import { enHelp } from "@/locales/en-help";
import { tr } from "@/locales/tr";
import { trHelp } from "@/locales/tr-help";
import {
  clockAmount,
  fixLabel,
  layout,
  noticeRow,
  type Row,
  reasonOf,
  severity,
  shownFix,
  summaryText,
  supportCode,
  supportPlace,
  verdict,
  verdictLine,
} from "./model";

// The real strings with the troubleshooter's module, as the app will have them once wired.
async function translator(lng: "tr" | "en"): Promise<TFunction> {
  const instance = i18next.createInstance();
  await instance.init({
    lng,
    fallbackLng: "tr",
    resources: {
      tr: { translation: { ...tr, help: trHelp } },
      en: { translation: { ...en, help: enHelp } },
    },
    interpolation: { escapeValue: false },
  });
  return instance.t;
}

let t: TFunction;
let tEn: TFunction;
beforeAll(async () => {
  t = await translator("tr");
  tEn = await translator("en");
});

const row = (
  check: Row["check"],
  level: Row["level"],
  code: Row["code"],
  extra: Partial<Row> = {},
): Row => ({
  check,
  level,
  code,
  facts: {},
  ...extra,
});

describe("supportCode", () => {
  it("writes codes the way support reads them", () => {
    expect(supportCode("serverUnreachable")).toBe("SERVER-UNREACHABLE");
    expect(supportCode("internetNoDns")).toBe("INTERNET-NO-DNS");
    expect(supportCode("javaX86")).toBe("JAVA-X86");
    expect(supportCode("noticeMaintenance")).toBe("NOTICE-MAINTENANCE");
    expect(supportCode("diskOk")).toBe("DISK-OK");
  });
});

describe("noticeRow", () => {
  const now = Date.parse("2026-10-21T18:00:00Z");
  const status = (level: "outage" | "degraded" | "notice", url?: string) => ({
    level,
    title: "Planlı Bakım",
    message: "24 Ekim Cumartesi 00:01–03:00.",
    url,
    expiresAt: "2026-10-21T18:10:00Z",
  });

  it("tells Studio's maintenance as information", () => {
    const notice = noticeRow({ title: "Sunucu bakımda", message: "" }, status("outage"), now);
    expect(notice).toMatchObject({
      level: "note",
      code: "noticeMaintenance",
      facts: { title: "Sunucu bakımda" },
    });
    expect(notice?.fix).toBeUndefined();
  });

  it("follows the status page's level and links it", () => {
    expect(noticeRow(undefined, status("outage", "https://status.projects.gg"), now)).toMatchObject(
      {
        level: "problem",
        code: "noticeOutage",
        fix: "statusPage",
        facts: { url: "https://status.projects.gg" },
      },
    );
    expect(noticeRow(undefined, status("degraded"), now)?.level).toBe("warning");
    const info = noticeRow(undefined, status("notice"), now);
    expect(info?.level).toBe("note");
    expect(info?.fix).toBeUndefined();
  });

  it("leaves out an expired notice and no notice", () => {
    expect(noticeRow(undefined, status("outage"), Date.parse("2026-10-21T18:10:00Z"))).toBeNull();
    expect(noticeRow(undefined, undefined, now)).toBeNull();
  });
});

describe("layout", () => {
  it("keeps the fixed order and adds conditional rows only with a finding", () => {
    const groups = layout(
      ["crash", "disk", "internet", "server", "java"],
      new Set(["memory", "notice"]),
    );
    expect(groups).toEqual([
      { group: "connection", checks: ["internet", "server", "notice"] },
      { group: "game", checks: ["disk", "java", "memory", "crash"] },
    ]);
  });

  it("drops a group with nothing in it", () => {
    expect(layout(["internet"], new Set())).toEqual([
      { group: "connection", checks: ["internet"] },
    ]);
  });
});

describe("verdict", () => {
  const rows = [
    row("internet", "ok", "internetOk"),
    row("server", "warning", "serverSlow"),
    row("disk", "problem", "diskFull"),
    row("mods", "problem", "modsBlocking"),
    row("notice", "note", "noticeInfo"),
    row("content", "skipped", "contentNeedsInternet"),
  ];

  it("waits while checking", () => {
    expect(verdict(rows, true)).toEqual({ kind: "checking" });
  });

  it("counts problems first, then warnings", () => {
    expect(verdict(rows, false)).toEqual({ kind: "problems", count: 2, checks: ["disk", "mods"] });
    expect(
      verdict(
        rows.filter((r) => r.level !== "problem"),
        false,
      ),
    ).toEqual({
      kind: "warnings",
      count: 1,
      checks: ["server"],
    });
  });

  it("is fine with notes and skipped rows", () => {
    expect(
      verdict([row("internet", "ok", "internetOk"), row("notice", "note", "noticeInfo")], false),
    ).toEqual({
      kind: "fine",
    });
  });

  it("ranks the levels", () => {
    expect(severity("problem")).toBeGreaterThan(severity("warning"));
    expect(severity("warning")).toBeGreaterThan(severity("note"));
    expect(severity("ok")).toBeGreaterThan(severity("skipped"));
  });
});

describe("reasonOf", () => {
  const now = Date.parse("2026-10-21T18:00:00Z");
  const say = (r: Row) => reasonOf(t, r, "tr", now);

  it("words the connection", () => {
    expect(say(row("internet", "ok", "internetOk"))).toBe("Bağlısın.");
    expect(say(row("internet", "problem", "internetInsecure"))).toBe(
      "Güvenli bağlantı kurulamıyor.",
    );
    expect(say(row("internet", "problem", "internetInsecure", { fix: "clockSettings" }))).toBe(
      "Saat yanlış olduğu için güvenli bağlantı kurulamıyor.",
    );
    expect(say(row("server", "ok", "serverOk", { facts: { latencyMs: 42, players: 1204 } }))).toBe(
      "42 ms, 1.204 oyuncu çevrimiçi.",
    );
    expect(say(row("server", "ok", "serverOk", { facts: { latencyMs: 42 } }))).toBe("Açık, 42 ms.");
    expect(say(row("server", "warning", "serverSlow", { facts: { latencyMs: 340 } }))).toBe(
      "Ping yüksek: 340 ms.",
    );
    for (const code of [
      "serverNeedsInternet",
      "contentNeedsInternet",
      "friendsNeedsInternet",
      "microsoftNeedsInternet",
      "discordNeedsInternet",
    ] as const) {
      expect(say(row("server", "skipped", code))).toBe("İnternet olmadan denenemedi.");
    }
  });

  it("says how far the clock is off", () => {
    expect(say(row("clock", "problem", "clockWrong", { facts: { minutes: 12 } }))).toBe(
      "Bilgisayarının saati 12 dakika ileride.",
    );
    expect(say(row("clock", "problem", "clockWrong", { facts: { minutes: -180 } }))).toBe(
      "Bilgisayarının saati 3 saat geride.",
    );
    expect(say(row("clock", "problem", "clockWrong", { facts: { minutes: -3 * 24 * 60 } }))).toBe(
      "Bilgisayarının saati 3 gün geride.",
    );
    expect(clockAmount(tEn, 1)).toBe("1 minute");
    expect(clockAmount(tEn, 5)).toBe("5 minutes");
    expect(clockAmount(tEn, 60 * 24 * 2)).toBe("2 days");
  });

  it("words this computer and the game", () => {
    expect(
      say(row("disk", "problem", "diskFull", { facts: { freeBytes: 820 * 1024 * 1024 } })),
    ).toBe("Neredeyse yer kalmadı: 820 MB boş.");
    expect(say(row("java", "ok", "javaReady", { facts: { major: 21 } }))).toBe("Java 21 hazır.");
    expect(say(row("java", "problem", "javaTooOld", { facts: { major: 8, need: 21 } }))).toBe(
      "Java 8 eski kalıyor, Java 21 gerekiyor.",
    );
    expect(say(row("mods", "ok", "modsOk", { facts: { count: 0 } }))).toBe("Kurulu mod yok.");
    expect(say(row("mods", "ok", "modsOk", { facts: { count: 48 } }))).toBe("48 mod, sorun yok.");
    expect(say(row("mods", "problem", "modsBlocking", { facts: { count: 2 } }))).toBe(
      "2 mod sorunu oyunun açılmasını engelliyor.",
    );
    for (const code of ["javaUnknown", "modsUnknown", "checkFailed"] as const) {
      expect(say(row("mods", "skipped", code))).toBe("Denetlenemedi.");
    }
  });

  it("names the last crash and the mod it points at", () => {
    const at = "2026-10-20T16:00:00Z";
    expect(
      say(row("crash", "problem", "crashRecent", { facts: { at, crash: "outOfMemory" } })),
    ).toBe("Dün: Oyunun belleği yetmedi.");
    expect(
      say(
        row("crash", "problem", "crashRecent", {
          facts: { at, crash: "modError", name: "Sodium" },
        }),
      ),
    ).toBe("Dün: Sodium oyunu çökertmiş olabilir.");
  });

  it("shows an announcement in the team's own words", () => {
    expect(say(row("notice", "note", "noticeInfo", { facts: { title: "Planlı Bakım" } }))).toBe(
      "Planlı Bakım",
    );
  });

  it("speaks English with its plurals", () => {
    const sayEn = (r: Row) => reasonOf(tEn, r, "en", now);
    expect(sayEn(row("mods", "ok", "modsOk", { facts: { count: 1 } }))).toBe("1 mod, no problems.");
    expect(sayEn(row("mods", "problem", "modsBlocking", { facts: { count: 2 } }))).toBe(
      "2 mod problems stop the game.",
    );
    expect(sayEn(row("internet", "problem", "internetOffline"))).toBe("No internet connection.");
  });
});

describe("fixLabel", () => {
  it("keeps the names the fixes have elsewhere", () => {
    expect(fixLabel(t, "makeSpace")).toBe("Yer aç");
    expect(fixLabel(t, "installJava")).toBe("Kur ve oyna");
    expect(fixLabel(t, "instanceSettings")).toBe("Kurulum ayarlarını aç");
    expect(fixLabel(t, "repair")).toBe("Onar");
    expect(fixLabel(t, "showMods")).toBe("Göster");
    expect(fixLabel(t, "clockSettings")).toBe("Saat ayarları");
  });
});

describe("verdictLine", () => {
  it("lists the rows the headline counts", () => {
    expect(verdictLine(t, ["disk", "mods"], "tr")).toBe("Disk alanı ve Modlar");
    expect(verdictLine(t, ["disk", "mods", "java"], "tr")).toBe("Disk alanı, Modlar ve Java");
    expect(verdictLine(tEn, ["disk", "mods"], "en")).toBe("Disk space and Mods");
  });

  it("counts in the headline", () => {
    expect(t("help.verdict.problems", { count: 2 })).toBe("2 sorun bulundu");
    expect(tEn("help.verdict.problems", { count: 1 })).toBe("Found 1 problem");
    expect(tEn("help.verdict.problems", { count: 3 })).toBe("Found 3 problems");
  });
});

describe("summaryText", () => {
  const system: HelpSystem = {
    launcher: "2.0.1-beta.1.6",
    debug: false,
    os: "Windows 11 Pro 24H2",
    arch: "x86_64",
    memoryMb: 16_271,
  };
  const rows = [
    row("internet", "ok", "internetOk", { facts: { latencyMs: 38 } }),
    row("server", "warning", "serverSlow", { facts: { latencyMs: 340, players: 1188 } }),
    row("disk", "problem", "diskFull", {
      fix: "makeSpace",
      facts: { freeBytes: 820 * 1024 * 1024 },
    }),
    row("crash", "problem", "crashRecent", {
      facts: { at: "2026-10-20T16:00:00Z", crash: "modError", name: "Sodium" },
    }),
  ];
  const text = () =>
    summaryText({
      t,
      language: "tr",
      system,
      instance: {
        kind: "official",
        minecraft: "26.1.2",
        loader: "fabric",
        loaderVersion: "0.17.2",
        packVersion: "1.4.2",
      },
      rows,
      launchCode: "AG-YOK",
      now: new Date("2026-10-21T18:00:00Z"),
    });

  it("says what support needs, worst first, with codes", () => {
    const lines = text().split("\n");
    expect(lines[0]).toBe("Projects Launcher sorun özeti");
    expect(lines[1]).toBe("Launcher: 2.0.1-beta.1.6");
    expect(lines[2]).toBe("Sistem: Windows 11 Pro 24H2, x86_64, Bellek 15,9 GB");
    expect(lines[3]).toBe("Kurulum: Projects resmî paketi 1.4.2, Minecraft 26.1.2, Fabric 0.17.2");
    expect(lines[4]).toMatch(/^Denetim: /);
    expect(lines[5]).toBe("Son açılış hatası: AG-YOK");
    expect(lines.slice(7)).toEqual([
      "Sorun: Disk alanı. Neredeyse yer kalmadı: 820 MB boş. [DISK-FULL]",
      "Sorun: Son çökme. Dün: Sodium oyunu çökertmiş olabilir. [CRASH-RECENT]",
      "Uyarı: Projects sunucusu. Ping yüksek: 340 ms. [SERVER-SLOW]",
      "Tamam: İnternet. Bağlısın. [INTERNET-OK]",
    ]);
  });

  it("carries nothing personal", () => {
    // The rows and facts have no names, ids, paths, addresses or tokens to begin with.
    expect(text()).not.toMatch(/[A-Za-z]:\\|\/Users\/|\\Users\\|@|https?:\/\//);
  });

  it("leaves out the loader of a vanilla instance", () => {
    const vanilla = summaryText({
      t,
      language: "tr",
      instance: { kind: "custom", minecraft: "1.21.4", loader: "vanilla" },
      rows: [],
      now: new Date("2026-10-21T18:00:00Z"),
    });
    expect(vanilla.split("\n")[1]).toBe("Kurulum: Kendi kurulumu, Minecraft 1.21.4");
  });
});

describe("shownFix", () => {
  const rows = (...list: Row[]) => new Map(list.map((r) => [r.check, r]));
  const insecure = row("internet", "problem", "internetInsecure", { fix: "clockSettings" });
  const clock = row("clock", "problem", "clockWrong", {
    fix: "clockSettings",
    facts: { minutes: -60 },
  });

  it("offers one button per fix", () => {
    // The clock's own row has the settings; the internet row only says why.
    expect(shownFix(insecure, rows(insecure, clock))).toBeUndefined();
    expect(shownFix(clock, rows(insecure, clock))).toBe("clockSettings");
    // Without the clock row the internet row keeps it.
    expect(shownFix(insecure, rows(insecure))).toBe("clockSettings");
  });

  it("offers nothing on a row that is fine", () => {
    const fine = row("disk", "ok", "diskOk", { fix: "makeSpace" });
    expect(shownFix(fine, rows(fine))).toBeUndefined();
    const low = row("disk", "warning", "diskLow", { fix: "makeSpace" });
    expect(shownFix(low, rows(low))).toBe("makeSpace");
  });

  it("keeps the clock's words on the internet row", () => {
    expect(reasonOf(t, insecure, "tr")).toBe(
      "Saat yanlış olduğu için güvenli bağlantı kurulamıyor.",
    );
  });
});

describe("the clock's fix where the system has no settings page", () => {
  it("leaves the button out on Linux", () => {
    const clock = row("clock", "problem", "clockWrong", { fix: "clockSettings" });
    const rows = new Map([[clock.check, clock]]);
    expect(shownFix(clock, rows, false)).toBeUndefined();
    expect(shownFix(clock, rows, true)).toBe("clockSettings");
    const disk = row("disk", "problem", "diskFull", { fix: "makeSpace" });
    expect(shownFix(disk, new Map([[disk.check, disk]]), false)).toBe("makeSpace");
  });
});

describe("supportPlace", () => {
  const forum = { platform: "forum", url: "https://forum.projects.gg" };

  it("sends to Studio's support link, else Discord", () => {
    expect(supportPlace({ support: "https://projects.gg/destek" }, true)).toEqual({
      kind: "page",
      url: "https://projects.gg/destek",
    });
    expect(supportPlace({ discord: "https://discord.gg/abc" }, true)).toEqual({
      kind: "discord",
      url: "https://discord.gg/abc",
    });
    expect(supportPlace({}, true)).toEqual({ kind: "discord", url: "https://projects.gg/discord" });
  });

  it("sends to the forum when Discord is out of reach", () => {
    expect(supportPlace({ discord: "https://discord.gg/abc", community: [forum] }, false)).toEqual({
      kind: "forum",
      url: "https://forum.projects.gg",
    });
    // Without a forum link Discord stays; a support page stays anyway.
    expect(supportPlace({ discord: "https://discord.gg/abc" }, false).kind).toBe("discord");
    expect(
      supportPlace({ support: "https://projects.gg/destek", community: [forum] }, false).kind,
    ).toBe("page");
  });
});
