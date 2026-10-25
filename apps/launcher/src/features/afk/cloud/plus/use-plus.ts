// AFK+'s state on the AFK page: which panel is open and on which section, the panel's first look
// (read when it opens and after an action, never on a timer), the live stream while it is on screen
// (the AI's state, the feed, approvals, new messages), and the writes (settings, the AI's switch,
// approvals). Nothing polls; a panel that leaves the screen stops its stream.
import { toast } from "@pl/ui";
import { type QueryClient, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { afkFailed } from "@/features/afk/errors";
import { useWindowAway } from "@/lib/window-away";
import { type CloudAccount, type CloudLiveEvent, type CloudPageData, cloudApi } from "../api";
import { CLOUD_KEY, useCloudPage } from "../use-cloud";
import {
  type PlusAction,
  type PlusApproval,
  type PlusConfig,
  type PlusFeedItem,
  type PlusPanel,
  type PlusSaveOutcome,
  plusApi,
} from "./api";
import type { SettingsDraft } from "./model";
import { type PlusFeatures, plusMe, plusOffer } from "./types";

export type PlusSection = "feed" | "messages" | "lists" | "settings" | "report";

interface PlusUi {
  /** The account whose AFK+ panel is open. */
  panelId: string | null;
  section: PlusSection;
  /** The conversation "Mesajlar" shows. */
  thread: string | null;
  /** The setup wizard is open for this account. */
  wizardFor: string | null;
  /** A settings group to bring into view ("Sınırları ayarla"). */
  focusGroup: string | null;
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
  setDraft: (id: string, draft: SettingsDraft | null) => void;
  putOffSetup: (id: string) => void;
}

export const usePlusUi = create<PlusUi>()((set) => ({
  panelId: null,
  section: "feed",
  thread: null,
  wizardFor: null,
  focusGroup: null,
  drafts: {},
  setupLater: [],
  openPanel: (panelId, section = "feed") => set({ panelId, section, thread: null }),
  closePanel: () => set({ panelId: null, thread: null, focusGroup: null }),
  setSection: (section) => set({ section, thread: null }),
  setThread: (thread) => set({ thread }),
  setWizard: (wizardFor) => set({ wizardFor }),
  setFocusGroup: (focusGroup) => set({ focusGroup }),
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
const feedKey = (id: string) => ["afk-plus", id, "feed"] as const;
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
 * The QoL parts Studio has switched on (`afkplus.features.*`). Without an offer in the overview
 * (it is always there while the panel shows) everything reads as on: the service refuses what it
 * does not run, in words.
 */
export function usePlusFeatures(): PlusFeatureFlags {
  const { data } = useCloudPage();
  return (
    plusOffer(data?.overview)?.features ?? {
      inbox: true,
      parse: true,
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

export interface FeedState {
  items: PlusFeedItem[];
  /** The cursor of the older lines, or null at the end. */
  next: string | null;
  /** The machine could not be asked. */
  away: boolean;
}

/** Newest first, without repeats (ULIDs sort in time order). */
function mergeFeed(current: PlusFeedItem[], incoming: PlusFeedItem[]): PlusFeedItem[] {
  const byId = new Map(current.map((item) => [item.id, item]));
  for (const item of incoming) byId.set(item.id, item);
  return [...byId.values()].sort((a, b) => b.at - a.at || (a.id < b.id ? 1 : -1));
}

/** The feed: one read when the panel opens, then the live stream, "Daha eski" on demand. */
export function usePlusFeed(id: string, enabled: boolean) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const query = useQuery<FeedState>({
    queryKey: feedKey(id),
    queryFn: async () => {
      try {
        const page = await plusApi.feed(id, null);
        const live = queryClient.getQueryData<FeedState>(feedKey(id))?.items ?? [];
        return { items: mergeFeed(live, page.items), next: page.next, away: false };
      } catch {
        return { items: [], next: null, away: true };
      }
    },
    enabled,
    staleTime: 30_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
  const [loadingOlder, setLoadingOlder] = useState(false);
  const older = async () => {
    const next = query.data?.next;
    if (!next || loadingOlder) return;
    setLoadingOlder(true);
    try {
      const page = await plusApi.feed(id, next);
      queryClient.setQueryData<FeedState>(feedKey(id), (state) =>
        state ? { ...state, items: mergeFeed(state.items, page.items), next: page.next } : state,
      );
    } catch (error) {
      afkFailed(error, t("afkPlus.feed.offline"));
    } finally {
      setLoadingOlder(false);
    }
  };
  return { ...query, older, loadingOlder };
}

export type LiveLink = "connecting" | "live" | "reconnecting" | "off";

/**
 * While the panel is on screen (the window in front), its account is watched with `ai`: the AI's
 * state, feed items, approvals and new messages go into the queries as they come.
 */
export function usePlusLive(id: string | null): LiveLink {
  const queryClient = useQueryClient();
  const away = useWindowAway();
  const [link, setLink] = useState<LiveLink>("off");
  const watching = Boolean(id) && !away;
  const host = useRef<string | null>(null);

  useEffect(() => {
    if (!id || !watching) {
      setLink("off");
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
            setLink(
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
        case "feed":
          if (event.acct === id) {
            // The live event's items are the backend's full values (types.ts).
            const items = event.items as PlusFeedItem[];
            queryClient.setQueryData<FeedState>(feedKey(id), (state) => ({
              items: mergeFeed(state?.items ?? [], items),
              next: state?.next ?? null,
              away: false,
            }));
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
    setLink("connecting");
    plusApi.watch(id).catch(() => setLink("off"));
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
          const rest = config.blocked.filter((n) => n.toLowerCase() !== name.toLowerCase());
          config.blocked = action === "block" ? [...rest, name] : rest;
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
 * made elsewhere meanwhile is read and `change` applied to it once more (list edits, a break).
 */
export function usePlusSave(id: string) {
  const queryClient = useQueryClient();
  return async (change: (config: PlusConfig) => PlusConfig): Promise<PlusSaveOutcome> => {
    const panel = queryClient.getQueryData<PlusPanel>(plusKey(id));
    if (!panel) throw new Error("no AFK+ settings yet");
    let outcome = await plusApi.save(id, panel.settings.aiRev, change(panel.settings.config));
    if (outcome.kind === "conflict") {
      const latest = outcome.settings;
      outcome = await plusApi.save(id, latest.aiRev, change(latest.config));
    }
    if (outcome.kind === "saved" || outcome.kind === "conflict") {
      const settings = outcome.settings;
      queryClient.setQueryData<PlusPanel>(plusKey(id), (current) =>
        current ? { ...current, settings } : current,
      );
    }
    return outcome;
  };
}

/**
 * A change saved at once (a rule added, edited, switched or removed; a break): shown at once, then
 * sent on top of the latest settings. `change` must give the same result when it runs twice (it
 * runs on the shown copy and again on what is sent). A failure reads the stored settings back, so
 * two quick edits never undo each other.
 */
export function usePlusInstantSave(id: string) {
  const queryClient = useQueryClient();
  const save = usePlusSave(id);
  const { t } = useTranslation();
  const restore = async () => {
    try {
      const settings = await plusApi.settings(id);
      queryClient.setQueryData<PlusPanel>(plusKey(id), (panel) =>
        panel ? { ...panel, settings } : panel,
      );
    } catch {
      void queryClient.invalidateQueries({ queryKey: plusKey(id) });
    }
  };
  return async (
    change: (config: PlusConfig) => PlusConfig,
    failed = "afkPlus.lists.failed",
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
