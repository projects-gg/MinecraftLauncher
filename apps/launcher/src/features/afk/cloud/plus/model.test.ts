import { describe, expect, it } from "vitest";
import { enAfkPlus } from "@/locales/en-afkplus";
import { trAfkPlus } from "@/locales/tr-afkplus";
import {
  aiState,
  applyDraft,
  blockedNameProblem,
  boughtInUse,
  boughtLeft,
  boughtReplies,
  CURRENCY_MAX,
  canSpend,
  configAfterControl,
  configChanges,
  currencyEditProblem,
  currencyFieldState,
  currencyProblem,
  editDraft,
  emptyLimit,
  giftReplies,
  type LimitField,
  limitProblems,
  limitReason,
  liveProblems,
  PREFIX_DEFAULT,
  personaTextProblem,
  prefixProblem,
  presetOf,
  readTime,
  type SettingsDraft,
  type SettingsPart,
  sameSettings,
  settingsPart,
  spendingField,
  startState,
  tradeNoteKey,
  windowProblems,
  withCurrency,
  withOrderMenu,
  withSettings,
} from "./model";
import type { PlusAi, PlusConfig, PlusOrder, PlusQuota } from "./types";

function config(parts: Partial<PlusConfig> = {}): PlusConfig {
  return {
    v: 2,
    enabled: true,
    ready: null,
    mode: "projects-smp",
    lang: "auto",
    persona: {
      preset: "friendly",
      intro: "",
      status: "",
      statusPreset: null,
      smallTalk: false,
      playerTalk: false,
      prefix: { on: false, text: "[AFK+]" },
    },
    replies: { scope: "trade", away: true, saveAt: 80 },
    authority: "ask",
    allow: { buy: true, sell: true },
    pay: "money",
    who: { mode: "all", friends: [] },
    orders: [],
    customItems: [],
    limits: {
      perDealMax: null,
      dailySpendMax: null,
      totalBudget: null,
      balanceFloor: 0,
      approvalAbove: 50_000,
      newCounterpartApprovalAbove: null,
      approvalTimeoutMin: 10,
    },
    discovery: {
      scanChat: true,
      proactivePm: true,
      ads: false,
      adTemplate: "buy-basic",
      adEveryMin: null,
      orderMenu: null,
    },
    trade: { returnRow: null, currency: null },
    summaryAt: "21:00",
    blocked: [],
    schedule: { on: false, windows: [] },
    pauseUntil: null,
    ...parts,
  };
}

function order(parts: Partial<PlusOrder> = {}): PlusOrder {
  return {
    id: "o1",
    on: true,
    side: "buy",
    item: "minecraft:elytra",
    qty: 1,
    req: { ench: {}, dur: 90, named: false, cursed: false },
    pay: "money",
    unit: "total",
    target: 32_000,
    limit: 40_000,
    style: "balanced",
    trade: "auto",
    until: 1_800_000_000,
    note: "",
    src: "text",
    at: 1_790_000_000,
    ...parts,
  };
}

function ai(parts: Partial<PlusAi>): PlusAi {
  return {
    state: "on",
    reason: null,
    staffReason: null,
    until: null,
    trade: { on: true, off: null, by: "bot", buy: "on" },
    pendingApprovals: 0,
    quota: null,
    asOf: null,
    spent: null,
    newMessages: null,
    ...parts,
  };
}

