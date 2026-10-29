// The 7/24 Bulut settings as questions (SETUP-UX-L): each question's answer in one plain sentence,
// the setup's step rules, which question holds a limit, and the ads' write (TRADE-ANY trade-any-4).
// The site's rules and words (afk.projects.gg's model/setup/answers.ts, locales/tr|en-setup.ts), so
// the two surfaces say the same thing about the same settings. Pure: answers.test.ts checks every
// sentence against the settings it describes.
import type { TFunction } from "i18next";
import type { LimitField } from "../plus/model";
import type {
  CloudNotifyPrefs,
  PlusAdPreview,
  PlusConfig,
  PlusDiscovery,
  PlusOrder,
} from "../plus/types";
import { formatAmount, type Lang } from "../plus/words";
import { secretPreview } from "../sensitive";
import type { Command, Draft, Rule } from "../settings-model";

// --- AFK+ ---------------------------------------------------------------------------------------------

/** The setup's steps in their order; the summary asks the same questions but the first. */
export const PLUS_STEPS = ["orders", "money", "act", "chat", "talk"] as const;
export type PlusStep = (typeof PLUS_STEPS)[number];
/** The summary's rows: the steps but the orders (they have their own section), then the rest. */
export type PlusQuestion = Exclude<PlusStep, "orders"> | "who" | "time" | "more" | "notify";

/** Each question's item (vanilla, drawn from the player's own game): one picture per question. */
export const PLUS_ITEMS: Record<PlusStep | "who" | "time" | "more" | "notify", string> = {
  orders: "minecraft:chest",
  money: "minecraft:gold_ingot",
  act: "minecraft:bell",
  chat: "minecraft:goat_horn",
  talk: "minecraft:writable_book",
  who: "minecraft:name_tag",
  time: "minecraft:clock",
  // "Diğer ayarlar", the AFK+ list's and the account's last row but one.
  more: "minecraft:comparator",
  // The launcher's own last row.
  notify: "minecraft:note_block",
};

/** The authority's own items: "ProBot AI ne yapsın?" shows the one chosen. */
export const AUTHORITY_ITEMS = {
  reply: "minecraft:paper",
  ask: "minecraft:bell",
  deal: "minecraft:emerald",
} as const;

export type Authority = keyof typeof AUTHORITY_ITEMS;

/** The authority as the launcher reads it: one it does not know reads as the default, "ask". */
export function authorityOf(value: string): Authority {
  return value === "reply" || value === "deal" ? value : "ask";
}

/** The orders' active buys and sells, as the setup's step rules read them. */
function sides(orders: readonly PlusOrder[]) {
  const on = orders.filter((order) => order.on);
  return { buys: on.filter((o) => o.side === "buy"), sells: on.filter((o) => o.side === "sell") };
}

/** Whether the order is the only active buy: then the total limit is all its own. */
export function soleBuy(orders: readonly PlusOrder[]): boolean {
  return sides(orders).buys.length === 1;
}

/**
 * The money step: asked while there is a buy order; with no order at all it is asked with
 * "Şimdilik alım yapmasın" on (nothing to buy yet); skipped while every order is a sale (selling
 * spends nothing).
 */
export function moneyStep(orders: readonly PlusOrder[]): "ask" | "notNow" | "skip" {
  const { buys, sells } = sides(orders);
  if (buys.length) return "ask";
  return sells.length ? "skip" : "notNow";
}

/** The first buy order paid in money at a price per piece: what the limits are shown against. */
export function fitOrder(orders: readonly PlusOrder[]): PlusOrder | null {
  return (
    sides(orders).buys.find((o) => o.pay === "money" && o.unit === "each" && o.limit > 0) ?? null
  );
}

/**
 * How many of the order's item the limits cover at its highest price, never more than it asks for:
 * one deal and in all (the money step's slots). Null without a price or without both limits.
 */
export function fitCounts(
  limits: { perDealMax: number | null; totalBudget: number | null },
  order: Pick<PlusOrder, "limit" | "qty" | "unit"> | null,
): { deal: number; total: number; price: number } | null {
  if (!order || order.limit <= 0) return null;
  if (limits.perDealMax === null || limits.totalBudget === null) return null;
  // A price for the whole lot is no price per piece: nothing to show then.
  if (order.unit !== "each") return null;
  const price = order.limit;
  const cap = Math.max(0, order.qty);
  return {
    deal: Math.min(cap, Math.floor(limits.perDealMax / price)),
    total: Math.min(cap, Math.floor(limits.totalBudget / price)),
    price,
  };
}

