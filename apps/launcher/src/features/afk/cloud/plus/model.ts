// AFK+ settings on the launcher: the contract's defaults, the checks made before a `PUT /plus`
// (README "AfkPlusConfig v1" → Rules, in the Worker's words: `buy[2].max` + `order`; the Worker and
// the agent have the last word), new row ids, rows from a typed rule's draft (QoL Q2), the AI
// state's sentence, and what changed between two saved versions (QoL Q7). The same rules as
// afk.projects.gg's, so the three surfaces agree.
import type {
  PlusAi,
  PlusBuyRow,
  PlusCatalog,
  PlusConfig,
  PlusLimits,
  PlusParseDraft,
  PlusQuota,
  PlusSellRow,
  PlusWindow,
} from "./types";

export type { PlusAi, PlusBuyRow, PlusCatalog, PlusConfig, PlusLimits, PlusSellRow, PlusWindow };

/** The contract's limits until the catalog says its profile's own. */
export const PLUS_LIMITS = {
  listMax: 10,
  maxDeal: 10_000_000,
  qtyMax: 9999,
  blockedMax: 100,
  windowsMax: 7,
  introMax: 60,
  statusMax: 80,
  replyMax: 200,
  parseMax: 200,
  /** "max > 10 × target": confirmed once (a zero too many). */
  wideFactor: 10,
  /** A break reaches at most 90 days ahead. */
  breakDays: [1, 3, 7, 14, 30] as const,
} as const;

export type Side = "buy" | "sell";
export type Style = "patient" | "balanced" | "quick";
export type ApprovalRule = "auto" | "above" | "always";
export type Scope = "all" | "trade" | "off";
export type Preset = "friendly" | "brief" | "polite" | "playful";
export type StatusPreset = "back_soon" | "back_evening" | "back_tomorrow" | "weekend" | "busy";

export const STYLES: Style[] = ["patient", "balanced", "quick"];
export const APPROVALS: ApprovalRule[] = ["auto", "above", "always"];
export const PRESETS: Preset[] = ["friendly", "brief", "polite", "playful"];
export const STATUS_PRESETS: StatusPreset[] = [
  "back_soon",
  "back_evening",
  "back_tomorrow",
  "weekend",
  "busy",
];

export function maxDealOf(catalog: PlusCatalog | null | undefined): number {
  return catalog?.money.maxDeal ?? PLUS_LIMITS.maxDeal;
}

export function listMaxOf(catalog: PlusCatalog | null | undefined): number {
  return Math.min(PLUS_LIMITS.listMax, catalog?.limits.listMax ?? PLUS_LIMITS.listMax);
}

/** Whether the bot may spend at all (without it buy rows only report what they find). */
export function canSpend(limits: PlusLimits): boolean {
  return limits.totalBudget !== null && limits.perDealMax !== null;
}

/** A new row id: ^[a-z0-9]{1,8}$, not taken in either list. */
export function newRowId(
  config: Pick<PlusConfig, "buy" | "sell">,
  random: () => number = Math.random,
): string {
  const taken = new Set([...config.buy, ...config.sell].map((row) => row.id));
  for (;;) {
    let id = "";
    for (let i = 0; i < 8; i++)
      id += "abcdefghijklmnopqrstuvwxyz0123456789"[Math.floor(random() * 36)];
    if (!taken.has(id)) return id;
  }
}

export function newBuyRow(id: string, item: string, parts: Partial<PlusBuyRow> = {}): PlusBuyRow {
  return {
    id,
    item,
    req: { ench: {}, minDurabilityPct: 90, allowCustomName: false, allowCurses: false },
    qty: 1,
    unit: "each",
    target: 1,
    max: 1,
    style: "balanced",
    approval: "above",
    dailyQty: 1,
    notify: true,
    enabled: true,
    ...parts,
  };
}

export function newSellRow(
  id: string,
  item: string,
  parts: Partial<PlusSellRow> = {},
): PlusSellRow {
  return {
    id,
    item,
    qty: 1,
    unit: "each",
    target: 1,
    min: 1,
    style: "balanced",
    approval: "above",
    dailyQty: 1,
    enabled: true,
    ...parts,
  };
}

