// AFK+ for browser previews: the data behind the mock (mock.ts). Preview switches, read once from the
// page URL beside `?cloud=`:
// - `&plus=<scenario>`:
//   - (none) or `off`: the service's plusMode is off; nothing of AFK+ shows anywhere;
//   - `on`: AFK+ is open; "Ahmet_AFK" is an AFK+ account in game with orders, approvals waiting, a
//     handoff and a first sight;
//   - `setup`: the same account before its first settings (the setup wizard, no orders);
//   - `paused`: its AI allowance is used up until later today;
//   - `offline`: its machine is not connected (the quota, the log and the messages wait);
//   - `maintenance`: AFK+ is under maintenance (the field is closed, the list reads);
//   - `owner`: the account is on a server without a trade profile (the player makes the trades);
//   - `full`: the active orders are at Studio's cap;
//   - `invite`: AFK+ is open to invited players only and this player is not one (nothing shows).
// - `&pick=<state>` ("Envanterden seç"): `ok` (default), `notRunning`, `setting`, `empty`, `loading`.
// - `&itemdata=fail`: the item list cannot load (the form says so and offers to try again).
// - `&audit=off|empty`: Studio switched the log off, or nothing was logged yet.
// - `&typed=off`: Studio switched typed orders off (the form and the inventory only).
// With `?cloud=intro` (no cloud account) `&plus=on` shows the AFK+ card beside 7/24 Bulut's.
// The item data here is a small stand-in written for previews (the real file comes from our CDN).
import { type InvFrame, type InvItem, SAMPLE_INVENTORY } from "@pl/ui/inventory";
import type { AfkNotice } from "@/lib/api/social-types";
import type { CloudAccount } from "../api";
import type {
  CloudNotifyPrefs,
  MePlus,
  PlusAi,
  PlusApproval,
  PlusAuditEntry,
  PlusConfig,
  PlusCustomItem,
  PlusHistoryEntry,
  PlusInboxLine,
  PlusInboxThread,
  PlusOffer,
  PlusOrder,
  PlusOrderView,
  PlusQuota,
  PlusReceipt,
  PlusReceiptSummary,
  PlusServer,
  PlusSettings,
  PlusWeeklyReport,
} from "./types";

export type PlusScenario =
  | "off"
  | "on"
  | "setup"
  | "paused"
  | "offline"
  | "maintenance"
  | "owner"
  | "full"
  | "invite";

const SCENARIOS: PlusScenario[] = [
  "off",
  "on",
  "setup",
  "paused",
  "offline",
  "maintenance",
  "owner",
  "full",
  "invite",
];

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

/** `&pick=`: what "Envanterden seç" shows. */
export const pickScenario = param("pick") ?? "ok";
/** `&itemdata=fail`: the item list cannot load. */
export const itemDataFails = param("itemdata") === "fail";
/** `&audit=off|empty`. */
export const auditScenario = param("audit");
/** `&typed=off`: typed orders are switched off. */
export const typedOrders = param("typed") !== "off";

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
      orders: typedOrders,
      audit: auditScenario !== "off",
      customItems: true,
      itemPay: true,
      anyServer: true,
      schedule: true,
      report: true,
      history: true,
      saver: true,
      notifyDigest: false,
      handoff: true,
      watch: false,
    },
    servers: { any: true, verified: ["play.projects.gg"] },
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
export const hasPlusAccount = [
  "on",
  "setup",
  "paused",
  "offline",
  "maintenance",
  "owner",
  "full",
].includes(plusScenario);

