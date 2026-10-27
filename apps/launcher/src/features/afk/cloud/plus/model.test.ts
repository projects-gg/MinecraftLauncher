import { describe, expect, it } from "vitest";
import {
  aiState,
  applyDraft,
  blockedNameProblem,
  canSpend,
  configChanges,
  editDraft,
  giftReplies,
  limitProblems,
  liveProblems,
  PREFIX_DEFAULT,
  personaTextProblem,
  prefixProblem,
  readTime,
  settingsPart,
  tradeNoteKey,
  windowProblems,
} from "./model";
import type { PlusAi, PlusConfig, PlusOrder, PlusQuota } from "./types";

function config(parts: Partial<PlusConfig> = {}): PlusConfig {
  return {
    v: 2,
    enabled: true,
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
    discovery: { scanChat: true, proactivePm: true, ads: false, adTemplate: "buy-basic" },
    trade: { returnRow: null },
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
    const brief = { w5hResetAt: null, weekResetAt: 1, exhaustedUntil: null, giftsLeft: 0 };
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

  it("counts the gifts' share of the replies left (the room holds both)", () => {
    const quota = (windowsLeft: number, gift: number, estReplies: number): PlusQuota => ({
      w5h: { used: 5_000 - windowsLeft, cap: 5_000, resetAt: null },
      week: { used: 0, cap: 20_000, resetAt: 1 },
      gifts: gift ? [{ id: "g1", remaining: gift, expiresAt: null }] : [],
      promo: null,
      frozen: false,
      estReplies,
      pct5: 0,
      pctWeek: 0,
      asOf: 1,
    });
    // 3,000 of the windows and 3,000 of a gift at 25 a reply: 240 left, 120 of them the gift's.
    expect(giftReplies(quota(3_000, 3_000, 240))).toBe(120);
    // The windows used up: every reply left is the gift's.
    expect(giftReplies(quota(0, 3_000, 120))).toBe(120);
    expect(giftReplies(quota(3_000, 0, 120))).toBe(0);
    expect(giftReplies(quota(0, 0, 0))).toBe(0);
  });
});
