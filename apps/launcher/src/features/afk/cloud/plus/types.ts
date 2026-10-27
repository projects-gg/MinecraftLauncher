// AFK+'s types as the UI receives them. pl-cloud-afk reads the Worker's JSON with
// `#[serde(default)]` (a field a service leaves out takes its default), so specta writes every
// field of these types optional; what the launcher's backend sends to the UI always carries them all
// (none is skipped when serialized). `Wire<T>` says so once, instead of a check at every read.
// Where serializing and reading differ (a skipped field: an item's `look`, the audit log's entries)
// specta writes two types; the UI takes the `_Serialize` one (what it receives) and sends the
// `_Deserialize` one. Values sent back (a config to save, an order, notification switches) are
// assignable to the bindings' types. A look's and an audit entry's fields stay optional: the backend
// leaves out what they lack.
import type * as B from "@/bindings";
import type { CloudAccount, CloudOverview } from "../api";

/**
 * Every field present, deep: lists and nested objects too; `null` stays where Rust has `Option`. A
 * union variant's `?: never` fields (specta marks the other variants' fields so) are left out.
 */
export type Wire<T> = T extends readonly (infer U)[]
  ? Wire<U>[]
  : T extends object
    ? {
        [K in keyof T as [Exclude<T[K], undefined>] extends [never] ? never : K]-?: Wire<
          Exclude<T[K], undefined>
        >;
      }
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
export type PlusAuditEntry = B.PlusAuditEntry_Serialize;
export type PlusAuditOutcome = B.PlusAuditOutcome_Serialize;
export type PlusBlockedCounts = Wire<B.PlusBlockedCounts>;
export type PlusCanBuy = Wire<B.PlusCanBuy>;
export type PlusCheckOutcome = B.PlusCheckOutcome;
export type PlusConfig = Wire<B.PlusConfig_Serialize>;
export type PlusControlAnswer = Wire<B.PlusControlAnswer>;
export type PlusCustomItem = Wire<B.PlusCustomItem_Serialize>;
export type PlusDiscovery = Wire<B.PlusDiscovery>;
export type PlusExport = Wire<B.PlusExport>;
export type PlusFeatures = Wire<B.PlusFeatures>;
export type PlusGift = Wire<B.PlusGift>;
export type PlusHistory = Wire<B.PlusHistory_Serialize>;
export type PlusHistoryEntry = Wire<B.PlusHistoryEntry_Serialize>;
export type PlusInboxLast = Wire<B.PlusInboxLast>;
export type PlusInboxLine = Wire<B.PlusInboxLine>;
export type PlusInboxPage = Wire<B.PlusInboxPage>;
export type PlusInboxThread = Wire<B.PlusInboxThread>;
export type PlusIssue = Wire<B.PlusIssue>;
export type PlusItemData = Wire<B.PlusItemData>;
export type PlusItemSummary = Wire<B.PlusItemSummary>;
export type PlusJudge = Wire<B.PlusJudge>;
export type PlusJudgeState = Wire<B.PlusJudgeState>;
export type PlusLimits = Wire<B.PlusLimits>;
export type PlusLook = B.PlusLook_Serialize;
export type PlusNewCustomItem = B.PlusNewCustomItem_Deserialize;
export type PlusNewMessages = Wire<B.PlusNewMessages>;
export type PlusNewOrder = B.PlusNewOrder_Deserialize;
export type PlusOffer = Wire<B.PlusOffer>;
export type PlusOrder = Wire<B.PlusOrder>;
export type PlusOrderLive = Wire<B.PlusOrderLive>;
export type PlusOrderOutcome = Wire<B.PlusOrderOutcome_Serialize>;
export type PlusOrderPatch = B.PlusOrderPatch;
export type PlusOrderSettings = B.PlusOrderSettings_Deserialize;
export type PlusOrderView = Wire<B.PlusOrderView>;
export type PlusPanelView = Wire<B.PlusPanelView_Serialize>;
export type PlusPersona = Wire<B.PlusPersona>;
export type PlusPick = Wire<B.PlusPick_Serialize>;
export type PlusPickOutcome = Wire<B.PlusPickOutcome_Serialize>;
export type PlusPromo = Wire<B.PlusPromo>;
export type PlusQuota = Wire<B.PlusQuota>;
export type PlusQuotaBrief = Wire<B.PlusQuotaBrief>;
export type PlusQuotaWindow = Wire<B.PlusQuotaWindow>;
export type PlusReceipt = Wire<B.PlusReceipt>;
export type PlusReceiptPage = Wire<B.PlusReceiptPage>;
export type PlusReceiptSummary = Wire<B.PlusReceiptSummary>;
export type PlusReplies = Wire<B.PlusReplies>;
export type PlusReplyOutcome = Wire<B.PlusReplyOutcome>;
export type PlusReq = Wire<B.PlusReq>;
export type PlusRestriction = Wire<B.PlusRestriction>;
export type PlusRules = Wire<B.PlusRules>;
export type PlusSaveOutcome = Wire<B.PlusSaveOutcome_Serialize>;
export type PlusSchedule = Wire<B.PlusSchedule>;
export type PlusServer = Wire<B.PlusServer>;
export type PlusSettings = Wire<B.PlusSettings_Serialize>;
export type PlusSpent = Wire<B.PlusSpent>;
export type PlusThreadPage = Wire<B.PlusThreadPage>;
export type PlusTotals = Wire<B.PlusTotals>;
export type PlusTradeSettings = Wire<B.PlusTradeSettings>;
export type PlusTradeState = Wire<B.PlusTradeState>;
export type PlusWeeklyAccount = Wire<B.PlusWeeklyAccount>;
export type PlusWeeklyReport = Wire<B.PlusWeeklyReport>;
export type PlusWho = Wire<B.PlusWho>;
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