/** An opening offer under a ceiling: four fifths of it, in round steps. */
function opening(ceiling: number): number {
  const raw = Math.floor((ceiling * 4) / 5);
  const step = raw >= 10_000 ? 1_000 : raw >= 1_000 ? 100 : 1;
  return Math.max(1, Math.floor(raw / step) * step);
}

/**
 * The tone preview's deal: another player sells what the first buy order buys and ProBot AI offers
 * under its price, never past one deal's limit; without such an order a plain elytra at 32.000.
 */
export function talkSample(
  orders: readonly PlusOrder[],
  perDealMax: number | null,
): { item: string | null; offer: number } {
  const order = fitOrder(orders);
  let ceiling = order ? order.limit : 40_000;
  if (perDealMax !== null && perDealMax > 0) ceiling = Math.min(ceiling, perDealMax);
  return { item: order?.item ?? null, offer: opening(ceiling) };
}

/** An amount as players write it in chat: "32k", "9,6k" ("9.6k" in English), "1,2m". */
export function chatAmount(amount: number, lang: Lang): string {
  const point = lang === "tr" ? "," : ".";
  const short = (size: number, unit: string) => {
    const tenths = Math.floor(amount / (size / 10));
    const whole = Math.floor(tenths / 10);
    const part = tenths % 10;
    return part ? `${whole}${point}${part}${unit}` : `${whole}${unit}`;
  };
  if (amount >= 1_000_000) return short(1_000_000, "m");
  if (amount >= 1_000) return short(1_000, "k");
  return String(amount);
}

/** The minutes a player may pick between ads, and the service's own while none is picked. */
export const AD_EVERY = [30, 45, 60, 120] as const;
export const AD_EVERY_DEFAULT = 60;

/**
 * The ads' part of a write: the minutes only to a service that knows them (one that sends
 * `adPreview`, trade-any-4). An older service refuses a key it does not know, so it never gets one,
 * and a stored one is not sent back to it (the site's `a813475`).
 */
export function adsWrite(
  discovery: PlusDiscovery,
  ads: boolean,
  every: number | null | undefined,
  intervals: boolean,
): PlusDiscovery {
  const { adEveryMin: _stored, ...today } = discovery;
  // Left out, not null: the field goes back missing (types.ts: it may be missing, read with `??`).
  if (!intervals) return { ...today, ads } as PlusDiscovery;
  const known = (AD_EVERY as readonly number[]).includes(every ?? -1);
  return { ...today, ads, adEveryMin: known ? (every as number) : AD_EVERY_DEFAULT };
}

/** "saatte", "2 saatte", "45 dakikada" / "every hour", "every 2 hours", "every 45 minutes". */
export function everyWords(minutes: number, t: TFunction): string {
  if (minutes % 60 === 0) {
    return minutes === 60
      ? t("setup.plus.ads.everyHour")
      : t("setup.plus.ads.everyHours", { n: minutes / 60 });
  }
  return t("setup.plus.ads.everyMinutes", { n: minutes });
}

/** A sentence's first letter in capitals, the language's way ("i" → "İ" in Turkish). */
function cap(text: string, lang: Lang): string {
  return text.charAt(0).toLocaleUpperCase(lang === "tr" ? "tr-TR" : "en-GB") + text.slice(1);
}

export interface PlusAnswerContext {
  lang: Lang;
  /** The server's money name after amounts ("ripple"), or null for bare amounts. */
  money: string | null;
  /** An order's item in the panel's language. */
  itemName: (ref: string) => string;
  /** Set while on a break: its last day with its ending ("30 Ekim'e" / "until 30 October"). */
  breakUntil: string | null;
  /** Studio's switches: the working hours and the low-credit saving. */
  schedule: boolean;
  saver: boolean;
  /** The service's ad plan (trade-any-4); null from an older service. */
  preview: PlusAdPreview | null;
  /** The notification switches (every cloud account's); null while they load. */
  notify: CloudNotifyPrefs | null;
}

export type PlusAnswerConfig = Pick<
  PlusConfig,
  | "orders"
  | "limits"
  | "authority"
  | "allow"
  | "discovery"
  | "persona"
  | "replies"
  | "who"
  | "blocked"
  | "schedule"
>;

/** An amount with the money's name after it, where the profile names one ("30.000 ripple"). */
export function withMoney(amount: string, money: string | null): string {
  return money ? `${amount} ${money}` : amount;
}

/** The notices "Bildirimler" switches, by group (the launcher's own row). */
export const NOTIFY_GROUPS: ReadonlyArray<{ key: string; events: readonly string[] }> = [
  // The assistant pausing is the account's state, not a summary.
  { key: "account", events: ["stopped", "paused", "renewal", "expired"] },
  { key: "deals", events: ["approval", "found", "deal_done", "deal_failed"] },
  { key: "messages", events: ["message", "incident"] },
  { key: "allowance", events: ["quota_full", "quota_80", "gift_ending"] },
  { key: "recaps", events: ["summary", "weekly"] },
];

