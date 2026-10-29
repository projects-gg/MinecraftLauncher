// The settings as questions (SETUP-UX-L): every answer sentence in both languages against the
// settings it describes, the setup's step rules, where a limit lives, the ads' write to an older
// service, and where a command row sits (it keeps its question until it is saved).
import i18next, { type TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import { enAfkPlus } from "@/locales/en-afkplus";
import { enSetup } from "@/locales/en-setup";
import { trAfkPlus } from "@/locales/tr-afkplus";
import { trSetup } from "@/locales/tr-setup";
import type { PlusAdPreview, PlusConfig, PlusOrder } from "../plus/types";
import type { Command, Draft } from "../settings-model";
import {
  accountAnswers,
  accountNeedsSetup,
  adsWrite,
  chatAmount,
  commandPlace,
  fitCounts,
  fitOrder,
  limitQuestion,
  moneyStep,
  notifyAnswer,
  type PlusAnswerConfig,
  type PlusAnswerContext,
  plusAnswers,
  rowWords,
  talkSample,
} from "./answers";

function translator(lng: "tr" | "en"): TFunction {
  const instance = i18next.createInstance();
  void instance.init({
    lng,
    fallbackLng: "tr",
    initAsync: false,
    interpolation: { escapeValue: false },
    resources: {
      tr: { translation: { setup: trSetup, afkPlus: trAfkPlus } },
      en: { translation: { setup: enSetup, afkPlus: enAfkPlus } },
    },
  });
  return instance.t;
}

const tr = translator("tr");
const en = translator("en");

const NAMES: Record<string, { tr: string; en: string }> = {
  "minecraft:elytra": { tr: "Elytra", en: "Elytra" },
  "minecraft:totem_of_undying": { tr: "Ölümsüzlük Totemi", en: "Totem of Undying" },
  "minecraft:diamond": { tr: "Elmas", en: "Diamond" },
};

function order(parts: Partial<PlusOrder>): PlusOrder {
  return {
    id: "o1",
    on: true,
    side: "buy",
    item: "minecraft:elytra",
    qty: 4,
    req: null,
    pay: "money",
    unit: "each",
    target: 0,
    limit: 12_000,
    style: "balanced",
    trade: "any",
    until: 2_000_000_000,
    note: "",
    src: "player",
    at: 1_700_000_000,
    ...parts,
  } as PlusOrder;
}

function config(parts: Partial<PlusAnswerConfig> = {}): PlusAnswerConfig {
  return {
    orders: [
      order({}),
      order({ id: "o2", item: "minecraft:totem_of_undying", qty: 6, limit: 9_000 }),
    ],
    limits: {
      perDealMax: 30_000,
      dailySpendMax: null,
      totalBudget: 100_000,
      balanceFloor: 0,
      approvalAbove: null,
      newCounterpartApprovalAbove: null,
      approvalTimeoutMin: 10,
    },
    authority: "ask",
    allow: { buy: true, sell: true },
    discovery: { scanChat: true, proactivePm: true, ads: false, adTemplate: "buy-basic" },
    persona: {
      preset: "polite",
      intro: "",
      status: "",
      statusPreset: null,
      smallTalk: false,
      playerTalk: true,
      prefix: { on: false, text: "[AFK+]" },
    },
    replies: { scope: "trade", away: true, saveAt: 80 },
    who: { mode: "all", friends: [] },
    blocked: [],
    schedule: { on: false, windows: [] },
    ...parts,
  } as PlusAnswerConfig;
}

function preview(parts: Partial<PlusAdPreview> = {}): PlusAdPreview {
  return {
    floorMin: 30,
    everyMin: 60,
    lines: ["Alıyorum: 4 Elytra, PM at"],
    namesPending: false,
    nextAt: null,
    warning: null,
    stoppedToday: false,
    ...parts,
  };
}

function ctx(lang: "tr" | "en", parts: Partial<PlusAnswerContext> = {}): PlusAnswerContext {
  return {
    lang,
    money: "ripple",
    itemName: (ref) => NAMES[ref]?.[lang] ?? ref,
    breakUntil: null,
    schedule: true,
    saver: true,
    preview: null,
    notify: null,
    ...parts,
  };
}

describe("AFK+ answers", () => {
  it("say the orders and the money in the site's words", () => {
    const said = plusAnswers(config(), ctx("tr"), tr);
    expect(said.orders).toBe("2 sipariş: Elytra, Ölümsüzlük Totemi");
    // Every amount carries the money's name where the profile names one.
    expect(said.money).toBe("Tek anlaşmada 30.000 ripple, toplam 100.000 ripple");
    const english = plusAnswers(config(), ctx("en"), en);
    expect(english.orders).toBe("2 orders: Elytra, Totem of Undying");
    expect(english.money).toBe("30,000 ripple per deal, 100,000 ripple in total");
    // One order is "order", without the money's name amounts are bare.
    expect(
      plusAnswers(config({ orders: [order({})] }), ctx("en", { money: null }), en).orders,
    ).toBe("1 order: Elytra");
    expect(plusAnswers(config(), ctx("tr", { money: null }), tr).money).toBe(
      "Tek anlaşmada 30.000, toplam 100.000",
    );
  });

  it("say no buying while a limit is missing, and no orders", () => {
    const limits = { ...config().limits, totalBudget: null };
    const said = plusAnswers(config({ limits, orders: [] }), ctx("tr"), tr);
    expect(said.money).toBe("Alım yapmaz, aradığını bulunca haber verir.");
    expect(said.orders).toBe("Sipariş yok");
  });

  it("say the authority, the approval limit only where it works, and a side that is off", () => {
    const limits = { ...config().limits, approvalAbove: 35_000 };
    expect(plusAnswers(config({ limits }), ctx("tr"), tr).act).toBe(
      "Pazarlık eder, anlaşınca sana sorar.",
    );
    expect(plusAnswers(config({ limits, authority: "deal" }), ctx("tr"), tr).act).toBe(
      "Sınırlarının içinde sormadan tamamlar. 35.000 ripple üstü alımlarda önce sana sorar.",
    );
    expect(plusAnswers(config({ authority: "deal" }), ctx("en"), en).act).toBe(
      "Closes deals within your limits without asking.",
    );
    expect(
      plusAnswers(config({ authority: "reply", allow: { buy: false, sell: true } }), ctx("tr"), tr)
        .act,
    ).toBe("Pazarlık etmez, teklifleri sana iletir. Yalnız satar.");
    expect(plusAnswers(config({ allow: { buy: false, sell: false } }), ctx("en"), en).act).toBe(
      "Haggles, then asks you before a deal. Buying and selling are off.",
    );
    // An authority the launcher does not know reads as the default.
    expect(plusAnswers(config({ authority: "later" }), ctx("tr"), tr).act).toBe(
      "Pazarlık eder, anlaşınca sana sorar.",
    );
  });

  it("say the ads as the service plans them, and nothing more from an older service", () => {
    const on = { ...config().discovery, ads: true };
    const at = (minutes: number, lang: "tr" | "en" = "tr") =>
      plusAnswers(
        config({ discovery: on }),
        ctx(lang, { preview: preview({ everyMin: minutes }) }),
        lang === "tr" ? tr : en,
      ).chat;
    // Scanning and writing to a seller once are on by default, and the answer says so.
    expect(at(60)).toBe(
      "Saatte bir ilan verir, aradığın eşya satılığa çıkınca satıcıya bir kez yazar.",
    );
    expect(at(120)).toBe(
      "2 saatte bir ilan verir, aradığın eşya satılığa çıkınca satıcıya bir kez yazar.",
    );
    expect(at(45)).toBe(
      "45 dakikada bir ilan verir, aradığın eşya satılığa çıkınca satıcıya bir kez yazar.",
    );
    // English says it short enough for the setup's rail in two lines.
    expect(at(60, "en")).toBe("Posts an ad every hour, messages sellers once.");
    expect(at(30, "en")).toBe("Posts an ad every 30 minutes, messages sellers once.");
    expect(plusAnswers(config({ discovery: on }), ctx("tr"), tr).chat).toBe(
      "İlan verir, aradığın eşya satılığa çıkınca satıcıya bir kez yazar.",
    );
    expect(
      plusAnswers(
        config({ discovery: { ...on, scanChat: false } }),
        ctx("tr", { preview: preview({ stoppedToday: true }) }),
        tr,
      ).chat,
    ).toBe("Bugün ilan vermez.");
    // Writing off: it only watches; both off: nothing more.
    const scanOnly = { ...config().discovery, proactivePm: false };
    expect(
      plusAnswers(config({ discovery: scanOnly }), ctx("tr", { preview: preview() }), tr).chat,
    ).toBe("İlan vermez, satış ilanlarını tarar.");
    expect(plusAnswers(config({ discovery: scanOnly }), ctx("en"), en).chat).toBe(
      "Posts no ads, watches chat for sale posts.",
    );
  });

  it("make the tone preview's deal from the first buy, never past one deal's limit", () => {
    const orders = [order({ item: "minecraft:totem_of_undying", limit: 12_000 })];
    expect(talkSample(orders, null)).toEqual({ item: "minecraft:totem_of_undying", offer: 9_600 });
    expect(talkSample(orders, 10_000)).toEqual({
      item: "minecraft:totem_of_undying",
      offer: 8_000,
    });
    expect(talkSample([], null)).toEqual({ item: null, offer: 32_000 });
    expect(talkSample([order({ side: "sell" })], 3_000)).toEqual({ item: null, offer: 2_400 });
    expect(chatAmount(32_000, "tr")).toBe("32k");
    expect(chatAmount(9_600, "tr")).toBe("9,6k");
    expect(chatAmount(9_600, "en")).toBe("9.6k");
    expect(chatAmount(1_250_000, "tr")).toBe("1,2m");
    expect(chatAmount(850, "tr")).toBe("850");
  });

  it("say the tone, whom it answers and when it works", () => {
    const said = plusAnswers(config({ blocked: ["Dolandirici_X", "Hizli_Satis"] }), ctx("tr"), tr);
    expect(said.talk).toBe("Kibar, oyuncu gibi");
    expect(said.who).toBe(
      "Herkesin takas mesajlarına cevap verir, başka mesajlara uzakta olduğunu söyler. 2 oyuncuyu engelledin.",
    );
    expect(said.time).toBe("Her zaman");
    const persona = { ...config().persona, preset: "brief", playerTalk: false };
    const friends = plusAnswers(
      config({
        persona,
        who: { mode: "friends", friends: ["Neon_Drift"] },
        replies: { scope: "all", away: false, saveAt: null },
        blocked: ["Dolandirici_X"],
        schedule: { on: true, windows: [] },
      }),
      ctx("en"),
      en,
    );
    expect(friends.talk).toBe("Short and clear");
    expect(friends.who).toBe(
      "Answers every private message from your friends. You blocked 1 player.",
    );
    expect(friends.time).toBe("Only during the hours you chose");
    expect(
      plusAnswers(
        config({ replies: { scope: "off", away: true, saveAt: null } }),
        ctx("tr", { breakUntil: "30 Ekim'e" }),
        tr,
      ),
    ).toMatchObject({
      who: "Yalnız hazır cevaplar verir, başka mesajlara uzakta olduğunu söyler.",
      time: "30 Ekim'e kadar molada",
    });
    // Studio's hours switch off: the stored hours do nothing, so it works all the time.
    expect(
      plusAnswers(
        config({ schedule: { on: true, windows: [] } }),
        ctx("tr", { schedule: false }),
        tr,
      ).time,
    ).toBe("Her zaman");
    expect(plusAnswers(config(), ctx("en", { breakUntil: "until 30 October" }), en).time).toBe(
      "On a break until 30 October",
    );
  });

  it("say the rest and the notices", () => {
    expect(plusAnswers(config(), ctx("tr"), tr).more).toBe("Kontör tasarrufu, günlük özet");
    expect(plusAnswers(config(), ctx("tr", { saver: false }), tr).more).toBe("Günlük özet");
    const prefs = {
      events: { summary: false, weekly: false, approval: true },
      quiet: { enabled: false, from: "23:00", to: "08:00" },
    };
    expect(notifyAnswer(prefs, tr)).toBe("2 bildirim kapalı");
    expect(notifyAnswer({ ...prefs, events: { summary: false } }, en)).toBe("1 notification off");
    expect(notifyAnswer({ events: {}, quiet: { ...prefs.quiet, enabled: true } }, tr)).toBe(
      "Hepsi açık, sessiz saatler açık",
    );
    expect(notifyAnswer(null, tr)).toBe("Bütün bulut hesapların için.");
  });
});

describe("AFK+ setup steps", () => {
  it("ask the money with a buy, start it off with no order, skip it while all are sales", () => {
    expect(moneyStep([order({})])).toBe("ask");
    expect(moneyStep([])).toBe("notNow");
    expect(moneyStep([order({ side: "sell", item: "minecraft:diamond" })])).toBe("skip");
    // An order that is off counts for nothing.
    expect(moneyStep([order({ on: false })])).toBe("notNow");
  });

  it("show the limits against the first buy priced per piece, never past what it asks for", () => {
    const orders = [
      order({ unit: "total", limit: 50_000 }),
      order({ id: "o2", item: "minecraft:totem_of_undying", qty: 6, limit: 9_000 }),
    ];
    const first = fitOrder(orders);
    expect(first?.item).toBe("minecraft:totem_of_undying");
    expect(fitCounts({ perDealMax: 30_000, totalBudget: 100_000 }, first)).toEqual({
      deal: 3,
      total: 6,
      price: 9_000,
    });
    expect(fitCounts({ perDealMax: null, totalBudget: 100_000 }, first)).toBeNull();
    expect(fitCounts({ perDealMax: 30_000, totalBudget: 100_000 }, orders[0] ?? null)).toBeNull();
  });

  it("go to the question that holds a limit", () => {
    expect(limitQuestion("perDealMax")).toEqual({ question: "money", advanced: false });
    expect(limitQuestion("dailySpendMax")).toEqual({ question: "money", advanced: true });
    expect(limitQuestion("approvalAbove")).toEqual({ question: "act", advanced: false });
    expect(limitQuestion("newCounterpartApprovalAbove").question).toBe("act");
  });
});

describe("Sohbete ilan against an older service", () => {
  const stored = {
    scanChat: true,
    proactivePm: false,
    ads: false,
    adTemplate: "buy-basic",
    adEveryMin: 45,
  } as PlusConfig["discovery"];

  it("never writes the minutes to a service without the plan, nor sends a stored one back", () => {
    const write = adsWrite(stored, true, 45, false);
    expect(write).toEqual({
      scanChat: true,
      proactivePm: false,
      ads: true,
      adTemplate: "buy-basic",
    });
    expect("adEveryMin" in write).toBe(false);
  });

  it("writes the pick, or the service's hour, to one that plans the ads", () => {
    expect(adsWrite(stored, true, 120, true).adEveryMin).toBe(120);
    expect(adsWrite(stored, false, null, true).adEveryMin).toBe(60);
    expect(adsWrite(stored, true, 25, true).adEveryMin).toBe(60);
  });
});

describe("Account answers", () => {
  function command(parts: Partial<Command>): Command {
    return {
      id: "c1",
      enabled: true,
      on: "join",
      delay: 5,
      every: null,
      at: null,
      words: { kind: "plain", text: "/is home" },
      ...parts,
    } as Command;
  }

  function draft(commands: Command[], rules: Draft["config"]["rules"] = []): Draft {
    return {
      name: "EjderAvcisi",
      host: "play.projects.gg",
      port: "",
      version: "26.3",
      lang: "tr",
      config: {
        owner: "",
        antiAfk: { enabled: true, delayMin: 60, delayMax: 90 },
        commands,
        rules,
        inventory: null,
        movement: null,
      },
    };
  }

  const rule = (pattern: string) => ({
    id: `r-${pattern}`,
    enabled: true,
    pattern,
    regex: false,
    from: "any",
    ownerOnly: false,
    cooldown: 30,
    reply: { kind: "plain" as const, text: "Merhaba" },
  });

  const login = command({
    id: "c2",
    delay: 3,
    words: { kind: "sealed", preview: "/login ******" },
  });
  const home = command({});

  it("say the server, the login, what it types and the rules", () => {
    const rows = draft([login, home], [rule("giriş yap"), rule("selam"), rule("sa")]);
    const saved = new Map(rows.config.commands.map((row) => [row.id, row]));
    const place = (row: Command) => commandPlace(row, saved, new Map());
    expect(accountAnswers(rows, place, tr)).toEqual({
      server: "play.projects.gg, sürüm 26.3",
      login: "Girişte /login ****** yazar",
      join: "5 sn sonra /is home yazar",
      replies: '3 kural: "giriş yap", "selam"',
      more: "AFK engelleme, tekrarlanan komutlar, konsol dili, oyuncu adı",
    });
    const english = accountAnswers({ ...rows, version: "auto", port: "25570" }, place, en);
    expect(english.server).toBe("play.projects.gg:25570, version automatic");
    expect(english.join).toBe("Types /is home 5 sec later");
    const empty = accountAnswers(draft([]), place, tr);
    expect(empty).toMatchObject({
      login: "Giriş komutu yok",
      join: "Bir şey yazmaz",
      replies: "Yanıt vermez",
    });
    const many = draft([
      login,
      command({ id: "c3", words: { kind: "secret", text: "/register x x", kept: false } }),
      home,
      command({ id: "c4", words: { kind: "plain", text: "/kit" } }),
    ]);
    const all = new Map(many.config.commands.map((row) => [row.id, row]));
    expect(accountAnswers(many, (row) => commandPlace(row, all, new Map()), tr)).toMatchObject({
      login: "Girişte 2 gizli komut yazar",
      join: "2 komut yazar: /is home ve diğerleri",
    });
  });

  it("keep a row under its question until it is saved", () => {
    const saved = new Map([[home.id, home]]);
    // Typed under "Girince ne yapsın?" and turned hidden (a password): it stays there.
    const typed = command({ words: { kind: "secret", text: "/login hunter2", kept: false } });
    expect(commandPlace(typed, saved, new Map())).toBe("join");
    // Added under "Nasıl giriş yapsın?" with nothing typed yet: the login's.
    const fresh = command({ id: "c9", words: { kind: "plain", text: "" } });
    expect(commandPlace(fresh, saved, new Map([["c9", "login" as const]]))).toBe("login");
    // Saved hidden: the login; on a clock: repeated.
    expect(commandPlace(login, new Map([[login.id, login]]), new Map())).toBe("login");
    expect(commandPlace(command({ id: "c5", on: "interval" }), saved, new Map())).toBe("repeat");
    // A hidden row never says its words.
    expect(rowWords(typed)).toBe("/login ******");
  });

  it("offer the account's setup only while it types nothing on joining and has no rule", () => {
    expect(accountNeedsSetup(draft([login]).config)).toBe(true);
    expect(accountNeedsSetup(draft([login, home]).config)).toBe(false);
    expect(accountNeedsSetup(draft([], [rule("selam")]).config)).toBe(false);
    expect(accountNeedsSetup(draft([command({ on: "interval" })]).config)).toBe(true);
  });
});
