// AFK+ for browser previews (scenarios in mock-data.ts): the backend's answers, kept in memory, and
// the live stream while the panel is open. Nothing runs while no panel watches. "Kontrol et" reads a
// few words the way the agent would (enough for the card's states): "elytra", "netherit" (which
// one?), "yakut" (a custom item), "ejder", an unknown name (a custom item question), "bakım" (the
// AI under maintenance), "bağlı değil" (the machine away), "discord" or "önce" (a rule that does
// not change), "sorma" (a note), and amounts as the players write them.
import { itemDataFromText } from "@pl/ui/items";
import { isTauri } from "@/lib/platform";
import { mockLive } from "../mock-live";
import type { PlusApi } from "./api";
import {
  auditEntry,
  auditScenario,
  itemDataFails,
  MOCK_ITEM_DATA_TEXT,
  mockAi,
  mockApprovals,
  mockAudit,
  mockConfig,
  mockHistory,
  mockLines,
  mockNotice,
  mockNotifyPrefs,
  mockPickOf,
  mockQuota,
  mockReceipts,
  mockReport,
  mockSettings,
  mockSummary,
  mockThreads,
  noticeScenario,
  nowSecs,
  PLUS_ID,
  plusScenario,
} from "./mock-data";
import { type CheckAnswer, fill, type OrderDraft } from "./order-draft";
import { checkOrderText } from "./order-rules";
import type {
  CloudNotifyPrefs,
  PlusApproval,
  PlusAuditEntry,
  PlusCheckOutcome,
  PlusConfig,
  PlusInboxLine,
  PlusOrder,
} from "./types";
import { parseAmount } from "./words";

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

let config: PlusConfig = mockConfig();
let aiRev = plusScenario === "setup" ? 0 : 14;
let approvals: PlusApproval[] = mockApprovals();
let audit: PlusAuditEntry[] = mockAudit();
let prefs: CloudNotifyPrefs = mockNotifyPrefs();
let checksLeft = plusScenario === "setup" ? 20 : 4;
let orderSeq = 100;
const threads = mockThreads();
const lines = new Map<string, PlusInboxLine[]>();
let ai = mockAi();
const offline = plusScenario === "offline";
const itemData = itemDataFromText(MOCK_ITEM_DATA_TEXT);

function machineAway(): Error {
  return Object.assign(new Error("AFK sunucusu şu an bağlı değil."), {
    code: "BULUT-SUNUCU",
    details: "mock host_offline",
  });
}

function settings() {
  return mockSettings(config, aiRev);
}

function emitAi() {
  ai = {
    ...ai,
    pendingApprovals: approvals.filter((a) => a.state === "pending").length,
    state: config.enabled ? ai.state : "off",
    // Off and never started: the service says it waits for the first "Başlat".
    reason: config.enabled ? ai.reason : config.ready ? "ready" : "player",
  };
  mockLive.emit({ kind: "ai", acct: PLUS_ID, ai });
}

/** A step for the log, shown at once while the panel watches. */
function log(k: string, parts: Partial<PlusAuditEntry> = {}) {
  const entry = auditEntry(nowSecs(), k, parts);
  audit = [entry, ...audit];
  mockLive.emit({ kind: "audit", acct: PLUS_ID, items: [entry] });
}