/** A typed rule's draft (QoL Q2) as a row, once its item is known. */
export function rowFromDraft(
  draft: PlusParseDraft,
  item: string,
  id: string,
): { side: "buy"; row: PlusBuyRow } | { side: "sell"; row: PlusSellRow } {
  const qty = Math.max(1, Math.min(PLUS_LIMITS.qtyMax, draft.qty || 1));
  const style = (STYLES as string[]).includes(draft.style) ? draft.style : "balanced";
  const approval = (APPROVALS as string[]).includes(draft.approval) ? draft.approval : "above";
  const unit = draft.unit === "total" ? "total" : "each";
  if (draft.side === "sell") {
    const target = draft.target ?? draft.min ?? 1;
    return {
      side: "sell",
      row: newSellRow(id, item, {
        qty,
        unit,
        target,
        min: Math.min(draft.min ?? target, target),
        style,
        approval,
        dailyQty: qty,
      }),
    };
  }
  const max = draft.max ?? draft.target ?? 1;
  return {
    side: "buy",
    row: newBuyRow(id, item, {
      qty,
      unit,
      target: Math.min(draft.target ?? max, max),
      max,
      style,
      approval,
      dailyQty: qty,
    }),
  };
}

// --- List and settings changes ---------------------------------------------------------------------
// Each change gives the same result when it runs twice: it runs on the shown copy at once and again
// on what is sent (use-plus.ts `usePlusInstantSave`).

type RuleRow = { side: "buy"; row: PlusBuyRow } | { side: "sell"; row: PlusSellRow };

/** Adds the rule, or puts it in place of the one with its id (at `index` when it is new). */
export function putRule(config: PlusConfig, rule: RuleRow, index?: number): PlusConfig {
  if (rule.side === "buy") {
    const at = config.buy.findIndex((row) => row.id === rule.row.id);
    const buy = config.buy.filter((row) => row.id !== rule.row.id);
    buy.splice(at >= 0 ? at : (index ?? buy.length), 0, rule.row);
    return { ...config, buy };
  }
  const at = config.sell.findIndex((row) => row.id === rule.row.id);
  const sell = config.sell.filter((row) => row.id !== rule.row.id);
  sell.splice(at >= 0 ? at : (index ?? sell.length), 0, rule.row);
  return { ...config, sell };
}

export function removeRule(config: PlusConfig, side: Side, id: string): PlusConfig {
  return side === "buy"
    ? { ...config, buy: config.buy.filter((row) => row.id !== id) }
    : { ...config, sell: config.sell.filter((row) => row.id !== id) };
}

export function setRuleEnabled(
  config: PlusConfig,
  side: Side,
  id: string,
  enabled: boolean,
): PlusConfig {
  return side === "buy"
    ? { ...config, buy: config.buy.map((row) => (row.id === id ? { ...row, enabled } : row)) }
    : { ...config, sell: config.sell.map((row) => (row.id === id ? { ...row, enabled } : row)) };
}

/** What "Ayarlar" edits as one draft; the lists, the switch and a break are saved at once. */
export type SettingsPart = Pick<
  PlusConfig,
  "lang" | "persona" | "replies" | "limits" | "discovery" | "trade" | "summaryAt" | "schedule"
>;

export function settingsPart(config: SettingsPart): SettingsPart {
  return {
    lang: config.lang,
    persona: config.persona,
    replies: config.replies,
    limits: config.limits,
    discovery: config.discovery,
    trade: config.trade,
    summaryAt: config.summaryAt,
    schedule: config.schedule,
  };
}

/** The draft's fields on top of the latest settings (what changed elsewhere meanwhile stays). */
export function withSettings(config: PlusConfig, part: SettingsPart): PlusConfig {
  return { ...config, ...part };
}

/** JSON with object keys in order, so two equal values compare equal however they were built. */
function stable(value: unknown): string {
  return JSON.stringify(value, (_key, inner: unknown) =>
    inner && typeof inner === "object" && !Array.isArray(inner)
      ? Object.fromEntries(
          Object.entries(inner as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : 1)),
        )
      : inner,
  );
}