/** "Hepsi açık", "2 bildirim kapalı", and whether the quiet hours are on. */
export function notifyAnswer(prefs: CloudNotifyPrefs | null, t: TFunction): string {
  if (!prefs) return t("afkPlus.settings.notifyAll");
  const off = NOTIFY_GROUPS.flatMap((group) => group.events).filter(
    (event) => prefs.events[event] === false,
  ).length;
  const base = off
    ? t("setup.plus.answers.notifySome", { count: off })
    : t("setup.plus.answers.notifyOn");
  return prefs.quiet.enabled ? `${base}${t("setup.plus.answers.notifyQuiet")}` : base;
}

/** One sentence per AFK+ question, from the settings as they stand (a draft's too). */
export function plusAnswers(
  config: PlusAnswerConfig,
  ctx: PlusAnswerContext,
  t: TFunction,
): Record<PlusStep | "who" | "time" | "more" | "notify", string> {
  const a = (key: string, values?: Record<string, unknown>) =>
    t(`setup.plus.answers.${key}`, values);
  const amount = (value: number) => formatAmount(value, ctx.lang);
  const { buys, sells } = sides(config.orders);
  const names = [...new Set([...buys, ...sells].map((o) => ctx.itemName(o.item)))];
  const orders = names.length
    ? a("orders", { count: buys.length + sells.length, names: names.slice(0, 3).join(", ") })
    : t("setup.plus.noOrders");

  const { perDealMax, totalBudget, approvalAbove } = config.limits;
  // Every amount carries the money's name where the profile names one (the site's rule).
  const priced = (value: number) => withMoney(amount(value), ctx.money);
  const money =
    perDealMax !== null && totalBudget !== null
      ? a("money", { perDeal: priced(perDealMax), total: priced(totalBudget) })
      : a("noBuying");

  // Both sides are the default: only one that is off is said.
  const allow = config.allow.buy
    ? config.allow.sell
      ? ""
      : a("buyOnly")
    : config.allow.sell
      ? a("sellOnly")
      : a("neither");
  const authority = authorityOf(config.authority);
  // The approval limit works only where it finishes deals by itself (README: `deal`).
  const said =
    authority === "deal"
      ? approvalAbove !== null
        ? `${a("deal")} ${a("dealAsks", { amount: priced(approvalAbove) })}`
        : a("deal")
      : a(authority);
  const act = allow ? `${said} ${allow}` : said;

  // Without the service's plan (an older service) there are no minutes to say: it posts ads.
  const preview = ctx.preview;
  const adsPart = !config.discovery.ads
    ? a("noAds")
    : !preview
      ? a("adsPlain")
      : preview.stoppedToday
        ? a("adsStopped")
        : cap(a("ads", { every: everyWords(preview.everyMin, t) }), ctx.lang);
  // Scanning and writing to a seller once are on by default: said, never hidden.
  const { scanChat, proactivePm } = config.discovery;
  const chat = `${adsPart}${scanChat && proactivePm ? a("writes") : scanChat ? a("scans") : a("noScan")}`;

  const preset = t(`afkPlus.settings.presets.${presetKey(config.persona.preset)}`);
  const talk = config.persona.playerTalk ? a("tone", { preset }) : preset;

  const friends = config.who.mode === "friends";
  const scope = config.replies.scope;
  // "Yalnız hazır cevaplar" names nobody; the other two say whose messages it answers.
  const whom =
    scope === "off"
      ? a("whoOff")
      : scope === "all"
        ? a(friends ? "whoAllFriends" : "whoAll")
        : a(friends ? "whoTradeFriends" : "whoTrade");
  const blocked = config.blocked.length ? a("blocked", { count: config.blocked.length }) : "";
  const who = `${whom}${config.replies.away ? a("away") : a("noAway")}${blocked}`;

  const time = ctx.breakUntil
    ? a("onBreak", { when: ctx.breakUntil })
    : ctx.schedule && config.schedule.on
      ? a("hours")
      : a("always");

  return {
    orders,
    money,
    act,
    chat,
    talk,
    who,
    time,
    more: ctx.saver ? a("more") : a("moreNoSaver"),
    notify: notifyAnswer(ctx.notify, t),
  };
}

/** The presets the launcher names; another reads as the default, "Kibar" (model.ts `presetOf`). */
function presetKey(preset: string): string {
  return ["friendly", "brief", "polite", "playful"].includes(preset) ? preset : "polite";
}

