// AFK+ for browser previews: the data behind the mock (mock.ts). Preview switch, read once from the
// page URL beside `?cloud=`: `&plus=<scenario>`
// - (none) or `off`: the service's plusMode is off; nothing of AFK+ shows anywhere;
// - `on`: AFK+ is open; "Ahmet_AFK" is an AFK+ account in game with two approvals waiting;
// - `setup`: the same account before its first settings (the setup wizard);
// - `paused`: its AI allowance is used up until later today;
// - `offline`: its machine is not connected (the quota, the feed and the messages wait);
// - `invite`: AFK+ is open to invited players only and this player is not one (nothing shows).
// With `?cloud=intro` (no cloud account) `&plus=on` shows the AFK+ card beside 7/24 Bulut's.
import type { AfkNotice } from "@/lib/api/social-types";
import type { CloudAccount } from "../api";
import type {
  CloudNotifyPrefs,
  MePlus,
  PlusAi,
  PlusApproval,
  PlusCatalog,
  PlusConfig,
  PlusDeal,
  PlusFeedItem,
  PlusHistoryEntry,
  PlusInboxLine,
  PlusInboxThread,
  PlusOffer,
  PlusQuota,
  PlusSettings,
  PlusWeeklyReport,
} from "./types";

export type PlusScenario = "off" | "on" | "setup" | "paused" | "offline" | "invite";

const SCENARIOS: PlusScenario[] = ["off", "on", "setup", "paused", "offline", "invite"];

function param(key: string): string | null {
  try {
    const fromSearch = new URLSearchParams(window.location.search).get(key);
    const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
    return fromSearch ?? fromHash;
  } catch {
    return null;
  }
}

export const plusScenario: PlusScenario = (() => {
  const value = param("plus");
  return SCENARIOS.find((s) => s === value) ?? "off";
})();

export const nowSecs = () => Math.floor(Date.now() / 1000);
/** When the 5-hour window renews: one moment for the state line, the bar and the notice. */
const W5H_RESET_AT = nowSecs() + 2 * 3_600 + 20 * 60;
const MIN = 60;
const HOUR = 3_600;
const DAY = 86_400;

export const PLUS_ID = "a_5RT8KQ2MZ7WD4XB9";
export const PLUS_NAME = "Ahmet_AFK";

export function mockOffer(): PlusOffer | null {
  if (plusScenario === "off") return null;
  return {
    mode: plusScenario === "invite" ? "invite" : "on",
    sales: true,
    prices: [
      { plan: "month", priceTl: "200.00", days: 30 },
      { plan: "year", priceTl: "2000.00", days: 365 },
    ],
    stock: { soldOut: false, remaining: null },
    refundWindowHours: 24,
    termsVersion: 1,
    allowance: { month: { w5h: 5_000, week: 20_000 }, year: { w5h: 5_000, week: 20_000 } },
    features: {
      inbox: true,
      parse: true,
      schedule: true,
      report: true,
      history: true,
      saver: true,
      notifyDigest: false,
      handoff: false,
      watch: false,
    },
  };
}

export function mockMe(): MePlus | null {
  if (plusScenario === "off") return null;
  return {
    canBuy:
      plusScenario === "invite"
        ? { ok: false, reason: "plus_invite_only", until: null }
        : { ok: true, reason: null, until: null },
    restricted: null,
  };
}

/** Whether the scenario has an AFK+ account. */
export const hasPlusAccount = ["on", "setup", "paused", "offline"].includes(plusScenario);

