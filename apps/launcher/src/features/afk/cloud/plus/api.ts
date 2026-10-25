// AFK+: what the panel asks of the backend (commands/cloud_afk_plus.rs over pl-cloud-afk). Nothing
// polls: the panel reads when it opens and after an action; while it is on screen its account is
// watched with `ai` and the AI's state, feed and approvals arrive as `cloud-afk-live` events (the
// console's stream, `cloudApi.onLive`). Browser previews use the mock (mock.ts).
import { type CmdError, commands } from "@/bindings";
import { CommandError } from "@/lib/api/tauri";
import { isTauri } from "@/lib/platform";
import { mockPlusApi } from "./mock";
import type {
  CloudNotifyPrefs,
  PlusAction,
  PlusAnswerOutcome,
  PlusApproval,
  PlusCatalog,
  PlusConfig,
  PlusControlAnswer,
  PlusDealPage,
  PlusDealSummary,
  PlusExport,
  PlusFeedPage,
  PlusHistory,
  PlusInboxPage,
  PlusPanelView,
  PlusParseOutcome,
  PlusQuota,
  PlusReplyOutcome,
  PlusSaveOutcome,
  PlusSettings,
  PlusThreadPage,
  PlusWeeklyReport,
  Wire,
} from "./types";

export type * from "./types";
export type PlusPanel = PlusPanelView;

export interface PlusApi {
  /** The panel's first look: settings, the mode's catalog, the quota and the approvals. */
  panel(id: string): Promise<PlusPanel>;
  /** The settings as stored now ("Son halini yükle"). */
  settings(id: string): Promise<PlusSettings>;
  catalog(mode: string): Promise<PlusCatalog | null>;
  save(id: string, rev: number, config: PlusConfig): Promise<PlusSaveOutcome>;
  /** "Bu hale dön": the revision `rev` becomes the next (`current`: the one on screen). */
  restore(id: string, rev: number, current: number): Promise<PlusSaveOutcome>;
  control(id: string, action: PlusAction, name?: string): Promise<PlusControlAnswer>;
  feed(id: string, after: string | null): Promise<PlusFeedPage>;
  deals(id: string, after: string | null): Promise<PlusDealPage>;
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
  parse(id: string, text: string): Promise<PlusParseOutcome>;
  history(id: string): Promise<PlusHistory>;
  summary(id: string, from: number | null, to: number | null): Promise<PlusDealSummary>;
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

const tauriPlusApi: PlusApi = {
  panel: (id) => ok(commands.cloudAfkPlusPanel(id)),
  settings: (id) => ok(commands.cloudAfkPlusSettings(id)),
  catalog: (mode) => ok(commands.cloudAfkPlusCatalog(mode)),
  save: (id, rev, config) => ok(commands.cloudAfkPlusSave(id, rev, config)),
  restore: (id, rev, current) => ok(commands.cloudAfkPlusRestore(id, rev, current)),
  control: (id, action, name) => ok(commands.cloudAfkPlusControl(id, action, name ?? null)),
  feed: (id, after) => ok(commands.cloudAfkPlusFeed(id, after)),
  deals: (id, after) => ok(commands.cloudAfkPlusDeals(id, after)),
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
  parse: (id, text) => ok(commands.cloudAfkPlusParse(id, text)),
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