/**
 * The question that holds a limit "Sınırlara git" goes to, and whether the field waits under that
 * question's "Gelişmiş": the spending amounts are "Ne kadar harcayabilsin?"'s, the approval limits
 * work only under "Pazarlık et ve tamamla" ("ProBot AI ne yapsın?").
 */
export function limitQuestion(field: LimitField): { question: PlusQuestion; advanced: boolean } {
  switch (field) {
    case "approvalAbove":
    case "newCounterpartApprovalAbove":
      return { question: "act", advanced: false };
    case "dailySpendMax":
      return { question: "money", advanced: true };
    default:
      return { question: "money", advanced: false };
  }
}

// --- 7/24 Bulut ----------------------------------------------------------------------------------------

export const ACCOUNT_STEPS = ["server", "login", "join", "replies"] as const;
export type AccountStep = (typeof ACCOUNT_STEPS)[number];
export type AccountQuestion = AccountStep | "more";

export const ACCOUNT_ITEMS: Record<AccountQuestion, string> = {
  server: "minecraft:compass",
  login: "minecraft:trial_key",
  join: "minecraft:command_block",
  replies: "minecraft:oak_sign",
  more: "minecraft:comparator",
};

/** The triggers that type a row when the account joins (the rest repeat on a clock). */
const JOIN_TRIGGERS = ["join", "start", "reconnect"];

export function isJoinTrigger(on: string): boolean {
  return JOIN_TRIGGERS.includes(on);
}

/** Which question a command row sits under: the login, what it types on joining, or a repeat. */
export type CommandPlace = "login" | "join" | "repeat";

/**
 * A row's question: where it was added in this visit, else as it was saved (a hidden login-time row
 * is the login itself), else as it stands. So a row keeps its place until it is saved: one that
 * turns hidden while typed (a password) does not jump to the other question under the player's hands.
 */
export function commandPlace(
  row: Command,
  saved: ReadonlyMap<string, Command>,
  added: ReadonlyMap<string, CommandPlace>,
): CommandPlace {
  const own = added.get(row.id);
  if (own) return own;
  const before = saved.get(row.id) ?? row;
  if (!isJoinTrigger(before.on)) return "repeat";
  return before.words.kind === "plain" ? "join" : "login";
}

/** A row's words for an answer: a hidden one as its preview ("/login ******"), never its text. */
export function rowWords(row: Command): string {
  const words = row.words;
  if (words.kind === "sealed") return words.preview;
  if (words.kind === "secret") return secretPreview(words.text);
  return words.text.trim();
}

/** The host as the address field shows it: the port only when it is not the game's own. */
function hostOf(draft: Pick<Draft, "host" | "port">): string {
  const host = draft.host.trim();
  const port = draft.port.trim();
  if (!host) return "-";
  return port && port !== "25565" && port !== "0" ? `${host}:${port}` : host;
}

/** One sentence per account question, from the draft as it stands. */
export function accountAnswers(
  draft: Pick<Draft, "host" | "port" | "version" | "config">,
  place: (row: Command) => CommandPlace,
  t: TFunction,
): Record<AccountQuestion, string> {
  const a = (key: string, values?: Record<string, unknown>) =>
    t(`setup.account.answers.${key}`, values);
  const version = draft.version === "auto" ? a("versionAuto") : draft.version;
  const rows = draft.config.commands;
  const logins = rows.filter((row) => place(row) === "login");
  const joins = rows.filter((row) => place(row) === "join" && rowWords(row));
  const firstLogin = logins[0];
  const firstJoin = joins[0];
  const login = !firstLogin
    ? a("noLogin")
    : logins.length === 1
      ? a("login", { text: rowWords(firstLogin) })
      : a("loginMany", { n: logins.length });
  const join = !firstJoin
    ? a("noJoin")
    : joins.length === 1
      ? a("join", { delay: firstJoin.delay ?? 0, text: rowWords(firstJoin) })
      : a("joinMany", { n: joins.length, text: rowWords(firstJoin) });
  const rules: Rule[] = draft.config.rules.filter((rule) => rule.pattern.trim());
  const replies = rules.length
    ? a("replies", {
        count: rules.length,
        words: rules
          .slice(0, 2)
          .map((rule) => `"${rule.pattern.trim()}"`)
          .join(", "),
      })
    : a("noReplies");
  return {
    server: a("server", { host: hostOf(draft), version }),
    login,
    join,
    replies,
    more: a("more"),
  };
}

/** The account's setup: offered while it has no login-time command besides the login and no rule. */
export function accountNeedsSetup(config: Draft["config"]): boolean {
  const plainJoins = config.commands.filter(
    (row) => isJoinTrigger(row.on) && row.words.kind === "plain",
  );
  return plainJoins.length === 0 && config.rules.length === 0;
}