describe("the tag before the assistant's lines (afkplus-config-v2.json's tag cases)", () => {
  it("says what the service says, or leaves it to the service", () => {
    const cases: Array<[string, string, string[], string | null]> = [
      ["a tag is cleaned", "  [Ejder   Bot]  ", [], null],
      ["a tag that is off is still checked", "[Yetkili]", [], "staff"],
      ["a tag posing as staff in digits", "[4dm1n]", [], "staff"],
      ["a tag posing as staff in camel case", "[ModBot]", [], "staff"],
      ["a tag posing as the system with spaces", "[S i s t e m]", [], "staff"],
      ["a tag posing as the server", "(Sunucu)", [], "staff"],
      ["a tag with a colour code", "&a[Bot]", [], "form"],
      ["a tag with a look-alike letter", "[Аdmin]", [], "form"],
      ["a tag in angle brackets", "<Bot>", [], "form"],
      ["a tag over 16 characters", "[Ejderin Botu Burada]", [], "length"],
      ["an empty tag", "   ", [], "empty"],
      // Links and apps are the service's.
      ["a tag naming another app", "[Discord]", [], null],
      ["a tag naming a friend", "[kaan_07]", ["Kaan_07"], "name"],
      ["a tag naming a blocked player", "Spammer1:", ["Spammer1"], "name"],
    ];
    for (const [name, text, names, want] of cases) {
      expect(prefixProblem(text, names), name).toBe(want);
    }
    expect(prefixProblem(PREFIX_DEFAULT.text)).toBeNull();
    expect(PREFIX_DEFAULT).toEqual({ on: false, text: "[AFK+]" });
  });
});

describe("AFK+ limits and fields", () => {
  it("say what an empty limit does: no buying without the per-deal or the total one", () => {
    expect(emptyLimit("perDealMax")).toBe("buysNothing");
    expect(emptyLimit("totalBudget")).toBe("buysNothing");
    expect(emptyLimit("dailySpendMax")).toBe("noLimit");
    expect(emptyLimit("approvalAbove")).toBe("noAsk");
    expect(emptyLimit("newCounterpartApprovalAbove")).toBe("noAsk");
    expect(emptyLimit("balanceFloor")).toBeNull();
    // The site's words under the two (SITE-W2), never "Sınır yok" there.
    expect(trAfkPlus.settings.emptyBuysNothing).toBe("Boş bırakırsan ProBot AI alım yapmaz.");
    expect(enAfkPlus.settings.emptyBuysNothing).toBe("Leave it empty and ProBot AI buys nothing.");
  });

  it("need both limits to buy", () => {
    expect(canSpend(config().limits)).toBe(false);
    expect(canSpend({ ...config().limits, perDealMax: 50_000, totalBudget: 300_000 })).toBe(true);
  });

  it("are checked as the Worker checks them", () => {
    expect(
      limitProblems({ ...config().limits, perDealMax: 10_000, approvalAbove: 20_000 }),
    ).toEqual({ approvalAbove: "approvalAbove" });
    expect(limitProblems({ ...config().limits, approvalTimeoutMin: 0 })).toEqual({
      approvalTimeoutMin: "timeout",
    });
    // The generic mode's cap is 100 million; a profile's own comes with the server.
    expect(limitProblems({ ...config().limits, totalBudget: 200_000_000 })).toEqual({
      totalBudget: "maxDeal",
    });
    expect(limitProblems({ ...config().limits, totalBudget: 20_000_000 }, 10_000_000)).toEqual({
      totalBudget: "maxDeal",
    });
  });

  it("read times typed quickly", () => {
    expect(readTime("0830")).toBe("08:30");
    expect(readTime("8:30")).toBe("08:30");
    expect(readTime("8.30")).toBe("08:30");
    expect(readTime("21")).toBe("21:00");
    expect(readTime("2460")).toBeNull();
    expect(readTime("akşam")).toBeNull();
    expect(windowProblems({ days: [1, 1], from: "08:00", to: "9:00" })).toEqual(["days", "time"]);
    expect(windowProblems({ days: [5, 6], from: "18:00", to: "02:00" })).toEqual([]);
  });

  it("refuse odd names and texts", () => {
    expect(blockedNameProblem("Kaan_07", ["kaan_07"])).toBe("duplicate");
    expect(blockedNameProblem("Ka", [])).toBe("format");
    expect(blockedNameProblem("Kaan_07", [])).toBeNull();
    // A friend who is also blocked: the Worker's `blocked`.
    expect(blockedNameProblem("Kaan_07", [], ["KAAN_07"])).toBe("blocked");
    expect(personaTextProblem("[AFK+] selam", 60)).toBe("marker");
    expect(personaTextProblem("［ＡＦＫ＋］ selam", 60)).toBe("marker");
    expect(personaTextProblem("discord.gg/abc", 60)).toBe("link");
    // AFK+ says no address at all, ours too (the service's text vectors).
    expect(personaTextProblem("play.projects.gg'deyim", 60)).toBe("link");
    for (const bad of ["/spawn", "&aYeşil", "Merhaba §lkalın", "Selam %username%"]) {
      expect(personaTextProblem(bad, 60)).toBe("form");
    }
    expect(personaTextProblem("%50 indirim", 60)).toBeNull();
    expect(personaTextProblem("Tom & Jerry", 60)).toBeNull();
    expect(personaTextProblem("0555 555 55 55", 80)).toBe("phone");
    expect(personaTextProblem("x".repeat(61), 60)).toBe("length");
  });

  it("say which marks hold the change bar's save while they are typed (AFK-CLEAR)", () => {
    const shown = settingsPart(config());
    expect(liveProblems(shown, 10_000_000)).toBe(false);
    const long = { ...shown, persona: { ...shown.persona, intro: "x".repeat(500) } };
    expect(liveProblems(long, 10_000_000)).toBe(true);
    // A status text only counts while it is the player's own (no preset chosen).
    const status = { ...shown, persona: { ...shown.persona, status: "x".repeat(500) } };
    expect(liveProblems(status, 10_000_000)).toBe(true);
    expect(
      liveProblems({ ...status, persona: { ...status.persona, statusPreset: "busy" } }, 10_000_000),
    ).toBe(false);
    const above = {
      ...shown,
      limits: { ...shown.limits, perDealMax: 10_000, approvalAbove: 20_000 },
    };
    expect(liveProblems(above, 10_000_000)).toBe(true);
    // Shown only after "Kaydet": not a reason to wait yet.
    const link = { ...shown, persona: { ...shown.persona, intro: "discord.gg/abc" } };
    expect(liveProblems(link, 10_000_000)).toBe(false);
  });
});

