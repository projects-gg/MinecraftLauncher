// AFK+ settings on the launcher (AfkPlusConfig v2, afkplus-free.md §1.2, §3): the contract's
// defaults, the checks made before a `PUT /plus` (README "AfkPlusConfig v2" → Rules, in the Worker's
// words; the Worker and the agent have the last word), the "Ayarlar" draft, the AI state's sentence,
// and what changed between two saved versions (QoL Q7). The same rules as afk.projects.gg's, so the
// three surfaces agree. Orders have their own module (order-draft.ts, orders.ts).
import type {
  PlusAction,
  PlusAi,
  PlusConfig,
  PlusLimits,
  PlusOrder,
  PlusQuota,
  PlusRules,
  PlusServer,
  PlusTradeSettings,
  PlusWindow,
} from "./types";
import type { Lang } from "./words";

export type { PlusAi, PlusConfig, PlusLimits, PlusWindow };

/** The contract's limits. */
export const PLUS_LIMITS = {
  /** The generic mode's money cap (`money.maxDeal`); a profile's own comes with `server.money`. */
  maxDeal: 100_000_000,
  blockedMax: 100,
  /** "Kimlerle": in-game names, at most 50. */
  friendsMax: 50,
  windowsMax: 7,
  introMax: 60,
  statusMax: 80,
  replyMax: 200,
  /** A break reaches at most 90 days ahead. */
  breakDays: [1, 3, 7, 14, 30] as const,
} as const;

export type Side = "buy" | "sell";
export type Style = "patient" | "balanced" | "quick";
export type Scope = "all" | "trade" | "off";
export type Preset = "friendly" | "brief" | "polite" | "playful";
export type StatusPreset = "back_soon" | "back_evening" | "back_tomorrow" | "weekend" | "busy";
/** "ProBot AI ne yapsın?": answer only, haggle and ask (the default), haggle and finish. */
export type Authority = "reply" | "ask" | "deal";

export const STYLES: Style[] = ["patient", "balanced", "quick"];
export const AUTHORITIES: Authority[] = ["reply", "ask", "deal"];
export const PRESETS: Preset[] = ["friendly", "brief", "polite", "playful"];
export const STATUS_PRESETS: StatusPreset[] = [
  "back_soon",
  "back_evening",
  "back_tomorrow",
  "weekend",
  "busy",
];

/**
 * The style replies take: the stored one, else "Kibar" where the settings name none the launcher
 * knows (the default since 28 Oct 2026, with "Oyuncu gibi konuşsun" on).
 */
export function presetOf(preset: string): Preset {
  return PRESETS.find((known) => known === preset) ?? "polite";
}

/** Whether the bot may spend at all (without it buy orders for money only tell what they find). */
export function canSpend(limits: PlusLimits): boolean {
  return limits.totalBudget !== null && limits.perDealMax !== null;
}

export type EmptyLimit = "buysNothing" | "noLimit" | "noAsk";

/**
 * What leaving a limit empty does (README `SpendLimits`): without the per-deal or the total limit
 * nothing is bought (the hard rule, `canSpend`), without the daily one there is no daily cap,
 * without an approval limit no amount is asked about first. Null where it cannot be empty.
 */
export function emptyLimit(key: keyof PlusLimits): EmptyLimit | null {
  switch (key) {
    case "perDealMax":
    case "totalBudget":
      return "buysNothing";
    case "dailySpendMax":
      return "noLimit";
    case "approvalAbove":
    case "newCounterpartApprovalAbove":
      return "noAsk";
    default:
      return null;
  }
}

/** "Sohbete ilan": the minutes between ads a player may pick (`discovery.adEveryMin`). */
export const AD_EVERY_MIN = [30, 45, 60, 120] as const;
/** The service's minutes while the player has picked none. */
export const AD_EVERY_DEFAULT = 60;

// --- The "Ayarlar" draft ---------------------------------------------------------------------------