export function mockAi(): PlusAi {
  const now = nowSecs();
  const paused = plusScenario === "paused";
  const maintenance = plusScenario === "maintenance";
  return {
    state: paused || maintenance ? "paused" : plusScenario === "offline" ? "unknown" : "on",
    reason: paused ? "quota" : maintenance ? "service" : null,
    staffReason: null,
    until: paused ? W5H_RESET_AT : null,
    trade: {
      on: plusScenario !== "owner",
      off: plusScenario === "owner" ? "profile" : null,
      by: plusScenario === "owner" ? "owner" : "bot",
      buy: "on",
    },
    // A used-up allowance stops only the AI: approvals go on (cloud-afk README, "Stops and pauses").
    pendingApprovals: plusScenario === "setup" ? 0 : plusScenario === "maintenance" ? 1 : 4,
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
    // The completed buys since the last reset (`mockReceipts`: 36.000 + 16.400).
    spent: plusScenario === "offline" ? null : { total: 52_400, today: 16_400 },
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

// --- The item data (a stand-in for previews) ----------------------------------------------------------

// Enchantments in id order; `x` the ones they cannot share an item with, `c` a curse.
const ENCH: [string, number, string, string, Record<string, unknown>?][] = [
  ["minecraft:binding_curse", 1, "Bağlılık Laneti", "Curse of Binding", { c: 1 }],
  ["minecraft:efficiency", 5, "Verimlilik", "Efficiency"],
  ["minecraft:feather_falling", 4, "Tüy Düşüşü", "Feather Falling"],
  ["minecraft:fortune", 3, "Servet", "Fortune", { x: [10] }],
  ["minecraft:infinity", 1, "Sonsuzluk", "Infinity", { x: [6] }],
  ["minecraft:looting", 3, "Yağma", "Looting"],
  ["minecraft:mending", 1, "Onarım", "Mending", { x: [4] }],
  ["minecraft:power", 5, "Güç", "Power"],
  ["minecraft:protection", 4, "Koruma", "Protection"],
  ["minecraft:sharpness", 5, "Keskinlik", "Sharpness"],
  ["minecraft:silk_touch", 1, "İpeksi Dokunuş", "Silk Touch", { x: [3] }],
  ["minecraft:unbreaking", 3, "Kırılmazlık", "Unbreaking"],
  ["minecraft:vanishing_curse", 1, "Yok Olma Laneti", "Curse of Vanishing", { c: 1 }],
];
const GROUPS = [
  [0, 6, 11, 12], // elytra
  [5, 6, 9, 11, 12], // swords
  [1, 3, 6, 10, 11, 12], // pickaxes
  [0, 6, 8, 11, 12], // armour
  [0, 2, 6, 8, 11, 12], // boots
  [4, 6, 7, 11, 12], // bow
  [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], // books
];
type Row = [string, number, number, string, string, number, Record<string, unknown>?];
const ITEMS: Row[] = [
  ["minecraft:elytra", 1, 432, "Elitra", "Elytra", 0, { a: ["elytra", "kanat"] }],
  [
    "minecraft:totem_of_undying",
    1,
    0,
    "Ölümsüzlük Totemi",
    "Totem of Undying",
    -1,
    { a: ["totem"] },
  ],
  ["minecraft:diamond", 64, 0, "Elmas", "Diamond", -1, { a: ["dia"] }],
  ["minecraft:diamond_block", 64, 0, "Elmas Bloğu", "Block of Diamond", -1],
  ["minecraft:diamond_sword", 1, 1561, "Elmas Kılıç", "Diamond Sword", 1, { a: ["dia kilic"] }],
  ["minecraft:diamond_pickaxe", 1, 1561, "Elmas Kazma", "Diamond Pickaxe", 2],
  ["minecraft:diamond_helmet", 1, 363, "Elmas Miğfer", "Diamond Helmet", 3],
  ["minecraft:diamond_chestplate", 1, 528, "Elmas Göğüslük", "Diamond Chestplate", 3],
  ["minecraft:diamond_boots", 1, 429, "Elmas Botlar", "Diamond Boots", 4],
  ["minecraft:netherite_ingot", 64, 0, "Netherit Külçesi", "Netherite Ingot", -1],
  ["minecraft:netherite_sword", 1, 2031, "Netherit Kılıç", "Netherite Sword", 1],
  ["minecraft:netherite_pickaxe", 1, 2031, "Netherit Kazma", "Netherite Pickaxe", 2],
  ["minecraft:netherite_chestplate", 1, 592, "Netherit Göğüslük", "Netherite Chestplate", 3],
  ["minecraft:enchanted_golden_apple", 64, 0, "Büyülü Altın Elma", "Enchanted Golden Apple", -1],
  ["minecraft:golden_apple", 64, 0, "Altın Elma", "Golden Apple", -1],
  ["minecraft:emerald", 64, 0, "Zümrüt", "Emerald", -1],
  ["minecraft:gold_ingot", 64, 0, "Altın Külçesi", "Gold Ingot", -1],
  ["minecraft:iron_ingot", 64, 0, "Demir Külçesi", "Iron Ingot", -1],
  ["minecraft:beacon", 64, 0, "Fener", "Beacon", -1],
  ["minecraft:lantern", 64, 0, "Fener", "Lantern", -1],
  ["minecraft:shulker_shell", 64, 0, "Shulker Kabuğu", "Shulker Shell", -1],
  ["minecraft:ender_pearl", 16, 0, "Ender İncisi", "Ender Pearl", -1],
  ["minecraft:experience_bottle", 64, 0, "Tecrübe Şişesi", "Bottle o' Enchanting", -1],
  ["minecraft:enchanted_book", 1, 0, "Büyülü Kitap", "Enchanted Book", 6],
  ["minecraft:bow", 1, 384, "Yay", "Bow", 5],
  ["minecraft:paper", 64, 0, "Kâğıt", "Paper", -1],
  ["minecraft:potion", 1, 0, "İksir", "Potion", -1],
  ["minecraft:purple_shulker_box", 1, 0, "Mor Shulker Kutusu", "Purple Shulker Box", -1],
  ["minecraft:cobblestone", 64, 0, "Kırıktaş", "Cobblestone", -1],
  ["minecraft:command_block", 64, 0, "Komut Bloğu", "Command Block", -1, { o: 1 }],
];

/** The item data file's text, as the backend hands it over. */
export const MOCK_ITEM_DATA_TEXT = JSON.stringify({
  format: 1,
  mc: "26.3",
  ench: ENCH.map(([id, max, tr, en, extra]) =>
    extra ? [id, max, tr, en, extra] : [id, max, tr, en],
  ),
  groups: GROUPS,
  items: ITEMS.map(([id, stack, damage, tr, en, group, extra]) =>
    extra ? [id, stack, damage, tr, en, group, extra] : [id, stack, damage, tr, en, group],
  ),
});

// --- Settings, orders -----------------------------------------------------------------------------

export function mockCustomItems(): PlusCustomItem[] {
  if (plusScenario === "setup") return [];
  const now = nowSecs();
  return [
    {
      k: "c1",
      name: "Yakut",
      base: "minecraft:paper",
      fp: "5f5dd9c2dc066136a75abba069336cad",
      look: { cmd: 10001, model: null },
      from: "inv",
      at: now - 3 * DAY,
    },
    // Typed, waiting for its first sight (no fingerprint yet).
    {
      k: "c2",
      name: "Ejder Kılıcı",
      base: null,
      fp: null,
      look: null,
      from: "text",
      at: now - DAY,
    },
  ];
}

function order(parts: Partial<PlusOrder> & Pick<PlusOrder, "id" | "item">): PlusOrder {
  const now = nowSecs();
  return {
    on: true,
    side: "buy",
    qty: 1,
    req: null,
    pay: "money",
    unit: "total",
    target: 1,
    limit: 1,
    style: "balanced",
    trade: "auto",
    until: now + 12 * DAY,
    note: "",
    src: "text",
    at: now - DAY,
    ...parts,
  };
}

export function mockOrders(): PlusOrder[] {
  if (plusScenario === "setup") return [];
  const now = nowSecs();
  const orders: PlusOrder[] = [
    order({
      id: "o1",
      item: "minecraft:elytra",
      req: { ench: {}, dur: 90, named: false, cursed: false },
      target: 32_000,
      limit: 40_000,
      note: "Elytra alırım, en fazla 40 bin",
      at: now - 2 * HOUR,
    }),
    order({
      id: "o2",
      item: "minecraft:totem_of_undying",
      qty: 3,
      unit: "each",
      req: { ench: {}, dur: 0, named: false, cursed: false },
      target: 8_000,
      limit: 10_000,
      style: "patient",
      src: "form",
      at: now - 3 * DAY,
      until: now + 2 * DAY,
    }),
    order({
      id: "o3",
      side: "sell",
      item: "minecraft:diamond",
      qty: 64,
      unit: "each",
      target: 120,
      limit: 100,
      style: "patient",
      src: "inv",
      at: now - 4 * DAY,
    }),
    order({
      id: "o4",
      item: "custom:c1",
      qty: 5,
      req: { ench: {}, dur: 0, named: false, cursed: false },
      target: 4_000,
      limit: 4_500,
      trade: "owner",
      note: "yakut alırım 5 tane, tanesi en fazla 900",
      at: now - DAY,
    }),
    order({
      id: "o5",
      item: "minecraft:netherite_ingot",
      qty: 2,
      req: { ench: {}, dur: 0, named: false, cursed: false },
      pay: "minecraft:diamond",
      target: 9,
      limit: 12,
      note: "2 netherit külçesi, elmasla öderim",
      at: now - 5 * HOUR,
    }),
    order({
      id: "o6",
      item: "custom:c2",
      qty: 1,
      req: { ench: {}, dur: 0, named: false, cursed: false },
      target: 60_000,
      limit: 75_000,
      note: "ejder kılıcı arıyorum 75k",
      at: now - 7 * HOUR,
    }),
    order({
      id: "o7",
      side: "sell",
      item: "minecraft:enchanted_golden_apple",
      qty: 8,
      unit: "each",
      target: 6_000,
      limit: 5_000,
      on: false,
      src: "form",
      at: now - 6 * DAY,
    }),
    // Finished: one done, one whose term passed.
    order({
      id: "o8",
      item: "minecraft:diamond_pickaxe",
      req: {
        ench: { "minecraft:efficiency": 5, "minecraft:mending": 1 },
        dur: 90,
        named: false,
        cursed: false,
      },
      target: 14_000,
      limit: 18_000,
      at: now - 9 * DAY,
      until: now + 5 * DAY,
    }),
    order({
      id: "o9",
      side: "sell",
      item: "minecraft:beacon",
      target: 90_000,
      limit: 80_000,
      at: now - 20 * DAY,
      until: now - DAY,
    }),
  ];
  if (plusScenario === "full") {
    for (let n = 0; n < 4; n++) {
      orders.push(
        order({
          id: `f${n}`,
          item:
            [
              "minecraft:emerald",
              "minecraft:gold_ingot",
              "minecraft:iron_ingot",
              "minecraft:ender_pearl",
            ][n] ?? "minecraft:emerald",
          qty: 64,
          unit: "each",
          target: 40 + n * 5,
          limit: 50 + n * 5,
          src: "form",
          at: now - (10 + n) * HOUR,
        }),
      );
    }
  }
  return orders;
}

/** The orders' states as `GET /plus` gives them. */
export function mockOrderViews(): Record<string, PlusOrderView> {
  const now = nowSecs();
  const view = (got: number, live: string | null = null, at = 0): PlusOrderView => ({
    status: "",
    got,
    at,
    live,
  });
  return {
    o1: view(0, "haggling"),
    o2: view(1, null, now - 2 * DAY),
    o3: view(20, null, now - DAY),
    o4: view(0, "your_trade"),
    o5: view(0, "waiting_ok"),
    o6: view(0, "waiting_ok"),
    o7: view(0),
    o8: view(1, null, now - 2 * DAY),
    o9: view(0),
  };
}

export function mockServer(): PlusServer {
  const owner = plusScenario === "owner";
  return {
    kind: owner ? "generic" : "verified",
    mode: owner ? "generic" : "projects-smp",
    name: owner ? null : { tr: "SMP", en: "SMP" },
    trade: owner ? "owner" : "bot",
    money: { label: null, maxDeal: owner ? 100_000_000 : 10_000_000, currency: owner ? "" : "smp" },
    mc: "26.3",
    ads: owner
      ? []
      : [
          { id: "buy-basic", tr: "Alıyorum: … PM at", en: "Buying: … DM me" },
          { id: "sell-basic", tr: "Satıyorum: … PM at", en: "Selling: … DM me" },
        ],
    on: true,
  };
}

export function mockConfig(): PlusConfig {
  const setup = plusScenario === "setup";
  return {
    v: 2,
    enabled: !setup,
    mode: plusScenario === "owner" ? "generic" : "projects-smp",
    lang: "auto",
    persona: {
      preset: "friendly",
      // Not written yet: the field shows its example in the interface's language.
      intro: "",
      status: "",
      statusPreset: setup ? null : "back_evening",
      smallTalk: false,
      playerTalk: false,
      prefix: { on: false, text: "[AFK+]" },
    },
    replies: { scope: "trade", away: true, saveAt: 80 },
    authority: "ask",
    allow: { buy: true, sell: true },
    pay: setup ? "money" : "both",
    who: { mode: "all", friends: setup ? [] : ["Kaan_07", "Ece_Gamer"] },
    orders: mockOrders(),
    customItems: mockCustomItems(),
    limits: {
      perDealMax: setup ? null : 50_000,
      dailySpendMax: setup ? null : 100_000,
      totalBudget: setup ? null : 300_000,
      balanceFloor: setup ? 0 : 20_000,
      approvalAbove: 35_000,
      newCounterpartApprovalAbove: setup ? null : 10_000,
      approvalTimeoutMin: 10,
    },
    discovery: { scanChat: true, proactivePm: true, ads: false, adTemplate: "buy-basic" },
    trade: { returnRow: null },
    summaryAt: "21:00",
    blocked: setup ? [] : ["Dolandirici_X", "Hizli_Satis"],
    schedule: {
      on: !setup,
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
    server: mockServer(),
    rules: {
      orderMax: plusScenario === "full" ? 10 : 10,
      orderStore: 20,
      customItemsMax: 20,
      orderDaysDefault: 14,
      orderDaysMax: 30,
      buyStacksPerDay: 1,
      checksPerDay: 20,
      checksLeft: plusScenario === "setup" ? 20 : 4,
      friendsMax: 50,
      noteMax: 200,
      textMax: 200,
      approvalTimeoutMin: [1, 60],
      features: {
        orders: typedOrders,
        audit: auditScenario !== "off",
        customItems: true,
        itemPay: true,
      },
    },
    orders: mockOrderViews(),
    // The completed buys since the last reset (`mockReceipts`: 36.000 + 16.400).
    spent: plusScenario === "offline" ? null : { total: 52_400, today: 16_400 },
    asOf: plusScenario === "offline" ? now - 37 * MIN : null,
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
  const list = approvalList();
  // Maintenance closes what waited for an answer (the player's own trade stays).
  return plusScenario === "maintenance"
    ? list.map((one) =>
        one.state === "pending" && one.kind !== "handoff"
          ? { ...one, state: "closed", closed: "ai_stopped" }
          : one,
      )
    : list;
}

function approvalList(): PlusApproval[] {
  const now = nowSecs();
  const base = {
    acct: PLUS_ID,
    state: "pending",
    maxPrice: null,
    closed: null,
    custom: null,
    seen: null,
  };
  return [
    {
      ...base,
      id: "01JBXK3D2E7Q2M9R3T5W8Y0ZA1",
      kind: "deal",
      order: "o5",
      // Two netherite ingots for nine diamonds: an item swap waits for the player's OK.
      deal: {
        side: "buy",
        item: "minecraft:netherite_ingot",
        qty: 2,
        price: 9,
        pay: "minecraft:diamond",
      },
      counterpart: "Deniz_TR",
      why: ["authority"],
      createdAt: now - 2 * MIN,
      expiresAt: now + 8 * MIN,
    },
    {
      ...base,
      id: "01JBXK3D2E7Q2M9R3T5W8Y0ZB2",
      kind: "handoff",
      order: "o4",
      deal: { side: "buy", item: "custom:c1", qty: 2, price: 1_800, pay: "money" },
      counterpart: "Kaan_07",
      why: [],
      createdAt: now - 40 * MIN,
      expiresAt: now + 23 * HOUR,
    },
    {
      ...base,
      id: "01JBXK3D2E7Q2M9R3T5W8Y0ZC3",
      kind: "confirm_item",
      order: "o6",
      custom: { k: "c2", field: "item" },
      deal: { side: "buy", item: "custom:c2", qty: 1, price: 72_000, pay: "money" },
      counterpart: "MertK",
      why: [],
      seen: {
        id: "minecraft:netherite_sword",
        cmd: 20077,
        model: null,
        name: "Ejder Kılıcı",
        lore: ["Efsanevi", "Hasar +12"],
      },
      createdAt: now - 5 * MIN,
      expiresAt: now + 5 * MIN,
    },
    {
      ...base,
      id: "01JBXK3D2E7Q2M9R3T5W8Y0ZD4",
      kind: "deal",
      order: "o1",
      // A buy for money above the approval limit, from a new player, in its last two minutes.
      deal: { side: "buy", item: "minecraft:elytra", qty: 1, price: 38_000, pay: "money" },
      counterpart: "Kaan_07",
      why: ["above", "new_counterpart"],
      createdAt: now - 8 * MIN,
      expiresAt: now + 2 * MIN,
    },
    // Closed in the last ten minutes ("Kapananlar"): one approved at a lower price, one the
    // maintenance closed (or that ran out).
    {
      ...base,
      id: "01JBXK3D2E7Q2M9R3T5W8Y0ZE5",
      kind: "deal",
      order: "o2",
      deal: { side: "buy", item: "minecraft:totem_of_undying", qty: 1, price: 9_800, pay: "money" },
      counterpart: "Ece_K",
      why: ["above"],
      state: "approved",
      maxPrice: 9_200,
      createdAt: now - 7 * MIN,
      expiresAt: now + 3 * MIN,
    },
    {
      ...base,
      id: "01JBXK3D2E7Q2M9R3T5W8Y0ZF6",
      kind: "deal",
      order: "o3",
      deal: { side: "sell", item: "minecraft:diamond", qty: 16, price: 1_900, pay: "money" },
      counterpart: "MertK",
      why: ["authority"],
      state: plusScenario === "maintenance" ? "closed" : "expired",
      closed: plusScenario === "maintenance" ? "ai_stopped" : null,
      createdAt: now - 14 * MIN,
      expiresAt: now - 4 * MIN,
    },
  ];
}

// --- The audit log --------------------------------------------------------------------------------

let auditCounter = 0;

/** The AFK+ day number of a moment (the sequence's day part; a stand-in for the agent's). */
function auditDay(at: number): number {
  return Math.floor((at - 4 * HOUR) / DAY);
}

export function auditEntry(
  at: number,
  k: string,
  parts: Partial<PlusAuditEntry> = {},
): PlusAuditEntry {
  auditCounter += 1;
  return { s: auditDay(at) * 1_000_000 + auditCounter, t: at * 1000, k, ...parts };
}

export function mockAudit(): PlusAuditEntry[] {
  if (plusScenario === "setup" || auditScenario === "empty") return [];
  const now = nowSecs();
  const at = (minutesAgo: number) => now - Math.round(minutesAgo * MIN);
  const list: PlusAuditEntry[] = [
    // Two days ago.
    auditEntry(at(2 * 24 * 60 + 200), "order_expired", { o: "o9", i: "minecraft:beacon" }),
    auditEntry(at(2 * 24 * 60 + 90), "paused", { r: "hours" }),
    auditEntry(at(2 * 24 * 60 + 30), "resumed"),
    // Yesterday.
    auditEntry(at(26 * 60), "pm_in", { w: "Baris_TR", x: "totem var 8500", o: "o2" }),
    auditEntry(at(26 * 60 - 1), "reply", { w: "Baris_TR", x: "[AFK+] 8.000 olur mu?", o: "o2" }),
    auditEntry(at(26 * 60 - 5), "agree", {
      w: "Baris_TR",
      o: "o2",
      i: "minecraft:totem_of_undying",
      q: 1,
      p: 8_000,
      u: "money",
    }),
    auditEntry(at(26 * 60 - 7), "trade_start", { w: "Baris_TR", o: "o2" }),
    auditEntry(at(26 * 60 - 9), "trade_done", {
      w: "Baris_TR",
      o: "o2",
      i: "minecraft:totem_of_undying",
      q: 1,
      p: 8_000,
      u: "money",
    }),
    auditEntry(at(25 * 60), "refuse", { w: "Hizli_Satis", r: "unsafe" }),
    auditEntry(at(25 * 60 - 1), "pm_in", { w: "Hizli_Satis", x: "…", h: true }),
    // Today.
    auditEntry(at(60), "found", { w: "Zeynep_12", i: "minecraft:elytra", p: 45_000, u: "money" }),
    auditEntry(at(45), "confirm_item", {
      w: "MertK",
      o: "o6",
      i: "custom:c2",
      a: "01JBXK3D2E7Q2M9R3T5W8Y0ZC3",
    }),
    auditEntry(at(40), "handoff", {
      w: "Kaan_07",
      o: "o4",
      i: "custom:c1",
      q: 2,
      p: 1_800,
      u: "money",
    }),
    auditEntry(at(9), "pm_in", { w: "Kaan_07", x: "elytra var 45k", o: "o1" }),
    auditEntry(at(8), "reply", { w: "Kaan_07", x: "[AFK+] Merhaba, 34.000 olur mu?", o: "o1" }),
    auditEntry(at(6), "counter", {
      w: "Kaan_07",
      o: "o1",
      i: "minecraft:elytra",
      p: 40_000,
      u: "money",
    }),
    auditEntry(at(5), "offer", {
      w: "Kaan_07",
      o: "o1",
      i: "minecraft:elytra",
      p: 37_500,
      u: "money",
    }),
    auditEntry(at(3), "owner_msg", { w: "MertK", x: "Akşam spawn'da olurum." }),
    auditEntry(at(2), "ask_owner", {
      w: "Deniz_TR",
      o: "o5",
      i: "minecraft:netherite_ingot",
      q: 2,
      p: 9,
      u: "minecraft:diamond",
      a: "01JBXK3D2E7Q2M9R3T5W8Y0ZA1",
    }),
    auditEntry(at(1), "gate", { w: "Hizli_Satis", r: "limit" }),
  ];
  return list.sort((a, b) => b.s - a.s);
}

// --- Messages --------------------------------------------------------------------------------------

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
      { at: at(7), from: "bot", text: "[AFK+] Merhaba, 34.000 olur mu?", hidden: null },
      { at: at(5), from: "them", text: "40 olmaz mı", hidden: null },
      { at: at(4), from: "bot", text: "[AFK+] 37.500 olur mu?", hidden: null },
      { at: at(3), from: "them", text: "tamam anlaştık", hidden: null },
    ];
  }
  if (name === "MertK") {
    return [
      { at: at(55), from: "them", text: "elmas lazım 64 tane", hidden: null },
      { at: at(54), from: "bot", text: "[AFK+] Var, tanesi 120. Olur mu?", hidden: null },
      { at: at(50), from: "them", text: "115 olur mu", hidden: null },
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
    orders: current.orders.map((o) => (o.id === "o1" ? { ...o, limit: 45_000 } : o)),
    authority: "deal",
  };
  const first: PlusConfig = {
    ...earlier,
    orders: earlier.orders.filter((o) => o.id !== "o3"),
    limits: { ...earlier.limits, perDealMax: null },
  };
  return [
    { aiRev: rev, at: now - 2 * HOUR, by: "launcher", config: current },
    { aiRev: rev - 1, at: now - DAY - 3 * HOUR, by: "site", config: earlier },
    { aiRev: rev - 2, at: now - 3 * DAY, by: "site", config: first },
  ].filter((entry) => entry.aiRev > 0);
}

// --- Receipts and the report --------------------------------------------------------------------------

export function mockReceipts(): PlusReceipt[] {
  if (plusScenario === "setup") return [];
  const now = nowSecs();
  const receipt = (
    daysAgo: number,
    side: "buy" | "sell",
    item: string,
    qty: number,
    price: number,
    counterpart: string,
    parts: Partial<PlusReceipt> = {},
  ): PlusReceipt => ({
    at: (now - Math.round(daysAgo * DAY)) * 1000,
    order: null,
    side,
    item,
    qty,
    price,
    pay: "money",
    currency: "smp",
    counterpart,
    approval: "auto",
    review: false,
    limit: null,
    ...parts,
  });
  return [
    receipt(0.07, "sell", "minecraft:diamond", 20, 2_400, "MertK", { order: "o3" }),
    receipt(1.08, "buy", "minecraft:totem_of_undying", 1, 8_000, "Baris_TR", { order: "o2" }),
    receipt(1.5, "buy", "custom:c1", 3, 2_700, "Kaan_07", { approval: "owner", review: true }),
    receipt(2.3, "buy", "minecraft:diamond_pickaxe", 1, 16_400, "Zeynep_12", {
      approval: "approved",
      order: "o8",
    }),
    receipt(4.5, "sell", "minecraft:diamond", 64, 7_680, "Ece_Gamer"),
    receipt(6.2, "buy", "minecraft:netherite_ingot", 1, 5, "Deniz_TR", {
      pay: "minecraft:diamond",
      currency: null,
      approval: "approved",
    }),
    receipt(8.6, "buy", "minecraft:elytra", 1, 36_000, "Kaan_07"),
  ];
}

export function mockSummary(): PlusReceiptSummary {
  const now = nowSecs();
  return {
    from: now - 30 * DAY,
    to: now,
    items: [
      {
        side: "sell",
        item: "minecraft:diamond",
        pay: "money",
        currency: "smp",
        deals: 2,
        qty: 84,
        total: 10_080,
        avgUnit: 120,
        minUnit: 120,
        maxUnit: 120,
      },
      {
        side: "buy",
        item: "minecraft:elytra",
        pay: "money",
        currency: "smp",
        deals: 1,
        qty: 1,
        total: 36_000,
        avgUnit: 36_000,
        minUnit: 36_000,
        maxUnit: 36_000,
      },
      {
        side: "buy",
        item: "minecraft:netherite_ingot",
        pay: "minecraft:diamond",
        currency: null,
        deals: 1,
        qty: 1,
        total: 5,
        avgUnit: 5,
        minUnit: 5,
        maxUnit: 5,
      },
    ],
    totals: [{ currency: "smp", buys: 3, sells: 2, spent: 60_400, earned: 10_080 }],
  };
}

export function mockReport(): PlusWeeklyReport {
  // Last AFK+ week, Monday 04:00 to Monday 04:00 (the contract's weeks).
  const to = nextMonday(nowSecs()) - 7 * DAY;
  const from = to - 7 * DAY;
  return {
    week: "2026-W42",
    from,
    to,
    accounts: [
      {
        id: PLUS_ID,
        name: PLUS_NAME,
        pms: 46,
        bought: 2,
        sold: 1,
        spent: 44_000,
        earned: 7_680,
        belowLimit: 1_600,
        aboveFloor: 640,
        blocked: { scam: 2, rmt: 1, unsafe: 1, fake: 0 },
        allowancePct: 38,
        currency: "smp",
      },
    ],
  };
}

// --- "Envanterden seç" ----------------------------------------------------------------------------

const YAKUT: InvItem = {
  slot: 16,
  id: "minecraft:paper",
  count: 12,
  label: { tr: "Kâğıt", en: "Paper" },
  name: "Yakut",
  cmd: 10001,
};

/** The bot's inventory as the preview shows it for `&pick=`. */
export function mockPickInventory(_id: string): {
  status: "waiting" | "ok" | "off";
  reason: string | null;
  locked: boolean;
  inventory: InvFrame | null;
  window: null;
} {
  const off = (reason: string) => ({
    status: "off" as const,
    reason,
    locked: false,
    inventory: null,
    window: null,
  });
  if (pickScenario === "notRunning") return off("not_running");
  if (pickScenario === "setting") return off("setting");
  if (pickScenario === "loading") {
    return { status: "waiting", reason: null, locked: false, inventory: null, window: null };
  }
  const items = pickScenario === "empty" ? [] : [...SAMPLE_INVENTORY.items, YAKUT];
  return {
    status: "ok",
    reason: null,
    locked: false,
    inventory: { ...SAMPLE_INVENTORY, items },
    window: null,
  };
}

/** What the machine would say a slot of the preview's inventory is. */
export function mockPickOf(
  slot: number,
):
  | { kind: "vanilla"; ref: string; ench: Record<string, number>; dur: number }
  | { kind: "custom"; name: string | null; base: string; fp: string; cmd: number | null }
  | { kind: "failed"; reason: string } {
  const item = mockPickInventory(PLUS_ID).inventory?.items.find((it) => it.slot === slot);
  if (!item) return { kind: "failed", reason: "empty" };
  if (item.inside || /shulker_box|bundle/.test(item.id))
    return { kind: "failed", reason: "container" };
  if (item.cmd !== undefined || item.potion) {
    return {
      kind: "custom",
      name: item.name ?? null,
      base: item.id,
      fp:
        item.cmd === 10001
          ? "5f5dd9c2dc066136a75abba069336cad"
          : "eeacc0821d6d8c1c9810381f8b966187",
      cmd: item.cmd ?? null,
    };
  }
  const curses = new Set(["minecraft:binding_curse", "minecraft:vanishing_curse"]);
  const ench: Record<string, number> = {};
  for (const e of item.ench ?? []) {
    const id = e.id.includes(":") ? e.id : `minecraft:${e.id}`;
    if (!curses.has(id)) ench[id] = e.lvl;
  }
  const dur = item.durability
    ? Math.floor((item.durability.left * 100) / Math.max(1, item.durability.max))
    : 0;
  return {
    kind: "vanilla",
    ref: item.id.includes(":") ? item.id : `minecraft:${item.id}`,
    ench,
    dur,
  };
}

// --- Notices ---------------------------------------------------------------------------------------

/** `&notice=<event>`: that AFK notice shows as the launcher's toast shortly after the page loads. */
export const noticeScenario: string | null = param("notice");

let noticeSeq = 0;

/** A notice as the friends service sends it (data only; the launcher words it). */
export function mockNotice(event: string): AfkNotice {
  const at = nowSecs();
  const base = { acct: PLUS_ID, name: PLUS_NAME };
  const data: Record<string, Partial<Record<string, string>>> = {
    approval: {
      ...base,
      approval: "01JBXK3D2E",
      approvalKind: "deal",
      side: "buy",
      item: "minecraft:elytra",
      qty: "1",
      price: "37500",
      pay: "money",
      expires: String(at + 8 * MIN),
    },
    handoff: {
      ...base,
      approval: "01JBXK3D2F",
      approvalKind: "handoff",
      side: "buy",
      item: "custom:c1",
      qty: "2",
      price: "1800",
      pay: "money",
    },
    confirm_item: {
      ...base,
      approval: "01JBXK3D2G",
      approvalKind: "confirm_item",
      side: "buy",
      item: "custom:c2",
    },
    deal_done: { ...base, side: "buy", item: "minecraft:elytra", qty: "1", price: "37500" },
    found: { ...base, item: "minecraft:totem_of_undying", price: "8200" },
    message: { ...base, count: "3" },
    quota: { ...base, which: "full", until: String(W5H_RESET_AT) },
    stopped: { ...base, reason: "banned" },
    weekly: { ...base, deals: "5" },
  };
  const name =
    event === "confirm_item"
      ? { tr: "Ejder Kılıcı", en: "Ejder Kılıcı" }
      : event === "handoff"
        ? { tr: "Yakut", en: "Yakut" }
        : event === "approval" || event === "deal_done"
          ? { tr: "Elitra", en: "Elytra" }
          : undefined;
  const approval = event === "approval" || event === "handoff" || event === "confirm_item";
  noticeSeq += 1;
  return {
    id: `01J${at.toString(36).toUpperCase()}${String(noticeSeq).padStart(13, "0")}`,
    event: approval ? "approval" : event,
    kind: approval ? "afk_deal" : "afk_info",
    priority: approval ? "high" : "normal",
    at,
    tag: approval ? `approval:${data[event]?.approval ?? "x"}` : undefined,
    quiet: false,
    data: data[event] ?? base,
    itemName: name,
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