export function sameSettings(a: SettingsPart, b: SettingsPart): boolean {
  return stable(settingsPart(a)) === stable(settingsPart(b));
}

/** Unsaved "Ayarlar" edits: what was saved when the player last edited, and the edited copy. */
export interface SettingsDraft {
  base: SettingsPart;
  value: SettingsPart;
}

const SETTINGS_KEYS = [
  "lang",
  "persona",
  "replies",
  "limits",
  "discovery",
  "trade",
  "summaryAt",
  "schedule",
] as const;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  Boolean(value) && typeof value === "object" && !Array.isArray(value);

/**
 * The player's own edits on top of `latest`: a field (one level into the groups: `limits.perDealMax`,
 * `persona.intro`) comes from the draft only where the draft differs from its base, so what changed
 * elsewhere meanwhile (the site, the phone) stays.
 */
export function applyDraft(
  latest: SettingsPart,
  draft: SettingsDraft | null | undefined,
): SettingsPart {
  if (!draft) return settingsPart(latest);
  const out: Record<string, unknown> = { ...settingsPart(latest) };
  for (const key of SETTINGS_KEYS) {
    const before = draft.base[key] as unknown;
    const after = draft.value[key] as unknown;
    if (isRecord(before) && isRecord(after)) {
      const merged: Record<string, unknown> = {
        ...(latest[key] as unknown as Record<string, unknown>),
      };
      for (const sub of Object.keys(after)) {
        if (stable(after[sub]) !== stable(before[sub])) merged[sub] = after[sub];
      }
      out[key] = merged;
    } else if (stable(after) !== stable(before)) {
      out[key] = after;
    }
  }
  return out as unknown as SettingsPart;
}

/** The draft after an edit of what is shown; null when it no longer differs from what is saved. */
export function editDraft(
  saved: SettingsPart,
  draft: SettingsDraft | null | undefined,
  update: (shown: SettingsPart) => SettingsPart,
): SettingsDraft | null {
  const value = update(applyDraft(saved, draft));
  return sameSettings(value, saved) ? null : { base: settingsPart(saved), value };
}

// --- Checks ------------------------------------------------------------------------------------------

export type RowProblem = "qty" | "target" | "order" | "maxDeal" | "dailyQty" | "durability";

export function buyRowProblems(
  row: PlusBuyRow,
  maxDeal: number = PLUS_LIMITS.maxDeal,
): RowProblem[] {
  const found: RowProblem[] = [];
  if (!Number.isInteger(row.qty) || row.qty < 1 || row.qty > PLUS_LIMITS.qtyMax) found.push("qty");
  if (!Number.isInteger(row.target) || row.target < 1) found.push("target");
  else if (row.target > row.max) found.push("order");
  if (row.max > maxDeal || row.target > maxDeal) found.push("maxDeal");
  if (!Number.isInteger(row.dailyQty) || row.dailyQty < 1 || row.dailyQty > PLUS_LIMITS.qtyMax)
    found.push("dailyQty");
  if (row.req.minDurabilityPct < 0 || row.req.minDurabilityPct > 100) found.push("durability");
  return found;
}

export function sellRowProblems(
  row: PlusSellRow,
  maxDeal: number = PLUS_LIMITS.maxDeal,
): RowProblem[] {
  const found: RowProblem[] = [];
  if (!Number.isInteger(row.qty) || row.qty < 1 || row.qty > PLUS_LIMITS.qtyMax) found.push("qty");
  if (!Number.isInteger(row.min) || row.min < 1) found.push("target");
  else if (row.min > row.target) found.push("order");
  if (row.target > maxDeal) found.push("maxDeal");
  if (!Number.isInteger(row.dailyQty) || row.dailyQty < 1 || row.dailyQty > PLUS_LIMITS.qtyMax)
    found.push("dailyQty");
  return found;
}

/** "max > 10 × target": the row is saved only after the player confirms it once. */
export function isWide(row: { target: number; max?: number }): boolean {
  return row.max !== undefined && row.max > PLUS_LIMITS.wideFactor * row.target;
}

