// AFK+ for browser previews (scenarios in mock-data.ts): the backend's answers, kept in memory, and
// the live stream while the panel is open. Nothing runs while no panel watches.
import { isTauri } from "@/lib/platform";
import { mockLive } from "../mock-live";
import type { PlusApi } from "./api";
import {
  CATALOG,
  mockAi,
  mockApprovals,
  mockConfig,
  mockDeals,
  mockFeed,
  mockHistory,
  mockLines,
  mockNotice,
  mockNotifyPrefs,
  mockQuota,
  mockReport,
  mockSettings,
  mockThreads,
  noticeScenario,
  nowSecs,
  PLUS_ID,
  plusScenario,
  ulid,
} from "./mock-data";
import type {
  CloudNotifyPrefs,
  PlusApproval,
  PlusConfig,
  PlusFeedItem,
  PlusInboxLine,
  PlusParseOutcome,
} from "./types";
import { parseAmount } from "./words";

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

let config: PlusConfig = mockConfig();
let aiRev = plusScenario === "setup" ? 0 : 14;
let approvals: PlusApproval[] = mockApprovals();
let feed: PlusFeedItem[] = mockFeed();
let prefs: CloudNotifyPrefs = mockNotifyPrefs();
const threads = mockThreads();
const lines = new Map<string, PlusInboxLine[]>();
let ai = mockAi();
const offline = plusScenario === "offline";

function machineAway(): Error {
  return Object.assign(new Error("AFK sunucusu şu an bağlı değil."), {
    code: "BULUT-SUNUCU",
    details: "mock host_offline",
  });
}

function emitAi() {
  ai = {
    ...ai,
    pendingApprovals: approvals.filter((a) => a.state === "pending").length,
    state: config.enabled ? ai.state : "off",
    reason: config.enabled ? ai.reason : "player",
  };
  mockLive.emit({ kind: "ai", acct: PLUS_ID, ai });
}

const CHATTER: Array<[string, string, string]> = [
  ["them", "Baris_TR", "totem var mı satan"],
  ["bot", "Baris_TR", "[AFK+] Ben alıyorum, satmıyorum. Elinde varsa 8.000 olur mu?"],
  ["them", "Ece_Gamer", "elmas tanesi kaç"],
  ["bot", "Ece_Gamer", "[AFK+] Tanesi 120, 64'lük paket var."],
];

let chatter: number | undefined;

