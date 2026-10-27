// AFK+: what the panel asks of the backend (commands/cloud_afk_plus.rs over pl-cloud-afk). Nothing
// polls: the panel reads when it opens and after an action; while it is on screen its account is
// watched with `ai` and the AI's state, new audit log entries, the orders' progress and approvals
// arrive as `cloud-afk-live` events (the console's stream, `cloudApi.onLive`). Browser previews
// use the mock (mock.ts).
import { type CmdError, commands } from "@/bindings";
import { CommandError } from "@/lib/api/tauri";
import { isTauri } from "@/lib/platform";
import { mockPlusApi } from "./mock";
import type {
  CloudNotifyPrefs,
  PlusAction,
  PlusAnswerOutcome,
  PlusApproval,
  PlusAuditEntry,
  PlusAuditOutcome,
  PlusCheckOutcome,
  PlusConfig,
  PlusControlAnswer,
  PlusExport,
  PlusHistory,
  PlusInboxPage,
  PlusItemData,
  PlusNewCustomItem,
  PlusNewOrder,
  PlusOrderOutcome,
  PlusOrderPatch,
  PlusOrderSettings,
  PlusPanelView,
  PlusPickOutcome,
  PlusQuota,
  PlusReceiptPage,
  PlusReceiptSummary,
  PlusReplyOutcome,
  PlusSaveOutcome,
  PlusSettings,
  PlusThreadPage,
  PlusWeeklyReport,
  Wire,
} from "./types";

export type * from "./types";
export type PlusPanel = PlusPanelView;

/** What "Envanterden seç" sends: the slot as the live inventory shows it and the item there. */
export interface PickTarget {
  slot: number;
  item: string;
  cmd: number | null;
  model: string | null;
}

export interface PlusApi {
  /** The panel's first look: settings (with the server, Studio's rules, the orders' states), the quota and the approvals. */
  panel(id: string): Promise<PlusPanel>;
  /** The settings as stored now ("Son halini yükle"). */
  settings(id: string): Promise<PlusSettings>;
  /** The game's item data for a Minecraft version (the account's), or null when it cannot be had now. */
  itemData(minecraft: string | null): Promise<PlusItemData | null>;
  save(id: string, rev: number, config: PlusConfig): Promise<PlusSaveOutcome>;
  /** "Bu hale dön": the revision `rev` becomes the next (`current`: the one on screen). */
  restore(id: string, rev: number, current: number): Promise<PlusSaveOutcome>;
  control(id: string, action: PlusAction, name?: string): Promise<PlusControlAnswer>;
  /** "Kontrol et": the card, or why not. */
  check(id: string, text: string): Promise<PlusCheckOutcome>;
  createOrder(
    id: string,
    order: PlusNewOrder,
    customItems: PlusNewCustomItem[],
    settings: PlusOrderSettings,
  ): Promise<PlusOrderOutcome>;
  patchOrder(id: string, order: string, patch: PlusOrderPatch): Promise<PlusOrderOutcome>;
  deleteOrder(id: string, order: string): Promise<PlusOrderOutcome>;
  pick(id: string, target: PickTarget): Promise<PlusPickOutcome>;
  /** A page of the audit log (`after`: newer than the last kept, `before`: older). */
  audit(
    id: string,
    before: number | null,
    after: number | null,
    group: string | null,
  ): Promise<PlusAuditOutcome>;
  /** What this computer kept of the audit log, newest first. */
  auditKept(id: string): Promise<PlusAuditEntry[]>;
  /** Keeps entries the live stream brought. */
  auditKeep(id: string, items: PlusAuditEntry[]): Promise<void>;
  /** Drops the audit logs kept for accounts not named. */
  auditPrune(keep: string[]): Promise<void>;
  receipts(id: string, before: string | null): Promise<PlusReceiptPage>;
  approvals(id: string): Promise<PlusApproval[]>;
  answer(
    id: string,
    approval: string,
    approve: boolean,
    maxPrice: number | null,
  ): Promise<PlusAnswerOutcome>;
  quota(id: string): Promise<PlusQuota>;
  inbox(id: string, after: string | null): Promise<PlusInboxPage>;
  thread(id: string, name: string, after: string | null): Promise<PlusThreadPage>;
  reply(id: string, name: string, text: string): Promise<PlusReplyOutcome>;
  read(id: string, name: string, upTo: number): Promise<void>;
  handBack(id: string, name: string): Promise<void>;
  history(id: string): Promise<PlusHistory>;
  summary(id: string, from: number | null, to: number | null): Promise<PlusReceiptSummary>;
  report(week: string | null): Promise<PlusWeeklyReport>;
  /** "Tabloyu indir": a CSV in the Downloads folder; its file name. */
  exportTable(
    id: string,
    name: string,
    from: number | null,
    to: number | null,
  ): Promise<PlusExport>;
  showExport(fileName: string): Promise<void>;
  notifyPrefs(): Promise<CloudNotifyPrefs>;
  setNotifyPrefs(prefs: CloudNotifyPrefs): Promise<CloudNotifyPrefs>;
  /** The player saw an AFK notice: the phone stays quiet. */
  noticeSeen(id: string): Promise<void>;
  /** The panel is on screen: the AI streams through `cloudApi.onLive`. */
  watch(id: string): Promise<void>;
  unwatch(id: string): Promise<void>;
}

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