export function mockAi(): PlusAi {
  const now = nowSecs();
  const paused = plusScenario === "paused";
  return {
    state: paused ? "paused" : plusScenario === "offline" ? "unknown" : "on",
    reason: paused ? "quota" : null,
    staffReason: null,
    until: paused ? W5H_RESET_AT : null,
    trade: { on: true, off: null, buy: "on" },
    // A used-up allowance stops only the AI: approvals go on (cloud-afk README, "Stops and pauses").
    pendingApprovals: plusScenario === "setup" ? 0 : 2,
    quota: {
      pct5: paused ? 100 : 40,
      pctWeek: paused ? 64 : 12,
      w5hResetAt: W5H_RESET_AT,
      weekResetAt: nextMonday(now),
      exhaustedUntil: paused ? W5H_RESET_AT : null,
      // Used up means the gift is gone too (it is spent before the AI stops).
      giftsLeft: paused ? 0 : 3_000,
    },
    asOf: null,
    // The completed buys since the last reset (`mockDeals`: 36.000 + 16.400), never a pending one.
    spent: plusScenario === "offline" ? null : { total: 52_400, today: 0 },
    newMessages: plusScenario === "on" ? { count: 2, at: now - 3 * MIN } : null,
  };
}

function nextMonday(now: number): number {
  const date = new Date(now * 1000);
  const day = (date.getDay() + 6) % 7;
  date.setDate(date.getDate() + (7 - day));
  date.setHours(4, 0, 0, 0);
  return Math.floor(date.getTime() / 1000);
}

/** The 7/24 Bulut mock's accounts with AFK+'s fields, and the AFK+ account first. */
export function withPlus(accounts: CloudAccount[], make: () => CloudAccount): CloudAccount[] {
  const plain = accounts.map((account) => ({
    ...account,
    tier: plusScenario === "off" ? "" : "plain",
    plusUntil: null,
    ai: null,
  }));
  if (!hasPlusAccount) return plain;
  return [make(), ...plain];
}

export function plusAccountParts(): Partial<CloudAccount> {
  const now = nowSecs();
  return {
    tier: "plus",
    plusUntil: now + 24 * DAY,
    expiresAt: now + 24 * DAY,
    ai: mockAi(),
  };
}

const item = (
  id: string,
  tr: string,
  en: string,
  stack = 64,
  damageable = false,
): PlusCatalog["items"][number] => ({
  id,
  tr,
  en,
  sprite: id.replace(/^minecraft:/, ""),
  stack,
  damageable,
});

export const CATALOG: PlusCatalog = {
  mode: "projects-smp",
  name: { tr: "SMP", en: "SMP" },
  items: [
    item("minecraft:elytra", "Elytra", "Elytra", 1, true),
    item("minecraft:totem_of_undying", "Ölümsüzlük Totemi", "Totem of Undying", 1),
    item("minecraft:diamond", "Elmas", "Diamond"),
    item("minecraft:diamond_block", "Elmas Bloğu", "Block of Diamond"),
    item("minecraft:netherite_ingot", "Netherit Külçesi", "Netherite Ingot"),
    item("minecraft:netherite_sword", "Netherit Kılıç", "Netherite Sword", 1, true),
    item("minecraft:netherite_pickaxe", "Netherit Kazma", "Netherite Pickaxe", 1, true),
    item("minecraft:enchanted_golden_apple", "Büyülü Altın Elma", "Enchanted Golden Apple"),
    item("minecraft:golden_apple", "Altın Elma", "Golden Apple"),
    item("minecraft:emerald", "Zümrüt", "Emerald"),
    item("minecraft:gold_ingot", "Altın Külçesi", "Gold Ingot"),
    item("minecraft:iron_ingot", "Demir Külçesi", "Iron Ingot"),
    item("minecraft:beacon", "Fener", "Beacon"),
    item("minecraft:shulker_shell", "Shulker Kabuğu", "Shulker Shell"),
    item("minecraft:ender_pearl", "Ender İncisi", "Ender Pearl", 16),
    item("minecraft:experience_bottle", "Büyü Şişesi", "Bottle o' Enchanting"),
  ],
  enchantments: [
    { id: "minecraft:unbreaking", tr: "Kırılmazlık", en: "Unbreaking", max: 3 },
    { id: "minecraft:mending", tr: "Onarım", en: "Mending", max: 1 },
    { id: "minecraft:sharpness", tr: "Keskinlik", en: "Sharpness", max: 5 },
  ],
  money: { currency: "smp", maxDeal: 10_000_000 },
  limits: { listMax: 10, approvalTimeoutMin: [1, 60] },
  adTemplates: [
    { id: "buy-basic", tr: "Alıyorum: … PM at", en: "Buying: … DM me" },
    { id: "sell-basic", tr: "Satıyorum: … PM at", en: "Selling: … DM me" },
  ],
};