describe("the Ayarlar draft", () => {
  it("keeps the fine-tune settings and puts only the player's edits on the latest", () => {
    const saved = settingsPart(config());
    const draft = editDraft(saved, null, (shown) => ({
      ...shown,
      authority: "deal",
      allow: { ...shown.allow, sell: false },
    }));
    expect(draft?.value.authority).toBe("deal");
    // Meanwhile the site switched buying off and added a friend.
    const latest = settingsPart(
      config({ allow: { buy: false, sell: true }, who: { mode: "friends", friends: ["Ece_"] } }),
    );
    const merged = applyDraft(latest, draft);
    expect(merged.authority).toBe("deal");
    expect(merged.allow).toEqual({ buy: false, sell: false });
    expect(merged.who).toEqual({ mode: "friends", friends: ["Ece_"] });
    // An edit undone is no draft.
    expect(editDraft(saved, draft, () => saved)).toBeNull();
  });

  it("takes only the draft's own friends added and removed", () => {
    const saved = settingsPart(config({ who: { mode: "friends", friends: ["Ece_", "Mert"] } }));
    const draft = editDraft(saved, null, (shown) => ({
      ...shown,
      who: { ...shown.who, friends: ["Ece_", "Kaan_07"] },
    }));
    // Meanwhile "Arkadaşlarıma ekle" added one (and the same name in another case).
    const latest = settingsPart(
      config({ who: { mode: "friends", friends: ["Ece_", "Mert", "Baris_TR", "kaan_07"] } }),
    );
    expect(applyDraft(latest, draft).who.friends).toEqual(["Ece_", "Baris_TR", "kaan_07"]);
  });

  it("sends the ads' minutes as they came, or as the player picked them", () => {
    // A document without them, as the service sends it (none picked, or a service without them).
    const absent: PlusConfig["discovery"] = JSON.parse(
      '{"scanChat": true, "proactivePm": true, "ads": true, "adTemplate": "buy-basic"}',
    );
    const saved = settingsPart(config({ discovery: absent }));
    const sent = (latest: SettingsPart, draft: SettingsDraft | null) =>
      JSON.parse(JSON.stringify(withSettings(config(), applyDraft(latest, draft)))).discovery;
    // Another edit sends them as they came: none is made here.
    const other = editDraft(saved, null, (shown) => ({ ...shown, summaryAt: "22:00" }));
    expect(sent(saved, other)).toEqual(absent);
    // The player's pick goes on top of the latest.
    const pick = editDraft(saved, null, (shown) => ({
      ...shown,
      discovery: { ...shown.discovery, adEveryMin: 45 },
    }));
    expect(sent(saved, pick).adEveryMin).toBe(45);
    // Minutes read stay through another edit.
    const read = settingsPart(config({ discovery: { ...absent, adEveryMin: 120 } }));
    const lang = editDraft(read, null, (shown) => ({ ...shown, lang: "en" }));
    expect(sent(read, lang).adEveryMin).toBe(120);
  });

  it("writes the order menu only as the player switched it (DONUT-ORDER)", () => {
    // Off is not in the document: the service stores it only while on.
    const absent: PlusConfig["discovery"] = JSON.parse(
      '{"scanChat": true, "proactivePm": true, "ads": false, "adTemplate": "buy-basic"}',
    );
    const on = withOrderMenu(absent, true, absent);
    expect(on.orderMenu).toBe(true);
    // Switched on and off again over a document without it: nothing to save.
    const back = withOrderMenu(on, false, absent);
    expect("orderMenu" in back).toBe(false);
    expect(
      sameSettings(
        settingsPart(config({ discovery: absent })),
        settingsPart(config({ discovery: back })),
      ),
    ).toBe(true);
    // A preview's null comes back as null.
    const none = { ...absent, orderMenu: null };
    expect(withOrderMenu(withOrderMenu(none, true, none), false, none)).toEqual(none);
    // Over a stored one, off goes as false for the service to leave it out.
    const stored = { ...absent, orderMenu: true };
    expect(withOrderMenu(stored, false, stored).orderMenu).toBe(false);
    // The key goes last, where the read document has it.
    expect(Object.keys(on).at(-1)).toBe("orderMenu");
  });
});