/**
 * A sale's amounts the trade window can show exactly (AFKPLUS-A2): under 1,000, or whole thousands,
 * or above a million whole millions. The bot rounds anything else, so the editor says so.
 */
export function isProvableSale(amount: number): boolean {
  if (amount < 1_000) return true;
  if (amount > 1_000_000) return amount % 1_000_000 === 0;
  return amount % 1_000 === 0;
}

export type LimitProblem = "approvalAbove" | "maxDeal" | "timeout" | "amount";

export function limitProblems(
  limits: PlusLimits,
  maxDeal: number = PLUS_LIMITS.maxDeal,
): Partial<Record<keyof PlusLimits, LimitProblem>> {
  const found: Partial<Record<keyof PlusLimits, LimitProblem>> = {};
  const amounts: Array<keyof PlusLimits> = [
    "perDealMax",
    "dailySpendMax",
    "totalBudget",
    "balanceFloor",
    "approvalAbove",
    "newCounterpartApprovalAbove",
  ];
  for (const key of amounts) {
    const value = limits[key] as number | null;
    if (value === null) continue;
    const low = key === "balanceFloor" ? 0 : 1;
    if (!Number.isInteger(value) || value < low) found[key] = "amount";
    else if (value > maxDeal) found[key] = "maxDeal";
  }
  if (
    !found.approvalAbove &&
    limits.approvalAbove !== null &&
    limits.perDealMax !== null &&
    limits.approvalAbove > limits.perDealMax
  ) {
    found.approvalAbove = "approvalAbove";
  }
  if (
    !Number.isInteger(limits.approvalTimeoutMin) ||
    limits.approvalTimeoutMin < 1 ||
    limits.approvalTimeoutMin > 60
  ) {
    found.approvalTimeoutMin = "timeout";
  }
  return found;
}

const TIME = /^([01]\d|2[0-3]):[0-5]\d$/;

export function isTime(value: string): boolean {
  return TIME.test(value);
}