export function mockConfig(): PlusConfig {
  return {
    v: 1,
    enabled: plusScenario !== "setup",
    mode: "projects-smp",
    lang: "auto",
    persona: {
      preset: "friendly",
      // Not written yet: the field shows its example in the interface's language.
      intro: "",
      status: "",
      statusPreset: plusScenario === "setup" ? null : "back_evening",
      smallTalk: true,
    },
    replies: { scope: "trade", away: true, saveAt: 80 },
    buy:
      plusScenario === "setup"
        ? []
        : [
            {
              id: "b1",
              item: "minecraft:elytra",
              req: { ench: {}, minDurabilityPct: 90, allowCustomName: false, allowCurses: false },
              qty: 1,
              unit: "total",
              target: 32_000,
              max: 40_000,
              style: "balanced",
              approval: "above",
              dailyQty: 1,
              notify: true,
              enabled: true,
            },
            {
              id: "b2",
              item: "minecraft:totem_of_undying",
              req: { ench: {}, minDurabilityPct: 90, allowCustomName: false, allowCurses: false },
              qty: 3,
              unit: "each",
              target: 8_000,
              max: 10_000,
              style: "patient",
              approval: "auto",
              dailyQty: 3,
              notify: true,
              enabled: true,
            },
          ],
    sell:
      plusScenario === "setup"
        ? []
        : [
            {
              id: "s1",
              item: "minecraft:diamond",
              qty: 64,
              unit: "each",
              target: 120,
              min: 100,
              style: "patient",
              approval: "auto",
              dailyQty: 128,
              enabled: true,
            },
          ],
    limits: {
      perDealMax: plusScenario === "setup" ? null : 50_000,
      dailySpendMax: plusScenario === "setup" ? null : 100_000,
      totalBudget: plusScenario === "setup" ? null : 300_000,
      balanceFloor: plusScenario === "setup" ? 0 : 20_000,
      approvalAbove: 35_000,
      newCounterpartApprovalAbove: plusScenario === "setup" ? null : 10_000,
      approvalTimeoutMin: 10,
    },
    discovery: { scanChat: true, proactivePm: true, ads: false, adTemplate: "buy-basic" },
    trade: { returnRow: null },
    summaryAt: "21:00",
    blocked: plusScenario === "setup" ? [] : ["Dolandirici_X", "Hizli_Satis"],
    schedule: {
      on: plusScenario !== "setup",
      windows: [
        { days: [1, 2, 3, 4, 5], from: "16:00", to: "02:00" },
        { days: [6, 7], from: "10:00", to: "03:00" },
      ],
    },
    pauseUntil: null,
  };
}

export function mockSettings(config: PlusConfig, aiRev: number): PlusSettings {
  const now = nowSecs();
  return {
    config,
    aiRev,
    judge: {
      intro: { state: "ok", reason: null, at: now - DAY },
      status: { state: "ok", reason: null, at: now - DAY },
    },
    profile: { id: "projects-smp", name: { tr: "SMP", en: "SMP" }, trade: true },
    // The completed buys since the last reset (`mockDeals`: 36.000 + 16.400), never a pending one.
    spent: plusScenario === "offline" ? null : { total: 52_400, today: 0 },
  };
}

export function mockQuota(): PlusQuota {
  const now = nowSecs();
  const paused = plusScenario === "paused";
  return {
    w5h: {
      used: paused ? 5_000 : 2_000,
      cap: 5_000,
      resetAt: W5H_RESET_AT,
    },
    week: { used: paused ? 12_800 : 2_400, cap: 20_000, resetAt: nextMonday(now) },
    gifts: paused ? [] : [{ id: "g1", remaining: 3_000, expiresAt: now + 6 * DAY }],
    promo: null,
    frozen: false,
    // The ledger's room counts the windows and the gifts: (3,000 + 3,000) at 25 credits a reply.
    estReplies: paused ? 0 : 240,
    pct5: paused ? 100 : 40,
    pctWeek: paused ? 64 : 12,
    asOf: now,
  };
}