describe("the money's name (Para birimi)", () => {
  it("is checked as the service checks it", () => {
    for (const name of ["TL", "coin", "ripple", "Altın Para", "₺", "$", "Gold.Coin", "Şeker_Ç"]) {
      expect(currencyProblem(name), name).toBeNull();
    }
    // Spaces are cleaned away, and an empty field is no name.
    expect(currencyProblem("  Altın   Para ")).toBeNull();
    expect(currencyProblem("   ")).toBeNull();
    // A digit is a number to the reply rule; a name needs a letter or a sign.
    for (const name of ["TL2", "100 coin", "-", "...", "TL!", "Аltın", "&aCoin"]) {
      expect(currencyProblem(name), name).toBe("form");
    }
    expect(currencyProblem("x".repeat(CURRENCY_MAX))).toBeNull();
    expect(currencyProblem("x".repeat(CURRENCY_MAX + 1))).toBe("length");
  });

  it("shows only while Studio's switch is on, a profile's own name read-only", () => {
    const money = (label: string | null, fixed: boolean) => ({
      money: {
        label: label === null ? null : { tr: label, en: label },
        fixed,
        maxDeal: 10_000_000,
        currency: "smp",
      },
    });
    const features = {
      orders: true,
      audit: true,
      customItems: true,
      itemPay: false,
      orderMenu: false,
    };
    const on = { features: { ...features, currency: true } };
    const off = { features: { ...features, currency: false } };
    expect(currencyFieldState(money(null, false), off, "tr")).toEqual({ kind: "hidden" });
    // An older service sends neither the switch nor `fixed`: no field either.
    const older = JSON.parse('{"features": {"orders": true}}');
    expect(currencyFieldState(money("ripple", false), older, "tr")).toEqual({ kind: "hidden" });
    // Projects SMP names no money: the player's own field, named or not.
    expect(currencyFieldState(money(null, false), on, "tr")).toEqual({ kind: "own" });
    expect(currencyFieldState(money("TL", false), on, "tr")).toEqual({ kind: "own" });
    // Survival's profile names it (ripple): read-only.
    expect(currencyFieldState(money("ripple", true), on, "en")).toEqual({
      kind: "fixed",
      label: "ripple",
    });
  });

  it("goes back as it came: left out, cleared or typed", () => {
    // A document without a name, as the service sends it (none stored, or a service without it).
    const absent: PlusConfig["trade"] = JSON.parse('{"returnRow": null}');
    const saved = settingsPart(config({ trade: absent }));
    const typing = (base: SettingsPart, text: string) => (shown: SettingsPart) => ({
      ...shown,
      trade: withCurrency(shown.trade, base.trade, text),
    });
    // Typed and emptied again: no edit is left, so no save writes the key.
    const typed = editDraft(saved, null, typing(saved, "TL"));
    expect(typed?.value.trade.currency).toBe("TL");
    expect(editDraft(saved, typed, typing(saved, "  "))).toBeNull();
    // Another edit sends the trade settings without it: the stored name stays.
    const other = editDraft(saved, null, (shown) => ({ ...shown, summaryAt: "22:00" }));
    const sent = withSettings(config({ trade: absent }), applyDraft(saved, other));
    expect(JSON.parse(JSON.stringify(sent)).trade).toEqual({ returnRow: null });
    // A stored name emptied goes as null (the save clears it); a typed one goes as typed.
    const named = settingsPart(config({ trade: { returnRow: null, currency: "TL" } }));
    const cleared = editDraft(named, null, typing(named, ""));
    expect(cleared?.value.trade).toEqual({ returnRow: null, currency: null });
    const renamed = editDraft(named, null, typing(named, "Altın Para"));
    expect(renamed?.value.trade.currency).toBe("Altın Para");
    // An untouched name takes the newest (the phone renamed it meanwhile).
    const latest = settingsPart(config({ trade: { returnRow: null, currency: "coin" } }));
    const lang = editDraft(named, null, (shown) => ({ ...shown, lang: "en" }));
    expect(applyDraft(latest, lang).trade.currency).toBe("coin");
    // Only a name the player typed is the panel's to refuse.
    expect(currencyEditProblem(typing(named, "TL2")(named).trade, named.trade)).toBe("form");
    const odd = { returnRow: null, currency: "TL2" };
    expect(currencyEditProblem(odd, odd)).toBeNull();
    expect(currencyEditProblem(absent, absent)).toBeNull();
  });
});