/** A command's answer as the UI reads it: every field present (types.ts). */
async function ok<T>(promise: Promise<Res<T>>): Promise<Wire<T>> {
  const result = await promise;
  if (result.status === "error") throw new CommandError(result.error);
  return result.data as Wire<T>;
}

/** A command's answer kept as the bindings type it (optional fields stay optional). */
async function raw<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new CommandError(result.error);
  return result.data;
}

const tauriPlusApi: PlusApi = {
  panel: (id) => ok(commands.cloudAfkPlusPanel(id)),
  settings: (id) => ok(commands.cloudAfkPlusSettings(id)),
  itemData: (minecraft) => ok(commands.cloudAfkPlusItemData(minecraft)),
  save: (id, rev, config) => ok(commands.cloudAfkPlusSave(id, rev, config)),
  restore: (id, rev, current) => ok(commands.cloudAfkPlusRestore(id, rev, current)),
  control: (id, action, name) => ok(commands.cloudAfkPlusControl(id, action, name ?? null)),
  check: (id, text) => raw(commands.cloudAfkPlusCheck(id, text)),
  createOrder: (id, order, customItems, settings) =>
    ok(commands.cloudAfkPlusOrderCreate(id, order, customItems, settings)),
  patchOrder: (id, order, patch) => ok(commands.cloudAfkPlusOrderPatch(id, order, patch)),
  deleteOrder: (id, order) => ok(commands.cloudAfkPlusOrderDelete(id, order)),
  pick: (id, target) =>
    ok(commands.cloudAfkPlusPick(id, target.slot, target.item, target.cmd, target.model)),
  audit: (id, before, after, group) => raw(commands.cloudAfkPlusAudit(id, before, after, group)),
  auditKept: (id) => raw(commands.cloudAfkPlusAuditKept(id)),
  auditKeep: async (id, items) => {
    await raw(commands.cloudAfkPlusAuditKeep(id, items));
  },
  auditPrune: async (keep) => {
    await raw(commands.cloudAfkPlusAuditPrune(keep));
  },
  receipts: (id, before) => ok(commands.cloudAfkPlusReceipts(id, before)),
  approvals: (id) => ok(commands.cloudAfkPlusApprovals(id)),
  answer: (id, approval, approve, maxPrice) =>
    ok(commands.cloudAfkPlusAnswer(id, approval, approve, maxPrice)),
  quota: (id) => ok(commands.cloudAfkPlusQuota(id)),
  inbox: (id, after) => ok(commands.cloudAfkPlusInbox(id, after)),
  thread: (id, name, after) => ok(commands.cloudAfkPlusThread(id, name, after)),
  reply: (id, name, text) => ok(commands.cloudAfkPlusReply(id, name, text)),
  read: async (id, name, upTo) => {
    await ok(commands.cloudAfkPlusRead(id, name, upTo));
  },
  handBack: async (id, name) => {
    await ok(commands.cloudAfkPlusHandBack(id, name));
  },
  history: (id) => ok(commands.cloudAfkPlusHistory(id)),
  summary: (id, from, to) => ok(commands.cloudAfkPlusSummary(id, from, to)),
  report: (week) => ok(commands.cloudAfkPlusReport(week)),
  exportTable: (id, name, from, to) => ok(commands.cloudAfkPlusExport(id, name, from, to)),
  showExport: async (fileName) => {
    await ok(commands.cloudAfkPlusShowExport(fileName));
  },
  notifyPrefs: () => ok(commands.cloudAfkNotifyPrefs()),
  setNotifyPrefs: (prefs) => ok(commands.cloudAfkSetNotifyPrefs(prefs)),
  noticeSeen: async (id) => {
    await commands.cloudAfkNoticeSeen(id);
  },
  watch: async (id) => {
    await ok(commands.cloudAfkPlusWatch(id));
  },
  unwatch: async (id) => {
    await commands.cloudAfkPlusUnwatch(id);
  },
};

export const plusApi: PlusApi = isTauri() ? tauriPlusApi : mockPlusApi;