export function mockApprovals(): PlusApproval[] {
  if (plusScenario === "setup") return [];
  const now = nowSecs();
  return [
    {
      id: "01JBXK3D2E7Q2M9R3T5W8Y0ZA1",
      acct: PLUS_ID,
      deal: { side: "buy", item: "minecraft:elytra", qty: 1, price: 37_500, currency: "smp" },
      counterpart: "Kaan_07",
      why: ["above"],
      createdAt: now - 2 * MIN,
      expiresAt: now + 8 * MIN,
      state: "pending",
      maxPrice: null,
      closed: null,
    },
    {
      id: "01JBXK3D2E7Q2M9R3T5W8Y0ZB2",
      acct: PLUS_ID,
      // Two totems from a player the account never traded with: 9.000 each is within the list's
      // 10.000 (that row buys without asking), but 18.000 is over the 10.000 the settings ask above
      // with a new player. A second item, so two cards never read as two offers for one Elytra.
      deal: {
        side: "buy",
        item: "minecraft:totem_of_undying",
        qty: 2,
        price: 18_000,
        currency: "smp",
      },
      counterpart: "Deniz_TR",
      why: ["new_counterpart"],
      createdAt: now - 6 * MIN,
      expiresAt: now + 4 * MIN,
      state: "pending",
      maxPrice: null,
      closed: null,
    },
  ];
}

let feedSeq = 0;
/** A ULID-like id in time order. */
export function ulid(at: number): string {
  feedSeq += 1;
  return `01J${at.toString(36).toUpperCase().padStart(10, "0")}${String(feedSeq).padStart(13, "0")}`;
}

export function mockFeed(): PlusFeedItem[] {
  if (plusScenario === "setup") return [];
  const now = nowSecs();
  const at = (minutesAgo: number) => now - minutesAgo * MIN;
  const base = {
    counterpart: null,
    text: null,
    item: null,
    qty: null,
    price: null,
    currency: null,
    reason: null,
  };
  const line = (
    minutesAgo: number,
    kind: string,
    parts: Partial<PlusFeedItem> = {},
  ): PlusFeedItem => ({ ...base, id: ulid(at(minutesAgo)), at: at(minutesAgo), kind, ...parts });
  const elytra = { item: "minecraft:elytra", qty: 1, currency: "smp" };
  return [
    line(2, "approval", { counterpart: "Kaan_07", ...elytra, price: 37_500 }),
    line(3, "pm_in", { counterpart: "Kaan_07", text: "tamam anlaştık" }),
    line(4, "pm_out", { counterpart: "Kaan_07", text: "[AFK+] 37.500, son teklifim." }),
    // The counter-offer comes from the message that made it, a moment later.
    line(5, "counter", { counterpart: "Kaan_07", ...elytra, price: 40_000 }),
    line(5.5, "pm_in", { counterpart: "Kaan_07", text: "40 olmaz mı" }),
    line(6, "approval", {
      counterpart: "Deniz_TR",
      item: "minecraft:totem_of_undying",
      qty: 2,
      price: 18_000,
      currency: "smp",
    }),
    line(7, "pm_out", { counterpart: "Kaan_07", text: "[AFK+] Merhaba! 34.000 olur mu?" }),
    line(8, "pm_in", { counterpart: "Kaan_07", text: "elytra var 45k" }),
    line(26, "found", { counterpart: "Zeynep_12", ...elytra, price: 45_000 }),
    line(41, "suspicious", { counterpart: "Hizli_Satis", reason: "scam" }),
    line(95, "trade_done", {
      counterpart: "MertK",
      item: "minecraft:diamond",
      qty: 64,
      price: 7_200,
      currency: "smp",
    }),
    line(97, "trade_started", {
      counterpart: "MertK",
      item: "minecraft:diamond",
      qty: 64,
      price: 7_200,
      currency: "smp",
    }),
    line(103, "deal", {
      counterpart: "MertK",
      item: "minecraft:diamond",
      qty: 64,
      price: 7_200,
      currency: "smp",
    }),
    line(26 * 60, "summary"),
    line(27 * 60, "trade_failed", {
      counterpart: "Baris_TR",
      item: "minecraft:totem_of_undying",
      qty: 1,
      reason: "timeout",
    }),
  ];
}