/** What "Ayarlar" edits as one draft; orders, the switch, a break and blocked names save at once. */
export type SettingsPart = Pick<
  PlusConfig,
  | "lang"
  | "persona"
  | "replies"
  | "authority"
  | "allow"
  | "pay"
  | "who"
  | "limits"
  | "discovery"
  | "trade"
  | "summaryAt"
  | "schedule"
>;

const SETTINGS_KEYS = [
  "lang",
  "persona",
  "replies",
  "authority",
  "allow",
  "pay",
  "who",
  "limits",
  "discovery",
  "trade",
  "summaryAt",
  "schedule",
] as const;

export function settingsPart(config: SettingsPart): SettingsPart {
  return {
    lang: config.lang,
    persona: config.persona,
    replies: config.replies,
    authority: config.authority,
    allow: config.allow,
    pay: config.pay,
    who: config.who,
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

const isRecord = (value: unknown): value is Record<string, unknown> =>
  Boolean(value) && typeof value === "object" && !Array.isArray(value);

const isNames = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((entry) => typeof entry === "string");

/**
 * A list of names edited in the draft (`who.friends`) on top of the latest one: only the draft's
 * own additions and removals, so a name added elsewhere meanwhile ("Arkadaşlarıma ekle") stays.
 * Names compare as the game does, without case. Null when these are not lists of names.
 */
function mergeNames(before: unknown, after: unknown, latest: unknown): string[] | null {
  if (!isNames(before) || !isNames(after) || !isNames(latest)) return null;
  const has = (list: string[], name: string) =>
    list.some((entry) => entry.toLowerCase() === name.toLowerCase());
  const out = latest.filter((name) => !has(before, name) || has(after, name));
  for (const name of after) if (!has(before, name) && !has(out, name)) out.push(name);
  return out;
}

/**
 * The player's own edits on top of `latest`: a field (one level into the groups: `limits.perDealMax`,
 * `allow.sell`) comes from the draft only where the draft differs from its base, so what changed
 * elsewhere meanwhile (the site, the phone) stays; a list of names takes only the draft's own
 * additions and removals.
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
        if (stable(after[sub]) !== stable(before[sub])) {
          merged[sub] = mergeNames(before[sub], after[sub], merged[sub]) ?? after[sub];
        }
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

export type NameProblem = "format" | "duplicate" | "blocked";

/**
 * An in-game name for a list ("Engellenenler", "Kimlerle"): its form, once ignoring case, and for
 * the friends list not a blocked name (the Worker's `blocked`).
 */
export function blockedNameProblem(
  name: string,
  list: readonly string[],
  blocked: readonly string[] = [],
): NameProblem | null {
  if (!/^[A-Za-z0-9_]{3,16}$/.test(name)) return "format";
  const lower = name.toLowerCase();
  if (list.some((entry) => entry.toLowerCase() === lower)) return "duplicate";
  if (blocked.some((entry) => entry.toLowerCase() === lower)) return "blocked";
  return null;
}

export type PersonaTextProblem = "form" | "length" | "marker" | "link" | "phone" | "email";

/**
 * The service's form rule for the texts AFK+ says (a persona text, a custom item's name): one line,
 * no command or format code first, no colour code ("&a", "§l"), no client variable ("%username%").
 */
export function breaksForm(text: string): boolean {
  return /\n|^[/\\&§]|§|&[0-9a-fk-or]|%[\p{L}\p{N}_]+%/iu.test(text);
}

/** The `[AFK+]` marker however it is spaced, cased or widened. */
export function hasMarker(text: string): boolean {
  return /\[\s*afk\s*\+\s*\]/iu.test(text.normalize("NFKC"));
}

/**
 * Fields the settings mark while they are edited, before any "Kaydet": a persona text longer than
 * its room, an approval line above the per-deal limit. While one is marked the change bar's
 * "Kaydet" waits (AFK-CLEAR, as the AFK settings bar since SHEET-UX).
 */
export function liveProblems(shown: SettingsPart, maxDeal: number): boolean {
  const persona = shown.persona;
  return (
    personaTextProblem(persona.intro, PLUS_LIMITS.introMax) === "length" ||
    (persona.statusPreset === null &&
      personaTextProblem(persona.status, PLUS_LIMITS.statusMax) === "length") ||
    limitProblems(shown.limits, maxDeal).approvalAbove === "approvalAbove"
  );
}

/** The launcher's quick look at a persona text; the Worker's text rule and its judge decide. */
export function personaTextProblem(text: string, max: number): PersonaTextProblem | null {
  const value = text.trim();
  if (!value) return null;
  if (breaksForm(value)) return "form";
  if ([...value].length > max) return "length";
  if (hasMarker(value)) return "marker";
  if (/@[a-z0-9-]+\.[a-z]{2,}/i.test(value)) return "email";
  // AFK+ sends no link at all: projects.gg counts like any other address (the service's rule).
  if (/(https?:\/\/|www\.|\b[a-z0-9-]+\.(com|net|org|gg|io|me|tr|xyz)\b)/i.test(value))
    return "link";
  if ((value.match(/\d/g) ?? []).length >= 10) return "phone";
  return null;
}

/** The tag before the assistant's lines until the player sets one: off, "[AFK+]" (27 Oct 2026). */
export const PREFIX_DEFAULT: Readonly<{ on: boolean; text: string }> = Object.freeze({
  on: false,
  text: "[AFK+]",
});
export const PREFIX_MAX = 16;

export type PrefixProblem = "empty" | "length" | "form" | "staff" | "name";

/** A tag's characters (the service's rule): Latin letters, ASCII digits, spaces and a few marks. */
const PREFIX_CHARS = /^[\p{Script=Latin}0-9 [\]()+\-_.!?*#|~^=:']+$/u;
/** Staff, system and server words, folded, anywhere in the tag's letters (the service's list). */
const PREFIX_STAFF_WORDS = [
  "yetkili",
  "admin",
  "moderator",
  "sistem",
  "system",
  "server",
  "sunucu",
  "konsol",
  "console",
  "staff",
  "kurucu",
  "founder",
  "owner",
  "destek",
  "support",
  "rehber",
  "helper",
  "duyuru",
  "bilgi",
  "info",
  "projects",
  "operator",
  "yonetic",
  "yonetim",
  "guvenlik",
  "security",
  "resmi",
  "official",
  "gelistirici",
  "developer",
];
/** Short ones: whole words only (camel case split first). */
const PREFIX_STAFF_SHORT = new Set(["mod", "adm", "op", "gm", "sys", "dev"]);
const PREFIX_FOLD: Record<string, string> = {
  ç: "c",
  ğ: "g",
  ı: "i",
  ö: "o",
  ş: "s",
  ü: "u",
  â: "a",
  î: "i",
  û: "u",
  "0": "o",
  "1": "i",
  "3": "e",
  "4": "a",
  "5": "s",
  "7": "t",
  "!": "i",
  "|": "i",
};

/**
 * The panel's quick look at the tag as the player types it (README "AfkPlusConfig v2" →
 * `persona.prefix`, the same look as the site's): empty, over 16 characters, other characters, a
 * staff or system word (look-alike digits and camel case too), or a friend's or blocked player's
 * name. `plus_check.rs` and the service have the last word (links, apps, words).
 */
export function prefixProblem(text: string, names: readonly string[] = []): PrefixProblem | null {
  // Spaces as the chat cleaning leaves them: trimmed, runs as one.
  const value = text.trim().replace(/\s+/gu, " ");
  if (!value) return "empty";
  if ([...value].length > PREFIX_MAX) return "length";
  if (!PREFIX_CHARS.test(value)) return "form";
  let form = "";
  for (const char of value.replace(/(\p{Ll})(\p{Lu})/gu, "$1 $2").toLocaleLowerCase("tr")) {
    form += PREFIX_FOLD[char] ?? char;
  }
  if (form.split(/[^a-z]+/u).some((word) => PREFIX_STAFF_SHORT.has(word))) return "staff";
  const letters = form.replace(/[^a-z]+/gu, "");
  if (PREFIX_STAFF_WORDS.some((word) => letters.includes(word))) return "staff";
  const known = new Set(names.map((name) => name.toLowerCase()));
  if (
    value
      .toLowerCase()
      .split(/[^a-z0-9_]+/u)
      .some((word) => known.has(word))
  )
    return "name";
  return null;
}

/** Days Monday first (1 … 7). */
export const WEEKDAYS = [1, 2, 3, 4, 5, 6, 7] as const;

/** A window's days in order, without repeats. */
export function sortedDays(days: readonly number[]): number[] {
  return [...new Set(days)].filter((d) => d >= 1 && d <= 7).sort((a, b) => a - b);
}

// --- "Para birimi" (TRADE-ANY, 28 Oct 2026) --------------------------------------------------------

/** The longest money name (`trade.currency`, code points). */
export const CURRENCY_MAX = 16;
/**
 * A money name's characters (the service's rule): Latin letters (Turkish ones too), spaces,
 * `. _ -` and the signs `$ € ₺ £`. No digit: the reply rule reads every digit as a number.
 */
const CURRENCY_CHARS = /^[\p{Script=Latin} ._\-$€₺£]+$/u;
/** At least one letter or sign. */
const CURRENCY_SHOWN = /[\p{Script=Latin}$€₺£]/u;

export type CurrencyProblem = "form" | "length";

/**
 * The panel's quick look at a money name as the player types it (the service's `checkCurrency`,
 * as `plus_check.rs` has it): other characters (a digit too) or over 16 of them. Empty is no name.
 * The service has the last word (listed words, links, contact data, apps).
 */
export function currencyProblem(text: string): CurrencyProblem | null {
  // Spaces as the chat cleaning leaves them: trimmed, runs as one.
  const value = text.trim().replace(/\s+/gu, " ");
  if (!value) return null;
  if (!CURRENCY_CHARS.test(value) || !CURRENCY_SHOWN.test(value)) return "form";
  if ([...value].length > CURRENCY_MAX) return "length";
  return null;
}

/**
 * A money name the player typed that the service would refuse. One they did not touch is the
 * service's to judge: it came from there (the site, the phone) and a save sends it back as it is.
 */
export function currencyEditProblem(
  shown: PlusTradeSettings,
  saved: PlusTradeSettings,
): CurrencyProblem | null {
  const name = shown.currency ?? null;
  if (name === null || name === (saved.currency ?? null)) return null;
  return currencyProblem(name);
}

/**
 * The trade settings after the player typed in "Para birimi": the text as typed, or null once it
 * is emptied, which clears the stored name. With nothing stored an emptied field goes back to how
 * it was read, so an untouched name never reaches a save (an older service knows no such key).
 */
export function withCurrency(
  trade: PlusTradeSettings,
  saved: PlusTradeSettings,
  text: string,
): PlusTradeSettings {
  if (text.trim()) return { ...trade, currency: text };
  return { ...trade, currency: saved.currency ? null : saved.currency };
}

/** How "Para birimi" shows: not at all, the profile's own name read-only, or the player's field. */
export type CurrencyFieldState =
  | { kind: "hidden" }
  | { kind: "fixed"; label: string }
  | { kind: "own" };

/**
 * "Para birimi" shows only while Studio's switch is on (an older service sends no switch: hidden).
 * A profile that names its money (Projects Survival and SkyBlock: ripple) shows that name read-only:
 * the player's own cannot change it.
 */
export function currencyFieldState(
  server: Pick<PlusServer, "money">,
  rules: Pick<PlusRules, "features">,
  lang: Lang,
): CurrencyFieldState {
  if (!rules.features.currency) return { kind: "hidden" };
  const label = server.money.label;
  if (server.money.fixed && label) {
    return { kind: "fixed", label: lang === "en" ? label.en : label.tr };
  }
  return { kind: "own" };
}

// --- "Sınırlara git" (owner, 28 Oct 2026) -----------------------------------------------------------

/** A limit "Sınırlara git" brings into view in "Harcama", its field focused. */
export type LimitField =
  | "perDealMax"
  | "totalBudget"
  | "dailySpendMax"
  | "approvalAbove"
  | "newCounterpartApprovalAbove";

/** An approval's reason that is a limit's, and that limit. */
export interface LimitReason {
  reason: string;
  field: LimitField;
}

const REASON_LIMITS = new Map<string, LimitField>([
  ["above", "approvalAbove"],
  ["new_counterpart", "newCounterpartApprovalAbove"],
  ["daily", "dailySpendMax"],
  ["budget", "totalBudget"],
]);

/**
 * The first of an approval's reasons that is a limit's ("Onay sınırının üstünde"): its line
 * carries "Sınırlara git". None for the others: a price, or the authority the player chose, is no
 * limit to go to.
 */
export function limitReason(why: readonly string[]): LimitReason | null {
  for (const reason of why) {
    const field = REASON_LIMITS.get(reason);
    if (field) return { reason, field };
  }
  return null;
}

/** Buying waits for both spending limits ("bulunca haber veriyor"): the first one missing. */
export function spendingField(limits: Pick<PlusLimits, "perDealMax" | "totalBudget">): LimitField {
  return limits.perDealMax === null ? "perDealMax" : "totalBudget";
}

// --- What changed (QoL Q7) --------------------------------------------------------------------------

export type ConfigChange =
  | { kind: "order_added"; side: Side; item: string }
  | { kind: "order_removed"; side: Side; item: string }
  | {
      kind: "order_amount";
      side: Side;
      item: string;
      field: "target" | "limit" | "qty";
      from: number;
      to: number;
    }
  | { kind: "order_toggled"; side: Side; item: string; on: boolean }
  | { kind: "order_other"; side: Side; item: string }
  | { kind: "authority"; from: string; to: string }
  | { kind: "limit"; field: keyof PlusLimits; from: number | null; to: number | null }
  | { kind: "fineTune" }
  | { kind: "persona" }
  | { kind: "replies" }
  | { kind: "schedule" }
  | { kind: "discovery" }
  | { kind: "blocked" }
  | { kind: "other" };

const sideOf = (order: PlusOrder): Side => (order.side === "sell" ? "sell" : "buy");

function diffOrders(
  before: readonly PlusOrder[],
  after: readonly PlusOrder[],
  out: ConfigChange[],
) {
  const old = new Map(before.map((order) => [order.id, order]));
  const next = new Map(after.map((order) => [order.id, order]));
  for (const order of after) {
    const side = sideOf(order);
    const was = old.get(order.id);
    if (!was) {
      out.push({ kind: "order_added", side, item: order.item });
      continue;
    }
    let told = false;
    for (const field of ["target", "limit", "qty"] as const) {
      if (was[field] !== order[field]) {
        out.push({
          kind: "order_amount",
          side,
          item: order.item,
          field,
          from: was[field],
          to: order[field],
        });
        told = true;
      }
    }
    if (was.on !== order.on) {
      out.push({ kind: "order_toggled", side, item: order.item, on: order.on });
      told = true;
    }
    if (!told && JSON.stringify(was) !== JSON.stringify(order)) {
      out.push({ kind: "order_other", side, item: order.item });
    }
  }
  for (const order of before) {
    if (!next.has(order.id))
      out.push({ kind: "order_removed", side: sideOf(order), item: order.item });
  }
}

/** What a saved version changed against the one before it, most telling first. */
export function configChanges(before: PlusConfig, after: PlusConfig): ConfigChange[] {
  const out: ConfigChange[] = [];
  diffOrders(before.orders, after.orders, out);
  if (before.authority !== after.authority) {
    out.push({ kind: "authority", from: before.authority, to: after.authority });
  }
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
  if (!same(before.allow, after.allow) || before.pay !== after.pay || !same(before.who, after.who))
    out.push({ kind: "fineTune" });
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
      !same(before.customItems, after.customItems) ||
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
  // Bought credits are part of the room too (cloud-afk README "AFK+ extra credits").
  const bought = Math.max(0, quota.bought?.remaining ?? 0);
  return Math.min(
    quota.estReplies,
    Math.floor((gift * quota.estReplies) / (windows + gift + bought)),
  );
}

/** Bought credits left (AFK+ extra credits); 0 from an older service. The site reads the same. */
export function boughtLeft(quota: PlusQuota): number {
  return Math.max(0, quota.bought?.remaining ?? 0);
}

/** How many of the replies left the bought credits pay for: their share of the room, as the gifts'. */
export function boughtReplies(quota: PlusQuota): number {
  const bought = boughtLeft(quota);
  if (bought <= 0 || quota.estReplies <= 0) return 0;
  const windows = Math.max(
    0,
    Math.min(quota.w5h.cap - quota.w5h.used, quota.week.cap - quota.week.used),
  );
  const gift = quota.gifts.reduce((sum, g) => sum + Math.max(0, g.remaining), 0);
  return Math.min(
    quota.estReplies,
    Math.floor((bought * quota.estReplies) / (windows + gift + bought)),
  );
}

/** The windows are full and every gift is spent: bought credits pay for the replies now. */
export function boughtInUse(quota: PlusQuota): boolean {
  if (quota.frozen || boughtLeft(quota) <= 0) return false;
  const windows = Math.min(quota.w5h.cap - quota.w5h.used, quota.week.cap - quota.week.used);
  const gifts = quota.gifts.reduce((sum, g) => sum + Math.max(0, g.remaining), 0);
  return windows <= 0 && gifts <= 0;
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
    // Waiting for its first start (28 Oct 2026: AFK+ never starts by itself): ready, "Başlat".
    case "ready":
      return line("ready", "ok", { canStart: true });
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
  // A server without a trade profile: it chats and agrees, the player makes the trades.
  if (ai.trade.by === "owner") return "tradeOwner";
  if (!ai.trade.on) return ai.trade.off === "profile" ? "tradeProfile" : "tradeLobby";
  if (ai.trade.buy === "notify") return "notifyOnly";
  return null;
}

/**
 * The state the rail shows while the assistant waits for its first start (the service's `ready`,
 * 28 Oct 2026: AFK+ never starts by itself): one plain line and "Başlat". Otherwise the AI's own
 * state, as it is.
 */
export function startState(state: AiState, ready: boolean): AiState {
  if (!ready) return state;
  return { key: "ready", tone: "ok", until: null, canStart: true, canStop: false };
}

/**
 * The settings after a control the service took, as it stores them: "Durdur" and "Başlat" turn the
 * switch, and "Başlat" ends the wait for the first start; blocking a player takes them off the
 * friends list too (a save would be refused).
 */
export function configAfterControl(
  config: PlusConfig,
  action: PlusAction,
  name?: string,
): PlusConfig {
  const next = { ...config };
  if (action === "pause" || action === "resume") next.enabled = action === "resume";
  if (action === "resume") next.ready = null;
  if ((action === "block" || action === "unblock") && name) {
    const other = (n: string) => n.toLowerCase() !== name.toLowerCase();
    const rest = next.blocked.filter(other);
    next.blocked = action === "block" ? [...rest, name] : rest;
    if (action === "block") next.who = { ...next.who, friends: next.who.friends.filter(other) };
  }
  return next;
}