describe("Sınırlara git: the limit a line is about", () => {
  it("goes from an approval's reason to its limit, and from no other", () => {
    const cases: Array<[string, LimitField | undefined]> = [
      ["above", "approvalAbove"],
      ["new_counterpart", "newCounterpartApprovalAbove"],
      ["daily", "dailySpendMax"],
      ["budget", "totalBudget"],
      // The authority the player chose, or a price, is no limit to go to.
      ["authority", undefined],
      ["price_unusual", undefined],
      ["other", undefined],
      ["constructor", undefined],
    ];
    for (const [reason, field] of cases) expect(limitReason([reason])?.field, reason).toBe(field);
    // The first reason that is a limit's carries it.
    const first = limitReason(["price_unusual", "daily", "above"]);
    expect(first).toEqual({ reason: "daily", field: "dailySpendMax" });
    expect(limitReason([])).toBeNull();
  });

  it("goes from buying that waits for the spending limits to the first one missing", () => {
    expect(spendingField({ perDealMax: null, totalBudget: null })).toBe("perDealMax");
    expect(spendingField({ perDealMax: 50_000, totalBudget: null })).toBe("totalBudget");
  });
});

describe("the first start (AFK+ never starts by itself)", () => {
  it("says ProBot AI is ready and offers to start it while it waits", () => {
    const off = aiState(ai({ state: "off", reason: "player" }));
    expect(startState(off, true)).toEqual({
      key: "ready",
      tone: "ok",
      until: null,
      canStart: true,
      canStop: false,
    });
    expect(trAfkPlus.ai.ready).toBe("ProBot AI hazır. Başlatınca çalışır.");
    // Never a bare "Başlat": beside the account's "Durdur" it read as the account's (SITE-W2).
    expect(trAfkPlus.control.start).toBe("ProBot AI'ı başlat");
    expect(enAfkPlus.control.start).toBe("Start ProBot AI");
  });

  it("reads the service's ready the same way, and other reasons as before", () => {
    const ready = aiState(ai({ state: "off", reason: "ready" }));
    expect(ready).toMatchObject({ key: "ready", tone: "ok", canStart: true, canStop: false });
    expect(startState(ready, false)).toBe(ready);
    // A started account switched off keeps its own line; a reason the launcher does not know
    // reads as it always did.
    expect(aiState(ai({ state: "off", reason: "player" }))).toMatchObject({ key: "player" });
    expect(aiState(ai({ state: "off", reason: "somethingNew" }))).toMatchObject({ key: "paused" });
  });

  it("leaves every other state as it is, a pause too", () => {
    const paused = aiState(ai({ state: "paused", reason: "break", until: 1 }));
    expect(startState(paused, false)).toBe(paused);
    const on = aiState(ai({}));
    expect(startState(on, false)).toBe(on);
  });

  it("ends with Başlat, while Durdur leaves the mark as it is", () => {
    const waiting = config({ enabled: false, ready: true });
    const started = configAfterControl(waiting, "resume");
    expect([started.enabled, started.ready]).toEqual([true, null]);
    const stopped = configAfterControl(waiting, "pause");
    expect([stopped.enabled, stopped.ready]).toEqual([false, true]);
    // Blocking still takes the player off the friends list.
    const friends = config({ who: { mode: "friends", friends: ["Pixel_Fox9"] } });
    const blocked = configAfterControl(friends, "block", "pixel_fox9");
    expect([blocked.blocked, blocked.who.friends]).toEqual([["pixel_fox9"], []]);
  });
});