export function mockThreads(): PlusInboxThread[] {
  if (plusScenario === "setup") return [];
  const now = nowSecs();
  return [
    {
      name: "Kaan_07",
      lastAt: now - 3 * MIN,
      last: { from: "them", text: "tamam anlaştık" },
      unread: 2,
      state: "active",
      handoffUntil: null,
    },
    {
      name: "MertK",
      lastAt: now - 40 * MIN,
      last: { from: "you", text: "Akşam spawn'da olurum, orada konuşalım." },
      unread: 0,
      state: "handoff",
      handoffUntil: now + 12 * MIN,
    },
    {
      name: "Zeynep_12",
      lastAt: now - 3 * HOUR,
      last: { from: "bot", text: "[AFK+] Elytra hâlâ satılık mı? 33.000 düşünüyorum." },
      unread: 0,
      state: "active",
      handoffUntil: null,
    },
    {
      name: "Hizli_Satis",
      lastAt: now - 41 * MIN,
      last: { from: "them", text: "önce sen /pay at sonra veririm" },
      unread: 0,
      state: "blocked",
      handoffUntil: null,
    },
  ];
}

export function mockLines(name: string): PlusInboxLine[] {
  const now = nowSecs();
  const at = (minutesAgo: number) => now - minutesAgo * MIN;
  if (name === "Kaan_07") {
    return [
      { at: at(8), from: "them", text: "elytra var 45k", hidden: null },
      { at: at(7), from: "bot", text: "[AFK+] Merhaba! 34.000 olur mu?", hidden: null },
      { at: at(5), from: "them", text: "40 olmaz mı", hidden: null },
      { at: at(4), from: "bot", text: "[AFK+] 37.500, son teklifim.", hidden: null },
      { at: at(3), from: "them", text: "tamam anlaştık", hidden: null },
    ];
  }
  if (name === "MertK") {
    return [
      { at: at(55), from: "them", text: "elmas lazım 64 tane", hidden: null },
      { at: at(54), from: "bot", text: "[AFK+] Var, tanesi 120. Olur mu?", hidden: null },
      { at: at(50), from: "them", text: "115 olur mu son", hidden: null },
      { at: at(40), from: "you", text: "Akşam spawn'da olurum, orada konuşalım.", hidden: null },
    ];
  }
  if (name === "Hizli_Satis") {
    return [
      { at: at(44), from: "them", text: "…", hidden: "unsafe" },
      { at: at(41), from: "them", text: "önce sen /pay at sonra veririm", hidden: null },
      {
        at: at(41),
        from: "bot",
        text: "[AFK+] Takası yalnız /takas penceresinden yaparım.",
        hidden: null,
      },
    ];
  }
  return [
    {
      at: at(190),
      from: "bot",
      text: "[AFK+] Elytra hâlâ satılık mı? 33.000 düşünüyorum.",
      hidden: null,
    },
  ];
}

export function mockHistory(current: PlusConfig, rev: number): PlusHistoryEntry[] {
  const now = nowSecs();
  const earlier: PlusConfig = {
    ...current,
    buy: current.buy.map((row) => (row.id === "b1" ? { ...row, max: 45_000 } : row)),
  };
  const first: PlusConfig = {
    ...earlier,
    sell: [],
    limits: { ...earlier.limits, perDealMax: null },
  };
  return [
    { aiRev: rev, at: now - 2 * HOUR, by: "launcher", config: current },
    { aiRev: rev - 1, at: now - DAY - 3 * HOUR, by: "site", config: earlier },
    { aiRev: rev - 2, at: now - 3 * DAY, by: "site", config: first },
  ].filter((entry) => entry.aiRev > 0);
}

