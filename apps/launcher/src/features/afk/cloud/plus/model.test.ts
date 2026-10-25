import { describe, expect, it } from "vitest";
import {
  aiState,
  blockedNameProblem,
  buyRowProblems,
  canSpend,
  configChanges,
  giftReplies,
  isProvableSale,
  isWide,
  limitProblems,
  newBuyRow,
  newRowId,
  newSellRow,
  personaTextProblem,
  readTime,
  rowFromDraft,
  sellRowProblems,
  tradeNoteKey,
  windowProblems,
} from "./model";
import type { PlusAi, PlusConfig, PlusParseDraft, PlusQuota } from "./types";

function config(parts: Partial<PlusConfig> = {}): PlusConfig {
  return {
    v: 1,
    enabled: true,
    mode: "projects-smp",
    lang: "auto",
    persona: { preset: "friendly", intro: "", status: "", statusPreset: null, smallTalk: true },
    replies: { scope: "trade", away: true, saveAt: 80 },
    buy: [],
    sell: [],
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

function ai(parts: Partial<PlusAi>): PlusAi {
  return {
    state: "on",
    reason: null,
    staffReason: null,
    until: null,
    trade: { on: true, off: null, buy: "on" },
    pendingApprovals: 0,
    quota: null,
    asOf: null,
    spent: null,
    newMessages: null,
    ...parts,
  };
}

describe("AFK+ rows", () => {
  it("get new ids that are not taken", () => {
    const taken = config({ buy: [newBuyRow("aaaaaaaa", "minecraft:elytra")] });
    let n = 0;
    // The first draw repeats the taken id, the second does not.
    const random = () => (n++ < 8 ? 0 : 0.5);
    const id = newRowId(taken, random);
    expect(id).toMatch(/^[a-z0-9]{8}$/);
    expect(id).not.toBe("aaaaaaaa");
  });

  it("are checked as the Worker checks them", () => {
    expect(
      buyRowProblems(newBuyRow("b1", "minecraft:elytra", { target: 32_000, max: 40_000 })),
    ).toEqual([]);
    expect(buyRowProblems(newBuyRow("b1", "x", { target: 40_000, max: 30_000 }))).toEqual([
      "order",
    ]);
    expect(buyRowProblems(newBuyRow("b1", "x", { qty: 0, target: 1, max: 20_000_000 }))).toEqual([
      "qty",
      "maxDeal",
    ]);
    expect(sellRowProblems(newSellRow("s1", "x", { target: 100, min: 120 }))).toEqual(["order"]);
    expect(isWide({ target: 4_000, max: 40_001 })).toBe(true);
    expect(isWide({ target: 4_000, max: 40_000 })).toBe(false);
  });

  it("say which sale prices the trade window shows exactly", () => {
    expect(isProvableSale(950)).toBe(true);
    expect(isProvableSale(37_000)).toBe(true);
    expect(isProvableSale(37_500)).toBe(false);
    expect(isProvableSale(3_000_000)).toBe(true);
    expect(isProvableSale(3_500_000)).toBe(false);
  });

  it("come from a typed rule's draft", () => {
    const draft: PlusParseDraft = {
      side: "buy",
      item: null,
      candidates: [],
      qty: 1,
      unit: "total",
      target: 32_000,
      max: 40_000,
      min: null,
      style: "balanced",
      approval: "above",
    };
    const buy = rowFromDraft(draft, "minecraft:elytra", "b1");
    expect(buy.side).toBe("buy");
    expect(buy.row).toMatchObject({ item: "minecraft:elytra", target: 32_000, max: 40_000 });
    const sell = rowFromDraft(
      { ...draft, side: "sell", target: 120, max: null, min: 100, qty: 64, unit: "each" },
      "minecraft:diamond",
      "s1",
    );
    expect(sell.side).toBe("sell");
    expect(sell.row).toMatchObject({ qty: 64, target: 120, min: 100, dailyQty: 64 });
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
    expect(limitProblems({ ...config().limits, totalBudget: 20_000_000 })).toEqual({
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
    expect(personaTextProblem("[AFK+] selam", 60)).toBe("marker");
    expect(personaTextProblem("discord.gg/abc", 60)).toBe("link");
    expect(personaTextProblem("projects.gg'de oynuyorum", 60)).toBeNull();
    expect(personaTextProblem("0555 555 55 55", 80)).toBe("phone");
    expect(personaTextProblem("x".repeat(61), 60)).toBe("length");
  });
});

describe("AFK+ settings history", () => {
  it("says what changed, most telling first", () => {
    const before = config({
      buy: [newBuyRow("b1", "minecraft:elytra", { target: 32_000, max: 40_000 })],
    });
    const after = config({
      buy: [newBuyRow("b1", "minecraft:elytra", { target: 32_000, max: 45_000 })],
      sell: [newSellRow("s1", "minecraft:diamond")],
      limits: { ...before.limits, perDealMax: 50_000 },
    });
    expect(configChanges(before, after)).toEqual([
      {
        kind: "row_amount",
        side: "buy",
        item: "minecraft:elytra",
        field: "max",
        from: 40_000,
        to: 45_000,
      },
      { kind: "row_added", side: "sell", item: "minecraft:diamond" },
      { kind: "limit", field: "perDealMax", from: null, to: 50_000 },
    ]);
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
    expect(tradeNoteKey(ai({ trade: { on: false, off: "mode", buy: "on" } }))).toBe("tradeLobby");
    expect(tradeNoteKey(ai({ trade: { on: true, off: null, buy: "notify" } }))).toBe("notifyOnly");
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