export const mockPlusApi: PlusApi = {
  async panel(id) {
    await delay(220);
    if (id !== PLUS_ID) throw new Error("plus_not_active");
    return {
      settings: mockSettings(config, aiRev),
      catalog: CATALOG,
      quota: offline ? null : mockQuota(),
      approvals: offline ? [] : approvals,
      machineAway: offline,
    };
  },
  async settings() {
    await delay(160);
    return mockSettings(config, aiRev);
  },
  async catalog() {
    await delay(80);
    return CATALOG;
  },
  async save(_id, rev, next) {
    await delay(380);
    if (rev !== aiRev) return { kind: "conflict", settings: mockSettings(config, aiRev) };
    config = next;
    aiRev += 1;
    emitAi();
    return { kind: "saved", settings: mockSettings(config, aiRev) };
  },
  async restore(_id, rev, current) {
    await delay(380);
    if (current !== aiRev) return { kind: "conflict", settings: mockSettings(config, aiRev) };
    const entry = mockHistory(config, aiRev).find((e) => e.aiRev === rev);
    if (entry) config = entry.config;
    aiRev += 1;
    return { kind: "saved", settings: mockSettings(config, aiRev) };
  },
  async control(_id, action, name) {
    await delay(260);
    if (action === "pause" || action === "resume") {
      config = { ...config, enabled: action === "resume" };
      ai = {
        ...ai,
        state: action === "resume" ? "on" : "off",
        reason: action === "resume" ? null : "player",
      };
      aiRev += 1;
    }
    if ((action === "block" || action === "unblock") && name) {
      const rest = config.blocked.filter((n) => n.toLowerCase() !== name.toLowerCase());
      config = { ...config, blocked: action === "block" ? [...rest, name] : rest };
      aiRev += 1;
    }
    emitAi();
    return { ai, aiRev };
  },
  async feed(_id, after) {
    await delay(260);
    if (offline) throw machineAway();
    if (after) return { items: [], next: null };
    return { items: feed, next: "older" };
  },
  async deals() {
    await delay(240);
    return { deals: mockDeals(), next: null };
  },
  async approvals() {
    await delay(160);
    return approvals;
  },
  async answer(_id, approvalId, approve, maxPrice) {
    await delay(320);
    const found = approvals.find((a) => a.id === approvalId);
    if (found?.state !== "pending") return { kind: "closed", state: found?.state ?? "closed" };
    const answered: PlusApproval = {
      ...found,
      state: approve ? "approved" : "rejected",
      maxPrice: approve ? maxPrice : null,
    };
    approvals = approvals.map((a) => (a.id === approvalId ? answered : a));
    const at = nowSecs();
    feed = [
      {
        id: ulid(at),
        at,
        kind: "approval_answer",
        counterpart: found.counterpart,
        text: null,
        item: found.deal.item,
        qty: found.deal.qty,
        price: maxPrice ?? found.deal.price,
        currency: found.deal.currency,
        reason: approve ? (maxPrice ? "lowered" : "approved") : "rejected",
      },
      ...feed,
    ];
    mockLive.emit({ kind: "approval", acct: PLUS_ID, approval: answered });
    emitAi();
    return { kind: "done", approval: answered };
  },
  async quota() {
    await delay(160);
    if (offline) throw machineAway();
    return mockQuota();
  },
  async inbox() {
    await delay(240);
    if (offline) throw machineAway();
    return { threads, next: null };
  },
  async thread(_id, name) {
    await delay(200);
    if (offline) throw machineAway();
    const thread = threads.find((t) => t.name === name);
    const own = lines.get(name) ?? mockLines(name);
    lines.set(name, own);
    return {
      lines: own,
      handoffUntil: thread?.handoffUntil ?? null,
      canReply: name !== "Zeynep_12" && thread?.state !== "blocked",
      next: null,
    };
  },
  async reply(_id, name, text) {
    await delay(420);
    if (/https?:\/\/|discord\.gg/i.test(text)) return { kind: "rejected", reason: "link" };
    if ((text.match(/\d/g) ?? []).length >= 10) return { kind: "rejected", reason: "phone" };
    const line: PlusInboxLine = { at: nowSecs(), from: "you", text, hidden: null };
    lines.set(name, [...(lines.get(name) ?? mockLines(name)), line]);
    const thread = threads.find((t) => t.name === name);
    if (thread) {
      thread.last = { from: "you", text };
      thread.lastAt = line.at;
      thread.state = "handoff";
      thread.handoffUntil = line.at + 30 * 60;
    }
    return { kind: "sent", line };
  },
  async read(_id, name) {
    const thread = threads.find((t) => t.name === name);
    if (thread) thread.unread = 0;
  },
  async handBack(_id, name) {
    await delay(200);
    const thread = threads.find((t) => t.name === name);
    if (thread) {
      thread.state = "active";
      thread.handoffUntil = null;
    }
  },
  async parse(_id, text): Promise<PlusParseOutcome> {
    await delay(900);
    const lower = text.toLocaleLowerCase("tr");
    const amounts = [...lower.matchAll(/\d+(?:[.,]\d+)?\s?(?:k|bin|m|milyon)?/g)]
      .map((match) => parseAmount(match[0]))
      .filter((n): n is number => n !== null && n > 0);
    const sell = /sat(ar|ıyorum|ılık|mak)|s>/.test(lower);
    if (/elytra|elitra|kanat/.test(lower)) {
      if (!amounts.length) return { kind: "failed", reason: "no_amount" };
      const limit = Math.max(...amounts);
      return {
        kind: "draft",
        draft: {
          side: sell ? "sell" : "buy",
          item: "minecraft:elytra",
          candidates: [],
          qty: 1,
          unit: "total",
          target: sell ? limit : Math.round((limit * 0.8) / 500) * 500,
          max: sell ? null : limit,
          min: sell ? Math.round((limit * 0.9) / 1000) * 1000 : null,
          style: "balanced",
          approval: "above",
        },
        issues: [],
      };
    }
    if (/netherit/.test(lower)) {
      return {
        kind: "draft",
        draft: {
          side: sell ? "sell" : "buy",
          item: null,
          candidates: [
            "minecraft:netherite_ingot",
            "minecraft:netherite_sword",
            "minecraft:netherite_pickaxe",
          ],
          qty: 1,
          unit: "each",
          target: amounts[0] ?? null,
          max: sell ? null : (amounts[0] ?? null),
          min: null,
          style: "balanced",
          approval: "above",
        },
        issues: [],
      };
    }
    return { kind: "failed", reason: "no_item" };
  },
  async history() {
    await delay(260);
    return { entries: mockHistory(config, aiRev) };
  },
  async summary() {
    await delay(260);
    const now = nowSecs();
    return {
      from: now - 30 * 86_400,
      to: now,
      items: [
        {
          side: "sell",
          item: "minecraft:diamond",
          currency: "smp",
          deals: 3,
          qty: 160,
          total: 18_720,
          avgUnit: 117,
          minUnit: 112,
          maxUnit: 120,
        },
        {
          side: "buy",
          item: "minecraft:elytra",
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
          item: "minecraft:totem_of_undying",
          currency: "smp",
          deals: 1,
          qty: 2,
          total: 16_400,
          avgUnit: 8_200,
          minUnit: 8_200,
          maxUnit: 8_200,
        },
      ],
      totals: [{ currency: "smp", buys: 2, sells: 3, spent: 52_400, earned: 18_720 }],
    };
  },
  async report() {
    await delay(260);
    return mockReport();
  },
  async exportTable(_id, name) {
    await delay(500);
    return { fileName: `afkplus-${name}-2026-10-24.csv` };
  },
  async showExport(fileName) {
    console.info("[afk+ mock] show", fileName);
  },
  async notifyPrefs() {
    await delay(180);
    return prefs;
  },
  async setNotifyPrefs(next) {
    await delay(200);
    prefs = { events: { ...prefs.events, ...next.events }, quiet: next.quiet };
    return prefs;
  },
  async noticeSeen(id) {
    console.info("[afk+ mock] seen", id);
  },
  async watch(id) {
    await delay(120);
    if (id !== PLUS_ID || offline) return;
    setTimeout(() => {
      mockLive.emit({ kind: "ai", acct: id, ai });
      mockLive.emit({ kind: "feed", acct: id, items: feed.slice(0, 20), snapshot: true });
    }, 240);
    let n = 0;
    window.clearInterval(chatter);
    chatter = window.setInterval(() => {
      const entry = CHATTER[n++ % CHATTER.length];
      if (!entry || !config.enabled) return;
      const [from, name, text] = entry;
      const at = nowSecs();
      const item: PlusFeedItem = {
        id: ulid(at),
        at,
        kind: from === "them" ? "pm_in" : "pm_out",
        counterpart: name,
        text,
        item: null,
        qty: null,
        price: null,
        currency: null,
        reason: null,
      };
      feed = [item, ...feed];
      mockLive.emit({ kind: "feed", acct: id, items: [item], snapshot: false });
    }, 9_000);
  },
  async unwatch() {
    window.clearInterval(chatter);
    chatter = undefined;
  },
};

// Preview: `&notice=<event>` shows that notice as the launcher's toast once the page settled (the
// Windows notification itself has no preview). Never in the app.
if (typeof window !== "undefined" && !isTauri() && noticeScenario) {
  const event = noticeScenario;
  window.setTimeout(() => {
    void import("./notices").then(({ showAfkNoticeToast }) =>
      showAfkNoticeToast(mockNotice(event)),
    );
  }, 1200);
}