export function mockDeals(): PlusDeal[] {
  const now = nowSecs();
  const deal = (
    daysAgo: number,
    side: "buy" | "sell",
    itemId: string,
    qty: number,
    price: number,
    counterpart: string,
    status = "completed",
    approval = "auto",
  ): PlusDeal => ({
    id: ulid(now - daysAgo * DAY),
    side,
    item: itemId,
    qty,
    price,
    currency: "smp",
    counterpart,
    status,
    reason: status === "failed" ? "timeout" : null,
    approval,
    at: now - Math.round(daysAgo * DAY),
  });
  return [
    deal(0.07, "sell", "minecraft:diamond", 64, 7_200, "MertK"),
    deal(1.1, "buy", "minecraft:totem_of_undying", 1, 8_500, "Baris_TR", "failed"),
    deal(2.3, "buy", "minecraft:elytra", 1, 36_000, "Zeynep_12", "completed", "approved"),
    deal(4.5, "sell", "minecraft:diamond", 64, 7_680, "Ece_Gamer"),
    deal(8.6, "buy", "minecraft:totem_of_undying", 2, 16_400, "Kaan_07"),
    deal(9.5, "sell", "minecraft:diamond", 32, 3_840, "MertK"),
  ];
}

export function mockReport(): PlusWeeklyReport {
  // Last AFK+ week, Monday 04:00 to Monday 04:00 (the contract's weeks), counted from the trades.
  const to = nextMonday(nowSecs()) - 7 * DAY;
  const from = to - 7 * DAY;
  const done = mockDeals().filter((d) => d.status === "completed" && d.at >= from && d.at < to);
  const buys = done.filter((d) => d.side === "buy");
  const sells = done.filter((d) => d.side === "sell");
  const total = (list: PlusDeal[]) => list.reduce((sum, d) => sum + d.price, 0);
  return {
    week: "2026-W42",
    from,
    to,
    accounts: [
      {
        id: PLUS_ID,
        name: PLUS_NAME,
        pms: 46,
        bought: buys.length,
        sold: sells.length,
        spent: total(buys),
        earned: total(sells),
        belowLimit: buys.length ? 1_600 : 0,
        aboveFloor: sells.length ? 640 : 0,
        blocked: { scam: 2, rmt: 1, unsafe: 1, fake: 0 },
        allowancePct: 38,
        currency: "smp",
      },
    ],
  };
}

/** `&notice=<event>`: that AFK notice shows as the launcher's toast shortly after the page loads. */
export const noticeScenario: string | null = param("notice");

/** A notice as the friends service sends it (data only; the launcher words it). */
export function mockNotice(event: string): AfkNotice {
  const at = nowSecs();
  const base = { acct: PLUS_ID, name: PLUS_NAME };
  const data: Record<string, Partial<Record<string, string>>> = {
    approval: {
      ...base,
      approval: "01JBXK3D2E",
      side: "buy",
      item: "minecraft:elytra",
      qty: "1",
      price: "37500",
      currency: "smp",
      expires: String(at + 8 * MIN),
    },
    deal_done: { ...base, side: "buy", item: "minecraft:elytra", qty: "1", price: "37500" },
    found: { ...base, item: "minecraft:totem_of_undying", price: "8200" },
    message: { ...base, count: "3" },
    quota: { ...base, which: "full", until: String(W5H_RESET_AT) },
    stopped: { ...base, reason: "banned" },
    weekly: { ...base, deals: "5" },
  };
  return {
    id: ulid(at),
    event,
    kind: event === "approval" ? "afk_deal" : "afk_info",
    priority: event === "approval" ? "high" : "normal",
    at,
    tag: event === "approval" ? "approval:01JBXK3D2E" : undefined,
    quiet: false,
    data: data[event] ?? base,
    itemName:
      event === "approval" || event === "deal_done" ? { tr: "Elytra", en: "Elytra" } : undefined,
  };
}

export function mockNotifyPrefs(): CloudNotifyPrefs {
  return {
    events: {
      stopped: true,
      renewal: true,
      expired: true,
      found: true,
      approval: true,
      deal_done: true,
      deal_failed: false,
      message: true,
      quota_full: true,
      quota_80: false,
      gift_ending: true,
      summary: false,
      weekly: true,
      paused: false,
      incident: true,
    },
    quiet: { enabled: true, from: "23:00", to: "08:00" },
  };
}