describe("the assistant's defaults", () => {
  it('take "Kibar" where the settings name no style, and keep a stored one', () => {
    expect(presetOf("brief")).toBe("brief");
    expect(presetOf("friendly")).toBe("friendly");
    expect(presetOf("")).toBe("polite");
    expect(presetOf("somethingNew")).toBe("polite");
  });
});

describe("AFK+ settings history", () => {
  it("says what changed, most telling first", () => {
    const before = config({ orders: [order()] });
    const after = config({
      orders: [
        order({ limit: 45_000 }),
        order({ id: "o2", side: "sell", item: "minecraft:diamond" }),
      ],
      authority: "deal",
      limits: { ...before.limits, perDealMax: 50_000 },
    });
    expect(configChanges(before, after)).toEqual([
      {
        kind: "order_amount",
        side: "buy",
        item: "minecraft:elytra",
        field: "limit",
        from: 40_000,
        to: 45_000,
        pay: "money",
      },
      { kind: "order_added", side: "sell", item: "minecraft:diamond" },
      { kind: "authority", from: "ask", to: "deal" },
      { kind: "limit", field: "perDealMax", from: null, to: 50_000 },
    ]);
    expect(configChanges(before, config({ orders: [] }))).toEqual([
      { kind: "order_removed", side: "buy", item: "minecraft:elytra" },
    ]);
    expect(configChanges(before, { ...before, pay: "both" })).toEqual([{ kind: "fineTune" }]);
    expect(configChanges(before, { ...before, summaryAt: "22:00" })).toEqual([{ kind: "other" }]);
  });
});