/** "Kontrol et": the card's draft, the way the agent reads the words (a preview's few). */
function check(text: string, inGame = false): PlusCheckOutcome {
  const lower = text.toLocaleLowerCase("tr");
  if (/bakım/.test(lower)) return { kind: "unavailable", reason: "paused" };
  if (/bağlı değil/.test(lower)) return { kind: "offline" };
  if (/discord|instagram|telegram|whatsapp/.test(lower)) {
    return { kind: "rejected", reason: "loosen", rule: "off_platform" };
  }
  if (/önce (sen|ben)|önden/.test(lower))
    return { kind: "rejected", reason: "loosen", rule: "prepay" };
  // The rules' money (28 Oct 2026): a rail refuses; money that may be the game's is asked first
  // ("Bu oyun parası"), unless the player said it is.
  const label = settings().server.money.label;
  const ruled = checkOrderText(text, { currency: label?.tr ?? null, inGame }).result;
  if (ruled === "reject:loosen:rmt") return { kind: "rejected", reason: "loosen", rule: "rmt" };
  if (ruled === "question:rmt") return { kind: "rejected", reason: "question", rule: "rmt" };
  if (checksLeft <= 0) return { kind: "limited", reason: "daily", retry_after: null };
  checksLeft -= 1;
  // "64 tane" is a quantity, never a price.
  const priced = lower.replace(/\d+\s*(tane|adet)/g, " ");
  const amounts = [...priced.matchAll(/\d+(?:[.,]\d+)?\s?(?:k|bin|m|milyon)?/g)]
    .map((match) => parseAmount(match[0]))
    .filter((n): n is number => n !== null && n > 0);
  const selling = /sat(ar|ıyorum|ılık|mak|arım|ış)/.test(lower);
  const buying = /al(ır|ıyorum|mak|arım|acağım)|arıyorum|lazım/.test(lower);
  const draft: OrderDraft = {
    side: selling ? "sell" : buying ? "buy" : null,
    item: null,
    qty: null,
    pay: "money",
    unit: null,
    target: null,
    limit: null,
    style: "balanced",
    req: { ench: {}, dur: null, named: false, cursed: false },
    trade: "auto",
    until: nowSecs() + 14 * 86_400,
    note: text.trim(),
    src: "text",
    amounts: "money",
    open: {},
    ok: [],
  };
  if (/elytra|elitra/.test(lower)) {
    draft.item = "minecraft:elytra";
  } else if (/netherit/.test(lower)) {
    draft.open.item = {
      options: [
        "minecraft:netherite_ingot",
        "minecraft:netherite_sword",
        "minecraft:netherite_pickaxe",
      ],
    };
  } else if (/yakut/.test(lower)) {
    draft.item = "custom:c1";
  } else if (/ejder/.test(lower)) {
    draft.item = "custom:c2";
  } else if (/elmas/.test(lower)) {
    draft.item = "minecraft:diamond";
  } else {
    // The words before the verb, at most three: "Gece Kalkanı alırım" → "Gece Kalkanı".
    const head = text.trim().split(/\s+(?:al|sat|ar)\S*|,/)[0] ?? text.trim();
    const words = head.split(/\s+/).slice(0, 3).join(" ");
    draft.open.item = { mention: words.slice(0, 32) };
  }
  if (amounts.length) draft.limit = selling ? Math.min(...amounts) : Math.max(...amounts);
  const qty = /(\d+)\s*(tane|adet)/.exec(lower)?.[1];
  if (qty) draft.qty = Number(qty);
  if (selling && draft.pay === "money") draft.req = null;
  const notes: string[] = [];
  if (/sorma|onaysız/.test(lower)) notes.push("approval_pref");
  if (/'(dan|den|tan|ten) al/.test(lower)) notes.push("named_player");
  const filled = fill(draft, {
    data: itemData,
    custom: config.customItems,
    money: { maxDeal: settings().server.money.maxDeal, round: null },
    pay: config.pay === "items" || config.pay === "both" ? config.pay : "money",
    features: { itemPay: true, customItems: true },
    limits: { perDealMax: config.limits.perDealMax, totalBudget: config.limits.totalBudget },
    trades: settings().server.trade === "owner" ? "owner" : "bot",
  });
  const answer: CheckAnswer = { draft: filled, questions: [], notes, left: checksLeft };
  return { kind: "draft", answer: JSON.stringify(answer) };
}

/** Conditions as the service stores them: every field there. */
function storedReq(
  req:
    | { ench?: Partial<Record<string, number>>; dur?: number; named?: boolean; cursed?: boolean }
    | null
    | undefined,
): PlusOrder["req"] {
  if (!req) return null;
  const ench: Record<string, number> = {};
  for (const [id, level] of Object.entries(req.ench ?? {})) {
    if (typeof level === "number") ench[id] = level;
  }
  return { ench, dur: req.dur ?? 0, named: req.named ?? false, cursed: req.cursed ?? false };
}

const isActive = (order: PlusOrder) => {
  const view = settings().orders[order.id];
  return order.on && (view?.got ?? 0) < order.qty && order.until > nowSecs();
};

let chatter: number | undefined;
const CHATTER: Array<[string, string, string]> = [
  ["pm_in", "Pixel_TR", "totem var mı satan"],
  ["reply", "Pixel_TR", "Ben alıyorum, satmıyorum. Elinde varsa 8.000 olur mu?"],
  ["pm_in", "Kirpi_Gamer", "elmas tanesi kaç"],
  ["reply", "Kirpi_Gamer", "Tanesi 120, 64'lük paket var."],
];

export const mockPlusApi: PlusApi = {
  async panel(id) {
    await delay(220);
    if (id !== PLUS_ID) throw new Error("plus_not_active");
    return {
      settings: settings(),
      quota: offline ? null : mockQuota(),
      approvals: offline ? [] : approvals,
      machineAway: offline,
    };
  },
  async settings() {
    await delay(160);
    return settings();
  },
  async itemData() {
    await delay(120);
    return itemDataFails ? null : { minecraft: "26.3", text: MOCK_ITEM_DATA_TEXT };
  },
  async save(_id, rev, next) {
    await delay(380);
    if (rev !== aiRev) return { kind: "conflict", settings: settings() };
    // The money's name as the service keeps it: a write without it keeps the stored one, an
    // emptied one is none.
    const sent = "currency" in next.trade ? next.trade.currency : config.trade.currency;
    const currency = sent?.trim().replace(/\s+/gu, " ") || null;
    // "Sipariş menüsü" as the service keeps it: only while on (off is none, here null); a write
    // without it keeps the stored one.
    const asked = next.discovery.orderMenu as boolean | null | undefined;
    const menu = asked == null ? config.discovery.orderMenu === true : asked;
    // Orders and custom items have their own routes: the settings' save keeps the stored ones. The
    // first-start mark is the service's: a save that turns the assistant on ends it.
    config = {
      ...next,
      ready: next.enabled ? null : config.ready,
      discovery: { ...next.discovery, orderMenu: menu ? true : null },
      trade: { ...next.trade, currency },
      orders: config.orders,
      customItems: config.customItems,
    };
    aiRev += 1;
    emitAi();
    return { kind: "saved", settings: settings() };
  },
  async restore(_id, rev, current) {
    await delay(380);
    if (current !== aiRev) return { kind: "conflict", settings: settings() };
    const entry = mockHistory(config, aiRev).find((e) => e.aiRev === rev);
    if (entry) config = entry.config;
    aiRev += 1;
    return { kind: "saved", settings: settings() };
  },
  async control(_id, action, name) {
    await delay(260);
    if (action === "pause" || action === "resume") {
      // "Başlat" also ends the wait for the first start (the service's `ready`).
      const ready = action === "resume" ? null : config.ready;
      config = { ...config, enabled: action === "resume", ready };
      ai = {
        ...ai,
        state: action === "resume" ? "on" : "off",
        reason: action === "resume" ? null : "player",
      };
      aiRev += 1;
      log(action === "resume" ? "resumed" : "paused", action === "pause" ? { r: "owner" } : {});
    }
    if ((action === "block" || action === "unblock") && name) {
      const rest = config.blocked.filter((n) => n.toLowerCase() !== name.toLowerCase());
      config = { ...config, blocked: action === "block" ? [...rest, name] : rest };
      aiRev += 1;
    }
    emitAi();
    return { ai, aiRev };
  },
  async check(_id, text, inGame) {
    await delay(900);
    if (offline) return { kind: "offline" };
    if (plusScenario === "maintenance") return { kind: "unavailable", reason: "paused" };
    return check(text, inGame === true);
  },
  async createOrder(_id, order, customItems, chip) {
    await delay(420);
    if (config.orders.filter(isActive).length >= settings().rules.orderMax) {
      return {
        kind: "invalid",
        issues: [{ field: "orders", reason: "limit_orders", problem: "active" }],
      };
    }
    const now = nowSecs();
    orderSeq += 1;
    const stored: PlusOrder = {
      ...order,
      id: order.id ?? `o${orderSeq}`,
      at: order.at ?? now,
      req: storedReq(order.req),
    };
    config = {
      ...config,
      pay: chip.pay ?? config.pay,
      limits: { ...config.limits, perDealMax: chip.perDealMax ?? config.limits.perDealMax },
      orders: [stored, ...config.orders.filter((o) => o.id !== stored.id)],
      customItems: [
        ...config.customItems,
        ...customItems.map((item) => ({
          ...item,
          look: item.look ? { cmd: item.look.cmd ?? null, model: item.look.model ?? null } : null,
          at: now,
        })),
      ],
    };
    aiRev += 1;
    return { kind: "saved", settings: settings(), order: stored, deferred: false };
  },
  async patchOrder(_id, orderId, patch) {
    await delay(300);
    const found = config.orders.find((o) => o.id === orderId);
    if (!found) return { kind: "gone" };
    const next: PlusOrder = {
      ...found,
      on: patch.on ?? found.on,
      qty: patch.qty ?? found.qty,
      req: patch.noReq ? null : patch.req ? storedReq(patch.req) : found.req,
      pay: patch.pay ?? found.pay,
      unit: patch.unit ?? found.unit,
      target: patch.target ?? found.target,
      limit: patch.limit ?? found.limit,
      style: patch.style ?? found.style,
      trade: patch.trade ?? found.trade,
      until: patch.extend
        ? Math.max(found.until, nowSecs()) + settings().rules.orderDaysDefault * 86_400
        : (patch.until ?? found.until),
    };
    config = { ...config, orders: config.orders.map((o) => (o.id === orderId ? next : o)) };
    aiRev += 1;
    return { kind: "saved", settings: settings(), order: next, deferred: false };
  },
  async deleteOrder(_id, orderId) {
    await delay(260);
    const found = config.orders.find((o) => o.id === orderId);
    if (!found) return { kind: "gone" };
    // An order whose trade is in its window waits for the trade (README "Deleting").
    const trading = settings().orders[orderId]?.live === "trading";
    if (!trading) config = { ...config, orders: config.orders.filter((o) => o.id !== orderId) };
    aiRev += 1;
    return { kind: "saved", settings: settings(), order: null, deferred: trading };
  },
  async pick(_id, target) {
    await delay(500);
    if (offline) return { kind: "offline" };
    const seen = mockPickOf(target.slot);
    if (seen.kind === "failed") return { kind: "failed", reason: seen.reason };
    if (seen.kind === "custom") {
      return {
        kind: "picked",
        pick: {
          ref: null,
          req: null,
          custom: {
            name: seen.name,
            base: seen.base,
            fp: seen.fp,
            look: seen.cmd !== null ? { cmd: seen.cmd, model: null } : null,
          },
        },
      };
    }
    return {
      kind: "picked",
      pick: {
        ref: seen.ref,
        req: { ench: seen.ench, dur: seen.dur, named: false, cursed: false },
        custom: null,
      },
    };
  },
  async audit(_id, before, after) {
    await delay(260);
    if (offline) return { kind: "kept", items: audit.slice(4) };
    if (auditScenario === "off") return { kind: "off" };
    if (after !== null)
      return { kind: "page", items: audit.filter((e) => e.s > after), more: false, from: null };
    const older = before === null ? audit : audit.filter((e) => e.s < before);
    return { kind: "page", items: older.slice(0, 14), more: older.length > 14, from: null };
  },
  async auditKept() {
    await delay(60);
    // The computer kept what it saw before (the offline preview shows them).
    return offline ? audit.slice(4) : [];
  },
  async auditKeep() {},
  async auditPrune() {},
  async receipts() {
    await delay(240);
    return { receipts: mockReceipts(), next: null };
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
    if (found.kind === "handoff") {
      log(approve ? "handoff_done" : "handoff_no", {
        w: found.counterpart,
        o: found.order ?? undefined,
      });
    } else if (found.kind === "confirm_item") {
      log(approve ? "owner_ok" : "owner_no", { w: found.counterpart, o: found.order ?? undefined });
    } else {
      log(approve ? "owner_ok" : "owner_no", { w: found.counterpart, o: found.order ?? undefined });
    }
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
      canReply: name !== "Buzul_12" && thread?.state !== "blocked",
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
    log("owner_msg", { w: name, x: text });
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
  async history() {
    await delay(260);
    return { entries: mockHistory(config, aiRev) };
  },
  async summary() {
    await delay(260);
    return mockSummary();
  },
  async report() {
    await delay(260);
    return mockReport();
  },
  async exportTable(_id, name) {
    await delay(500);
    return { fileName: `afkplus-${name}-2026-10-26.csv` };
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
    // The machine's socket, as in the app: it says whether the machine is there.
    mockLive.emit({ kind: "status", host: "kz1", status: "live" });
    mockLive.emit({ kind: "hello", host: "kz1", online: mockLive.machineOnline() });
    if (id !== PLUS_ID || offline) return;
    setTimeout(() => mockLive.emit({ kind: "ai", acct: id, ai }), 240);
    let n = 0;
    window.clearInterval(chatter);
    chatter = window.setInterval(() => {
      const entry = CHATTER[n++ % CHATTER.length];
      if (!entry || !config.enabled) return;
      const [k, w, x] = entry;
      log(k, { w, x });
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