/** "0830", "8:30", "8.30" → "08:30" (a 24-hour time typed quickly); null when it is none. */
export function readTime(text: string): string | null {
  const digits = text.replace(/[^\d]/g, "");
  const raw = text.trim();
  let hours: number;
  let minutes: number;
  const parts = /^(\d{1,2})[:.](\d{2})$/.exec(raw);
  if (parts?.[1] && parts[2]) {
    hours = Number(parts[1]);
    minutes = Number(parts[2]);
  } else if (/^\d{3,4}$/.test(digits) && digits === raw) {
    hours = Number(digits.slice(0, digits.length - 2));
    minutes = Number(digits.slice(-2));
  } else if (/^\d{1,2}$/.test(raw)) {
    hours = Number(raw);
    minutes = 0;
  } else {
    return null;
  }
  if (hours > 23 || minutes > 59) return null;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

export type WindowProblem = "days" | "time";

export function windowProblems(window: PlusWindow): WindowProblem[] {
  const found: WindowProblem[] = [];
  const days = new Set(window.days);
  if (
    !window.days.length ||
    days.size !== window.days.length ||
    window.days.some((d) => d < 1 || d > 7)
  )
    found.push("days");
  if (!isTime(window.from) || !isTime(window.to)) found.push("time");
  return found;
}

export type NameProblem = "format" | "duplicate";

export function blockedNameProblem(name: string, list: readonly string[]): NameProblem | null {
  if (!/^[A-Za-z0-9_]{3,16}$/.test(name)) return "format";
  const lower = name.toLowerCase();
  if (list.some((entry) => entry.toLowerCase() === lower)) return "duplicate";
  return null;
}

export type PersonaTextProblem = "length" | "marker" | "link" | "phone" | "email";

/** The launcher's quick look at a persona text; the Worker's text rule and its judge decide. */
export function personaTextProblem(text: string, max: number): PersonaTextProblem | null {
  const value = text.trim();
  if (!value) return null;
  if ([...value].length > max) return "length";
  if (/\[afk\+\]/i.test(value)) return "marker";
  if (/@[a-z0-9-]+\.[a-z]{2,}/i.test(value)) return "email";
  // The one address a persona text may carry is the server's own (README "AFK text form").
  const scanned = value.replace(/\bprojects\.gg\b/gi, "");
  if (/(https?:\/\/|www\.|\b[a-z0-9-]+\.(com|net|org|gg|io|me|tr|xyz)\b)/i.test(scanned))
    return "link";
  if ((value.match(/\d/g) ?? []).length >= 10) return "phone";
  return null;
}

/** Days Monday first (1 … 7). */
export const WEEKDAYS = [1, 2, 3, 4, 5, 6, 7] as const;

/** A window's days in order, without repeats. */
export function sortedDays(days: readonly number[]): number[] {
  return [...new Set(days)].filter((d) => d >= 1 && d <= 7).sort((a, b) => a - b);
}

// --- What changed (QoL Q7) --------------------------------------------------------------------------

export type ConfigChange =
  | { kind: "row_added"; side: Side; item: string }
  | { kind: "row_removed"; side: Side; item: string }
  | {
      kind: "row_amount";
      side: Side;
      item: string;
      field: "target" | "max" | "min" | "qty" | "dailyQty";
      from: number;
      to: number;
    }
  | { kind: "row_toggled"; side: Side; item: string; enabled: boolean }
  | { kind: "row_other"; side: Side; item: string }
  | { kind: "limit"; field: keyof PlusLimits; from: number | null; to: number | null }
  | { kind: "persona" }
  | { kind: "replies" }
  | { kind: "schedule" }
  | { kind: "discovery" }
  | { kind: "blocked" }
  | { kind: "other" };

type AnyRow = PlusBuyRow | PlusSellRow;

function diffRows(
  side: Side,
  before: readonly AnyRow[],
  after: readonly AnyRow[],
  out: ConfigChange[],
) {
  const old = new Map(before.map((row) => [row.id, row]));
  const next = new Map(after.map((row) => [row.id, row]));
  for (const row of after) {
    const was = old.get(row.id);
    if (!was) {
      out.push({ kind: "row_added", side, item: row.item });
      continue;
    }
    let told = false;
    for (const field of ["target", "max", "min", "qty", "dailyQty"] as const) {
      const from = (was as unknown as Record<string, unknown>)[field];
      const to = (row as unknown as Record<string, unknown>)[field];
      if (typeof from === "number" && typeof to === "number" && from !== to) {
        out.push({ kind: "row_amount", side, item: row.item, field, from, to });
        told = true;
      }
    }
    if (was.enabled !== row.enabled) {
      out.push({ kind: "row_toggled", side, item: row.item, enabled: row.enabled });
      told = true;
    }
    if (!told && JSON.stringify(was) !== JSON.stringify(row)) {
      out.push({ kind: "row_other", side, item: row.item });
    }
  }
  for (const row of before) {
    if (!next.has(row.id)) out.push({ kind: "row_removed", side, item: row.item });
  }
}

/** What a saved version changed against the one before it, most telling first. */
export function configChanges(before: PlusConfig, after: PlusConfig): ConfigChange[] {
  const out: ConfigChange[] = [];
  diffRows("buy", before.buy, after.buy, out);
  diffRows("sell", before.sell, after.sell, out);
  for (const field of Object.keys(after.limits) as Array<keyof PlusLimits>) {
    if (before.limits[field] !== after.limits[field]) {
      out.push({
        kind: "limit",
        field,
        from: before.limits[field] as number | null,
        to: after.limits[field] as number | null,
      });
    }
  }
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  if (!same(before.persona, after.persona) || before.lang !== after.lang)
    out.push({ kind: "persona" });
  if (!same(before.replies, after.replies)) out.push({ kind: "replies" });
  if (!same(before.schedule, after.schedule) || before.pauseUntil !== after.pauseUntil)
    out.push({ kind: "schedule" });
  if (!same(before.discovery, after.discovery)) out.push({ kind: "discovery" });
  if (!same(before.blocked, after.blocked)) out.push({ kind: "blocked" });
  if (
    !out.length &&
    (before.summaryAt !== after.summaryAt ||
      !same(before.trade, after.trade) ||
      before.enabled !== after.enabled ||
      before.mode !== after.mode)
  ) {
    out.push({ kind: "other" });
  }
  return out;
}

// --- The AI's state --------------------------------------------------------------------------------

export type AiTone = "ok" | "muted" | "warn" | "error";

export interface AiState {
  /** A key under `afkPlus.ai`. */
  key: string;
  /** For `…Until` keys: the moment, with how to say it. */
  until: { at: number; how: "at" | "until" } | null;
  tone: AiTone;
  /** The player can start it again (their own stop, a safety stop, a server mute). */
  canStart: boolean;
  /** The player can stop it now. */
  canStop: boolean;
}

/**
 * How many of the replies left come from gifts. The ledger's room, and so `estReplies`, counts the
 * windows and the live gifts together (cloud-afk README, "AFK+ quotas"): the gifts' share of the
 * replies is their share of that room.
 */
export function giftReplies(quota: PlusQuota): number {
  const gift = quota.gifts.reduce((sum, g) => sum + Math.max(0, g.remaining), 0);
  if (gift <= 0 || quota.estReplies <= 0) return 0;
  const windows = Math.max(
    0,
    Math.min(quota.w5h.cap - quota.w5h.used, quota.week.cap - quota.week.used),
  );
  return Math.min(quota.estReplies, Math.floor((gift * quota.estReplies) / (windows + gift)));
}

/** The assistant's state as one sentence's key (README "AFK+ tier": never a code). */
export function aiState(ai: PlusAi | null | undefined): AiState {
  const line = (
    key: string,
    tone: AiTone,
    extra: Partial<Omit<AiState, "key" | "tone">> = {},
  ): AiState => ({ key, tone, until: null, canStart: false, canStop: false, ...extra });
  if (!ai || ai.state === "unknown" || !ai.state) return line("unknown", "muted");
  if (ai.state === "on") return line("on", "ok", { canStop: true });
  const paused = ai.state === "paused";
  switch (ai.reason) {
    case "restricted":
      return ai.until
        ? line("restrictedUntil", "error", { until: { at: ai.until, how: "until" } })
        : line("restricted", "error");
    case "staff":
      return line("staff", "error");
    case "service":
      return line(paused ? "servicePaused" : "serviceOff", "muted");
    case "server":
      return line("server", "warn");
    case "player":
      return line("player", "muted", { canStart: true });
    // The pauses that end by themselves can still be stopped for good: the stop is the player's own
    // switch (`config.enabled`), stored and sent on when the machine hears it (README "Control").
    case "break":
      return ai.until
        ? line("breakUntil", "muted", { until: { at: ai.until, how: "until" }, canStop: true })
        : line("breakOn", "muted", { canStop: true });
    case "schedule":
      return ai.until
        ? line("scheduleUntil", "muted", { until: { at: ai.until, how: "at" }, canStop: true })
        : line("schedule", "muted", { canStop: true });
    case "gate":
      return line("gate", "warn", { canStart: true });
    case "muted":
      return line("muted", "warn", { canStart: true });
    case "host":
      return line("host", "muted", { canStop: true });
    case "budget":
      return line("budget", "muted", { canStop: true });
    case "outage":
      return line("outage", "warn", { canStop: true });
    case "quota": {
      const at = ai.until ?? ai.quota?.exhaustedUntil ?? null;
      // Name the window that is full ("5 saatlik sınır doldu") when one is; else the allowance.
      const base =
        (ai.quota?.pct5 ?? 0) >= 100
          ? "quotaW5h"
          : (ai.quota?.pctWeek ?? 0) >= 100
            ? "quotaWeek"
            : "quota";
      return at
        ? line(`${base}Until`, "warn", { until: { at, how: "at" }, canStop: true })
        : line(base, "warn", { canStop: true });
    }
    default:
      return line("paused", "muted");
  }
}

/** What trading can do now, when it is not everything (the bot in the lobby, no spending limit). */
export function tradeNoteKey(ai: PlusAi | null | undefined): string | null {
  if (ai?.state !== "on") return null;
  if (!ai.trade.on) return ai.trade.off === "profile" ? "tradeProfile" : "tradeLobby";
  if (ai.trade.buy === "notify") return "notifyOnly";
  return null;
}