describe("the AI's state", () => {
  it("reads as one sentence with what the player can do", () => {
    expect(aiState(ai({}))).toMatchObject({ key: "on", tone: "ok", canStop: true });
    expect(aiState(null)).toMatchObject({ key: "unknown" });
    expect(aiState(ai({ state: "off", reason: "player" }))).toMatchObject({
      key: "player",
      canStart: true,
    });
    expect(aiState(ai({ state: "paused", reason: "quota", until: 1_792_418_400 }))).toMatchObject({
      key: "quotaUntil",
      tone: "warn",
      until: { at: 1_792_418_400, how: "at" },
      // A pause that ends by itself can still be stopped for good.
      canStop: true,
    });
    // The full window is named when the brief says which.
    const brief = {
      w5hResetAt: null,
      weekResetAt: 1,
      exhaustedUntil: null,
      giftsLeft: 0,
      boughtLeft: 0,
    };
    expect(
      aiState(
        ai({
          state: "paused",
          reason: "quota",
          until: 1,
          quota: { ...brief, pct5: 100, pctWeek: 64 },
        }),
      ),
    ).toMatchObject({ key: "quotaW5hUntil" });
    expect(
      aiState(
        ai({
          state: "paused",
          reason: "quota",
          until: 1,
          quota: { ...brief, pct5: 30, pctWeek: 100 },
        }),
      ),
    ).toMatchObject({ key: "quotaWeekUntil" });
    expect(aiState(ai({ state: "paused", reason: "break", until: 1 }))).toMatchObject({
      key: "breakUntil",
      until: { how: "until" },
    });
    expect(aiState(ai({ state: "off", reason: "gate" }))).toMatchObject({ canStart: true });
    expect(aiState(ai({ state: "paused", reason: "somethingNew" }))).toMatchObject({
      key: "paused",
    });
  });

  it("says what trading can do now", () => {
    expect(tradeNoteKey(ai({}))).toBeNull();
    expect(tradeNoteKey(ai({ trade: { on: false, off: "mode", by: "bot", buy: "on" } }))).toBe(
      "tradeLobby",
    );
    expect(tradeNoteKey(ai({ trade: { on: false, off: "profile", by: "owner", buy: "on" } }))).toBe(
      "tradeOwner",
    );
    expect(tradeNoteKey(ai({ trade: { on: true, off: null, by: "bot", buy: "notify" } }))).toBe(
      "notifyOnly",
    );
  });

  const quota = (windowsLeft: number, gift: number, estReplies: number, bought = 0): PlusQuota => ({
    w5h: { used: 5_000 - windowsLeft, cap: 5_000, resetAt: null },
    week: { used: 0, cap: 20_000, resetAt: 1 },
    gifts: gift ? [{ id: "g1", remaining: gift, expiresAt: null }] : [],
    bought: { remaining: bought, packs: bought ? [{ id: "gr_b1", remaining: bought }] : [] },
    promo: null,
    frozen: false,
    estReplies,
    pct5: 0,
    pctWeek: 0,
    asOf: 1,
  });

  it("counts the gifts' share of the replies left (the room holds both)", () => {
    // 3,000 of the windows and 3,000 of a gift at 25 a reply: 240 left, 120 of them the gift's.
    expect(giftReplies(quota(3_000, 3_000, 240))).toBe(120);
    // The windows used up: every reply left is the gift's.
    expect(giftReplies(quota(0, 3_000, 120))).toBe(120);
    expect(giftReplies(quota(3_000, 0, 120))).toBe(0);
    expect(giftReplies(quota(0, 0, 0))).toBe(0);
    // Bought credits are room too: 3,000 + 3,000 gift + 6,000 bought over 480 replies.
    expect(giftReplies(quota(3_000, 3_000, 480, 6_000))).toBe(120);
  });

  it("counts the bought credits' share of the replies left (AFK+ extra credits)", () => {
    // 3,000 + 3,000 gift + 6,000 bought over 480 replies: half of them the bought credits'.
    expect(boughtReplies(quota(3_000, 3_000, 480, 6_000))).toBe(240);
    // The windows and the gifts used up: every reply left is the bought credits'.
    expect(boughtReplies(quota(0, 0, 400, 8_000))).toBe(400);
    expect(boughtReplies(quota(3_000, 0, 120))).toBe(0);
    expect(boughtReplies(quota(0, 0, 0, 8_000))).toBe(0);
  });

  it("says when bought credits pay (AFK+ extra credits)", () => {
    expect(boughtLeft(quota(0, 0, 400, 8_000))).toBe(8_000);
    expect(boughtInUse(quota(0, 0, 400, 8_000))).toBe(true);
    // The windows or a gift still pay first.
    expect(boughtInUse(quota(500, 0, 400, 8_000))).toBe(false);
    expect(boughtInUse(quota(0, 100, 400, 8_000))).toBe(false);
    expect(boughtInUse({ ...quota(0, 0, 400, 8_000), frozen: true })).toBe(false);
    expect(boughtInUse(quota(0, 0, 0))).toBe(false);
  });
});
