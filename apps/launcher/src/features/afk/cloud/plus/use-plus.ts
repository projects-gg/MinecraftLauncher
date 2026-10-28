// AFK+'s state on the AFK page: which panel is open and on which section, the panel's first look
// (read when it opens and after an action, never on a timer), the live stream while it is on screen
// (the AI's state, new audit log entries, the orders' progress, approvals, new messages), and the
// writes (settings, orders, the AI's switch, approvals). Nothing polls; a panel that leaves the
// screen stops its stream.
import { toast } from "@pl/ui";
import { type QueryClient, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { afkFailed } from "@/features/afk/errors";
import { useWindowAway } from "@/lib/window-away";
import { type CloudAccount, type CloudLiveEvent, type CloudPageData, cloudApi } from "../api";
import { CLOUD_KEY, useCloudPage } from "../use-cloud";
import {
  type PlusAction,
  type PlusApproval,
  type PlusAuditEntry,
  type PlusConfig,
  type PlusOrderLive,
  type PlusPanel,
  type PlusSaveOutcome,
  type PlusSettings,
  plusApi,
} from "./api";
import type { LimitField, SettingsDraft } from "./model";
import { type PlusFeatures, plusMe, plusOffer } from "./types";

export type PlusSection = "feed" | "messages" | "orders" | "settings" | "report";

interface PlusUi {
  /** The account whose AFK+ panel is open. */
  panelId: string | null;
  section: PlusSection;
  /** The conversation "Mesajlar" shows. */
  thread: string | null;
  /** The setup wizard is open for this account. */
  wizardFor: string | null;
  /** A settings group to bring into view, and the field in it to focus ("Sınırlara git"). */
  focusGroup: string | null;
  focusField: string | null;
  /**
   * Unsaved "Ayarlar" edits per account: they wait where they were made, across sections and
   * the panel closing (design.md G6-P), until saved or dropped with "Vazgeç".
   */
  drafts: Record<string, SettingsDraft>;
  /** Accounts whose setup the player put off this session ("Sonra"). */
  setupLater: string[];
  openPanel: (id: string, section?: PlusSection) => void;
  closePanel: () => void;
  setSection: (section: PlusSection) => void;
  setThread: (name: string | null) => void;
  setWizard: (id: string | null) => void;
  setFocusGroup: (group: string | null) => void;
  /** "Sınırlara git": "Ayarlar" at that limit, its field in view and focused. */
  goToLimit: (field: LimitField) => void;
  setDraft: (id: string, draft: SettingsDraft | null) => void;
  putOffSetup: (id: string) => void;
}

export const usePlusUi = create<PlusUi>()((set) => ({
  panelId: null,
  section: "feed",
  thread: null,
  wizardFor: null,
  focusGroup: null,
  focusField: null,
  drafts: {},
  setupLater: [],
  openPanel: (panelId, section = "feed") => set({ panelId, section, thread: null }),
  closePanel: () => set({ panelId: null, thread: null, focusGroup: null, focusField: null }),
  setSection: (section) => set({ section, thread: null }),
  setThread: (thread) => set({ thread }),
  setWizard: (wizardFor) => set({ wizardFor }),
  setFocusGroup: (focusGroup) => set({ focusGroup, focusField: null }),
  goToLimit: (focusField) =>
    set({ section: "settings", thread: null, focusGroup: "limits", focusField }),
  setDraft: (id, draft) =>
    set((state) => {
      const drafts = { ...state.drafts };
      if (draft) drafts[id] = draft;
      else delete drafts[id];
      return { drafts };
    }),
  putOffSetup: (id) =>
    set((state) => ({
      setupLater: state.setupLater.includes(id) ? state.setupLater : [...state.setupLater, id],
    })),
}));

export const plusKey = (id: string) => ["afk-plus", id] as const;
const auditKey = (id: string) => ["afk-plus", id, "audit"] as const;
export const inboxKey = (id: string) => ["afk-plus", id, "inbox"] as const;
export const threadKey = (id: string, name: string) => ["afk-plus", id, "thread", name] as const;

/** Whether AFK+ shows at all, and for this player (README "off means invisible"; an invite list). */
export function plusShown(data: CloudPageData | undefined): boolean {
  const overview = data?.overview;
  const offer = plusOffer(overview);
  if (!overview?.available || !offer) return false;
  if (offer.mode !== "invite") return true;
  // Invited players, and those with an AFK+ account, see it; nobody else hears of it.
  return Boolean(
    plusMe(overview)?.canBuy.ok || overview.accounts.some((account) => account.plusUntil !== null),
  );
}

export type PlusFeatureFlags = PlusFeatures;

/**
 * The parts Studio has switched on (`afkplus.features.*`). Without an offer in the overview (it is
 * always there while the panel shows) everything reads as on but swaps and other servers: the
 * service refuses what it does not run, in words.
 */
export function usePlusFeatures(): PlusFeatureFlags {
  const { data } = useCloudPage();
  return (
    plusOffer(data?.overview)?.features ?? {
      inbox: true,
      orders: true,
      audit: true,
      customItems: true,
      itemPay: false,
      anyServer: false,
      schedule: true,
      report: true,
      history: true,
      saver: true,
      notifyDigest: false,
      handoff: false,
      watch: false,
    }
  );
}

/**
 * AFK+ extra credits on sale now (cloud-afk README "AFK+ extra credits"): the rail's "Kontör al"
 * opens the site's packs. Null while Studio has them off or nothing can be bought this moment.
 */
export function usePlusTopupOpen(): boolean {
  const { data } = useCloudPage();
  const topup = plusOffer(data?.overview)?.topup ?? null;
  return Boolean(topup?.sales && topup.packs.length > 0);
}

/** The account has AFK+ now, or had it (its settings and history stay readable). */
export function isPlusAccount(account: CloudAccount | null | undefined): boolean {
  return Boolean(account && (account.tier === "plus" || account.plusUntil !== null));
}

/** Puts an account's AI state into the overview (a live `ai`, a control's answer). */
export function storeAi(
  queryClient: QueryClient,
  id: string,
  ai: CloudAccount["ai"],
  keep: Array<keyof NonNullable<CloudAccount["ai"]>> = ["spent", "newMessages"],
) {
  queryClient.setQueryData<CloudPageData>(CLOUD_KEY, (data) =>
    data
      ? {
          ...data,
          overview: {
            ...data.overview,
            accounts: data.overview.accounts.map((account) => {
              if (account.id !== id) return account;
              if (!ai || !account.ai) return { ...account, ai };
              // A live state carries no spent total or new messages: the last known ones stay.
              const kept = { ...ai };
              for (const key of keep) {
                if (kept[key] === null || kept[key] === undefined) {
                  (kept as Record<string, unknown>)[key] = account.ai[key];
                }
              }
              return { ...account, ai: kept };
            }),
          },
        }
      : data,
  );
}

/** What is left unread after the player read a conversation (the card and the rail count it). */
export function storeUnread(queryClient: QueryClient, id: string, count: number) {
  queryClient.setQueryData<CloudPageData>(CLOUD_KEY, (data) =>
    data
      ? {
          ...data,
          overview: {
            ...data.overview,
            accounts: data.overview.accounts.map((account) =>
              account.id === id && account.ai
                ? {
                    ...account,
                    ai: {
                      ...account.ai,
                      newMessages:
                        count > 0
                          ? {
                              count,
                              at: account.ai.newMessages?.at ?? Math.floor(Date.now() / 1000),
                            }
                          : null,
                    },
                  }
                : account,
            ),
          },
        }
      : data,
  );
}

/** Pending approvals first (the newest first), then the answered ones. */
function sortApprovals(list: PlusApproval[]): PlusApproval[] {
  return [...list].sort(
    (a, b) =>
      Number(b.state === "pending") - Number(a.state === "pending") || b.createdAt - a.createdAt,
  );
}

export function storeApproval(queryClient: QueryClient, id: string, approval: PlusApproval) {
  queryClient.setQueryData<PlusPanel>(plusKey(id), (panel) =>
    panel
      ? {
          ...panel,
          approvals: sortApprovals([
            approval,
            ...panel.approvals.filter((a) => a.id !== approval.id),
          ]),
        }
      : panel,
  );
}

/**
 * Puts written settings into the panel (an order write, a save) as the backend merged them: a
 * save's server view is the service's answer (its money name), else the one read; the rules stay.
 */
export function storeSettings(queryClient: QueryClient, id: string, settings: PlusSettings) {
  queryClient.setQueryData<PlusPanel>(plusKey(id), (panel) => {
    if (!panel) return panel;
    // The orders' states the panel knows are the live ones (`aiord`); the write's answer carries
    // those read when the panel opened, so it only adds a new order's.
    const orders = { ...panel.settings.orders };
    for (const key of Object.keys(orders)) {
      if (!settings.config.orders.some((order) => order.id === key)) delete orders[key];
    }
    return { ...panel, settings: { ...settings, orders: { ...settings.orders, ...orders } } };
  });
}

/** The live `aiord`: orders whose progress or live state changed. */
function storeOrders(queryClient: QueryClient, id: string, live: Record<string, PlusOrderLive>) {
  queryClient.setQueryData<PlusPanel>(plusKey(id), (panel) => {
    if (!panel) return panel;
    const orders = { ...panel.settings.orders };
    for (const [key, value] of Object.entries(live)) {
      // The status is derived where it is shown (statusOf); the stored word is the last read one.
      orders[key] = { status: orders[key]?.status ?? "looking", ...value };
    }
    return { ...panel, settings: { ...panel.settings, orders } };
  });
}

/** The panel's first look; read again when it opens. */
export function usePlusPanel(id: string | null) {
  return useQuery({
    queryKey: plusKey(id ?? ""),
    queryFn: () => plusApi.panel(id ?? ""),
    enabled: Boolean(id),
    staleTime: 0,
    gcTime: 60_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
}

// --- the audit log ("Akış") -------------------------------------------------------------------------

export interface AuditState {
  /** Newest first, each once (by its sequence). */
  items: PlusAuditEntry[];
  /** Older entries are left on the machine. */
  more: boolean;
  /** The machine could not be asked: these are what this computer kept. */
  away: boolean;
  /** Studio switched the log off. */
  off: boolean;
}

/** Newest first, without repeats. */
export function mergeAudit(
  current: PlusAuditEntry[],
  incoming: PlusAuditEntry[],
): PlusAuditEntry[] {
  const bySeq = new Map(current.map((entry) => [entry.s, entry]));
  for (const entry of incoming) bySeq.set(entry.s, entry);
  return [...bySeq.values()].sort((a, b) => b.s - a.s);
}

/**
 * The audit log: what this computer kept shows at once, then the machine's newer entries past the
 * last one kept, then the live stream; "Daha eski" reads older pages on demand.
 */
export function usePlusAudit(id: string, enabled: boolean) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const query = useQuery<AuditState>({
    queryKey: auditKey(id),
    queryFn: async () => {
      const kept = await plusApi.auditKept(id).catch(() => [] as PlusAuditEntry[]);
      const newest = kept[0]?.s ?? null;
      const outcome = await plusApi.audit(id, null, newest, null).catch(() => null);
      const live = queryClient.getQueryData<AuditState>(auditKey(id))?.items ?? [];
      if (!outcome || outcome.kind === "kept") {
        const items = outcome?.kind === "kept" ? outcome.items : kept;
        return { items: mergeAudit(live, items), more: false, away: true, off: false };
      }
      if (outcome.kind === "off") return { items: [], more: false, away: false, off: true };
      if (outcome.kind === "limited") {
        return { items: mergeAudit(live, kept), more: false, away: false, off: false };
      }
      if (newest === null) {
        return {
          items: mergeAudit(live, outcome.items),
          more: outcome.more,
          away: false,
          off: false,
        };
      }
      // Past the newest entry kept: with more between than one page, the kept ones are older than
      // a gap, so the log goes on from the page (they stay on disk); without, they follow the page
      // and the machine may still have older ones ("Daha eski" asks).
      if (outcome.more) {
        return { items: mergeAudit(live, outcome.items), more: true, away: false, off: false };
      }
      return {
        items: mergeAudit(mergeAudit(live, kept), outcome.items),
        more: true,
        away: false,
        off: false,
      };
    },
    enabled,
    staleTime: 30_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
  const [loadingOlder, setLoadingOlder] = useState(false);
  const older = async () => {
    const items = query.data?.items ?? [];
    const oldest = items[items.length - 1]?.s ?? null;
    if (loadingOlder) return;
    setLoadingOlder(true);
    try {
      const outcome = await plusApi.audit(id, oldest, null, null);
      if (outcome.kind === "page") {
        queryClient.setQueryData<AuditState>(auditKey(id), (state) =>
          state
            ? { ...state, items: mergeAudit(state.items, outcome.items), more: outcome.more }
            : state,
        );
      } else if (outcome.kind === "kept") {
        queryClient.setQueryData<AuditState>(auditKey(id), (state) =>
          state ? { ...state, away: true, more: false } : state,
        );
      }
    } catch (error) {
      afkFailed(error, t("afkPlus.audit.offline"));
    } finally {
      setLoadingOlder(false);
    }
  };
  return { ...query, older, loadingOlder };
}

export type LiveLink = "connecting" | "live" | "reconnecting" | "off";

/**
 * While the panel is on screen (the window in front), its account is watched with `ai`: the AI's
 * state, audit log entries, the orders' progress, approvals and new messages go into the queries as
 * they come.
 */
export function usePlusLive(id: string | null): LiveLink {
  const queryClient = useQueryClient();
  const away = useWindowAway();
  const [link, setLink] = useState<LiveLink>("off");
  const watching = Boolean(id) && !away;
  const host = useRef<string | null>(null);
  // The link as last set, and whether it was live before: the stream carries only what happens
  // while it is open, so a link live again (a reconnect, the window back) reads the log past the
  // last entry kept and the orders' progress again.
  const last = useRef<LiveLink>("off");
  const wasLive = useRef(false);
  const forId = useRef<string | null>(null);

  useEffect(() => {
    if (forId.current !== id) {
      forId.current = id;
      wasLive.current = false;
    }
    const update = (next: LiveLink) => {
      if (next === "live" && last.current !== "live" && id) {
        if (wasLive.current) {
          void queryClient.invalidateQueries({ queryKey: auditKey(id) });
          void queryClient.invalidateQueries({ queryKey: plusKey(id) });
        }
        wasLive.current = true;
      }
      last.current = next;
      setLink(next);
    };
    if (!id || !watching) {
      update("off");
      return;
    }
    const account = queryClient
      .getQueryData<CloudPageData>(CLOUD_KEY)
      ?.overview.accounts.find((a) => a.id === id);
    host.current = account?.host ?? null;
    const stop = cloudApi.onLive((event: CloudLiveEvent) => {
      switch (event.kind) {
        case "status":
          if (event.host === host.current) {
            update(
              event.status === "live"
                ? "live"
                : event.status === "connecting"
                  ? "connecting"
                  : event.status === "reconnecting"
                    ? "reconnecting"
                    : "off",
            );
          }
          break;
        case "ai":
          if (event.acct === id) storeAi(queryClient, id, event.ai);
          break;
        case "audit":
          if (event.acct === id) {
            const items = event.items as PlusAuditEntry[];
            // Only into a log already read: one made of streamed lines alone would stand for the
            // whole log (no backlog, no "Daha eski"); kept on disk, they come with the next read.
            queryClient.setQueryData<AuditState>(auditKey(id), (state) =>
              state ? { ...state, items: mergeAudit(state.items, items) } : state,
            );
            void plusApi.auditKeep(id, items).catch(() => undefined);
          }
          break;
        case "orders":
          if (event.acct === id) {
            storeOrders(queryClient, id, event.orders as Record<string, PlusOrderLive>);
          }
          break;
        case "approval":
          if (event.acct === id) storeApproval(queryClient, id, event.approval as PlusApproval);
          break;
        case "messages":
          if (event.acct === id) void queryClient.invalidateQueries({ queryKey: inboxKey(id) });
          break;
        default:
          break;
      }
    });
    update("connecting");
    plusApi.watch(id).catch(() => update("off"));
    return () => {
      stop();
      void plusApi.unwatch(id);
    };
  }, [id, watching, queryClient]);

  return link;
}

/** The AI's switch and the per-player controls; the answer goes where the page reads it. */
export function usePlusControl(id: string) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return async (action: PlusAction, name?: string): Promise<boolean> => {
    try {
      const answer = await plusApi.control(id, action, name);
      storeAi(queryClient, id, answer.ai);
      queryClient.setQueryData<PlusPanel>(plusKey(id), (panel) => {
        if (!panel) return panel;
        const config = { ...panel.settings.config };
        if (action === "pause" || action === "resume") config.enabled = action === "resume";
        if ((action === "block" || action === "unblock") && name) {
          const other = (n: string) => n.toLowerCase() !== name.toLowerCase();
          const rest = config.blocked.filter(other);
          config.blocked = action === "block" ? [...rest, name] : rest;
          // The service takes a blocked name off the friends list too (a save would be refused).
          if (action === "block")
            config.who = { ...config.who, friends: config.who.friends.filter(other) };
        }
        const spent =
          action === "reset_spent" && panel.settings.spent
            ? { ...panel.settings.spent, total: 0 }
            : panel.settings.spent;
        return { ...panel, settings: { ...panel.settings, config, aiRev: answer.aiRev, spent } };
      });
      return true;
    } catch (error) {
      afkFailed(error, t("afkPlus.control.failed"));
      return false;
    }
  };
}

/**
 * Saves a change made on the settings as stored now: `change` gets the latest config. A change
 * made elsewhere meanwhile is read and `change` applied to it once more (a break, a friend added).
 */
export function usePlusSave(id: string) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return async (change: (config: PlusConfig) => PlusConfig): Promise<PlusSaveOutcome> => {
    const panel = queryClient.getQueryData<PlusPanel>(plusKey(id));
    if (!panel) throw new Error("no AFK+ settings yet");
    // With paying by items switched off (Studio), a stored "items" or "both" would be refused at
    // every save while its control is hidden: it goes as money.
    const toSend = (settings: PlusSettings) => {
      const config = change(settings.config);
      return settings.rules.features.itemPay || config.pay === "money"
        ? config
        : { ...config, pay: "money" };
    };
    let outcome = await plusApi.save(id, panel.settings.aiRev, toSend(panel.settings));
    if (outcome.kind === "conflict") {
      const latest = outcome.settings;
      outcome = await plusApi.save(id, latest.aiRev, toSend(latest));
    }
    if (outcome.kind === "saved" || outcome.kind === "conflict") {
      storeSettings(queryClient, id, outcome.settings);
    }
    if (outcome.kind === "conflict") {
      // Changed elsewhere again meanwhile: the newest shows, nothing of this was saved.
      throw Object.assign(new Error(t("afkPlus.settings.changedElsewhere")), {
        details: "AFK+ settings: rev_conflict twice",
      });
    }
    return outcome;
  };
}

/**
 * A settings change saved at once (a break, a friend added, a switch of "İnce ayar"): shown at
 * once, then sent on top of the latest settings. `change` must give the same result when it runs
 * twice (it runs on the shown copy and again on what is sent). A failure reads the stored settings
 * back, so two quick edits never undo each other.
 */
export function usePlusInstantSave(id: string) {
  const queryClient = useQueryClient();
  const save = usePlusSave(id);
  const { t } = useTranslation();
  const restore = useCallback(async () => {
    try {
      const settings = await plusApi.settings(id);
      storeSettings(queryClient, id, settings);
    } catch {
      void queryClient.invalidateQueries({ queryKey: plusKey(id) });
    }
  }, [id, queryClient]);
  return async (
    change: (config: PlusConfig) => PlusConfig,
    failed = "afkPlus.settings.saveFailed",
  ): Promise<boolean> => {
    const panel = queryClient.getQueryData<PlusPanel>(plusKey(id));
    if (!panel) return false;
    queryClient.setQueryData<PlusPanel>(plusKey(id), {
      ...panel,
      settings: { ...panel.settings, config: change(panel.settings.config) },
    });
    try {
      const outcome = await save(change);
      if (outcome.kind === "invalid") {
        await restore();
        toast.error(t(failed));
        return false;
      }
      return true;
    } catch (error) {
      await restore();
      afkFailed(error, t(failed));
      return false;
    }
  };
}
