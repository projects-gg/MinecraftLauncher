// AFK+'s types as the UI receives them. pl-cloud-afk reads the Worker's JSON with
// `#[serde(default)]` (a field a service leaves out takes its default), so specta writes every
// field of these types optional; what the launcher's backend sends to the UI always carries them all
// (none is skipped when serialized). `Wire<T>` says so once, instead of a check at every read.
// Values sent back (a config to save, notification switches) are assignable to the bindings' types.
import type * as B from "@/bindings";
import type { CloudAccount, CloudOverview } from "../api";

/** Every field present, deep: lists and nested objects too; `null` stays where Rust has `Option`. */
export type Wire<T> = T extends readonly (infer U)[]
  ? Wire<U>[]
  : T extends object
    ? { [K in keyof T]-?: Wire<Exclude<T[K], undefined>> }
    : T;

export type CloudNotifyPrefs = Wire<B.CloudNotifyPrefs>;
export type CloudQuietHours = Wire<B.CloudQuietHours>;
export type MePlus = Wire<B.MePlus>;
export type PlusAction = B.PlusAction;
export type PlusAdTemplate = Wire<B.PlusAdTemplate>;
export type PlusAi = Wire<B.PlusAi>;
export type PlusAllowance = Wire<B.PlusAllowance>;
export type PlusAnswerOutcome = Wire<B.PlusAnswerOutcome>;
export type PlusApproval = Wire<B.PlusApproval>;
export type PlusApprovalDeal = Wire<B.PlusApprovalDeal>;
export type PlusBlockedCounts = Wire<B.PlusBlockedCounts>;
export type PlusBuyRow = Wire<B.PlusBuyRow>;
export type PlusCanBuy = Wire<B.PlusCanBuy>;
export type PlusCatalog = Wire<B.PlusCatalog>;
export type PlusCatalogItem = Wire<B.PlusCatalogItem>;
export type PlusCatalogLimits = Wire<B.PlusCatalogLimits>;
export type PlusConfig = Wire<B.PlusConfig>;
export type PlusControlAnswer = Wire<B.PlusControlAnswer>;
export type PlusDeal = Wire<B.PlusDeal>;
export type PlusDealPage = Wire<B.PlusDealPage>;
export type PlusDealSummary = Wire<B.PlusDealSummary>;
export type PlusDiscovery = Wire<B.PlusDiscovery>;
export type PlusEnchantment = Wire<B.PlusEnchantment>;
export type PlusExport = Wire<B.PlusExport>;
export type PlusFeatures = Wire<B.PlusFeatures>;
export type PlusFeedItem = Wire<B.PlusFeedItem>;
export type PlusFeedPage = Wire<B.PlusFeedPage>;
export type PlusGift = Wire<B.PlusGift>;
export type PlusHistory = Wire<B.PlusHistory>;
export type PlusHistoryEntry = Wire<B.PlusHistoryEntry>;
export type PlusInboxLast = Wire<B.PlusInboxLast>;
export type PlusInboxLine = Wire<B.PlusInboxLine>;
export type PlusInboxPage = Wire<B.PlusInboxPage>;
export type PlusInboxThread = Wire<B.PlusInboxThread>;
export type PlusIssue = Wire<B.PlusIssue>;
export type PlusItemSummary = Wire<B.PlusItemSummary>;
export type PlusJudge = Wire<B.PlusJudge>;
export type PlusJudgeState = Wire<B.PlusJudgeState>;
export type PlusLimits = Wire<B.PlusLimits>;
export type PlusMoney = Wire<B.PlusMoney>;
export type PlusNewMessages = Wire<B.PlusNewMessages>;
export type PlusOffer = Wire<B.PlusOffer>;
export type PlusPanelView = Wire<B.PlusPanelView>;
export type PlusParseDraft = Wire<B.PlusParseDraft>;
export type PlusParseOutcome = Wire<B.PlusParseOutcome>;
export type PlusPersona = Wire<B.PlusPersona>;
export type PlusProfile = Wire<B.PlusProfile>;
export type PlusPromo = Wire<B.PlusPromo>;
export type PlusQuota = Wire<B.PlusQuota>;
export type PlusQuotaBrief = Wire<B.PlusQuotaBrief>;
export type PlusQuotaWindow = Wire<B.PlusQuotaWindow>;
export type PlusReplies = Wire<B.PlusReplies>;
export type PlusReplyOutcome = Wire<B.PlusReplyOutcome>;
export type PlusRequirements = Wire<B.PlusRequirements>;
export type PlusRestriction = Wire<B.PlusRestriction>;
export type PlusSaveOutcome = Wire<B.PlusSaveOutcome>;
export type PlusSchedule = Wire<B.PlusSchedule>;
export type PlusSellRow = Wire<B.PlusSellRow>;
export type PlusSettings = Wire<B.PlusSettings>;
export type PlusSpent = Wire<B.PlusSpent>;
export type PlusThreadPage = Wire<B.PlusThreadPage>;
export type PlusTotals = Wire<B.PlusTotals>;
export type PlusTradeSettings = Wire<B.PlusTradeSettings>;
export type PlusTradeState = Wire<B.PlusTradeState>;
export type PlusWeeklyAccount = Wire<B.PlusWeeklyAccount>;
export type PlusWeeklyReport = Wire<B.PlusWeeklyReport>;
export type PlusWindow = Wire<B.PlusWindow>;
export type PlusWindowCaps = Wire<B.PlusWindowCaps>;

/** An account's AI state as the overview carries it (the live `ai` event too). */
export function plusAi(account: CloudAccount | null | undefined): PlusAi | null {
  return (account?.ai ?? null) as PlusAi | null;
}

/** The AFK+ offer of the overview: null while AFK+ is off (then nothing of it shows). */
export function plusOffer(overview: CloudOverview | null | undefined): PlusOffer | null {
  return (overview?.plus ?? null) as PlusOffer | null;
}

/** What the player may do about AFK+ (buy, an invite). */
export function plusMe(overview: CloudOverview | null | undefined): MePlus | null {
  return (overview?.plusMe ?? null) as MePlus | null;
}
