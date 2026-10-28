// 7/24 Bulut's state on the AFK page: the overview (read when the page opens, after an action and
// when the friends session changes; never on a timer), the cloud twins of desktop accounts, the
// layers that are open, and the two-places guard.
import { toast } from "@pl/ui";
import { type QueryClient, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { afkFailed } from "@/features/afk/errors";
import { useSocialUi } from "@/features/social/store";
import { type AfkAccount, api } from "@/lib/api";
import { type CloudAccount, type CloudPageData, cloudApi } from "./api";
import { foldedIds } from "./twins";

export const CLOUD_KEY = ["cloud-afk"] as const;

/**
 * Signing in or out of Friends changes what the service shows: the overview is asked again then
 * (once, by the AFK page's 7/24 Bulut section; never on a timer).
 */
export function useCloudSync() {
  const queryClient = useQueryClient();
  const auth = useSocialUi((s) => s.view.auth);
  const enabled = useSocialUi((s) => s.view.enabled);
  const seen = useRef<string | null>(null);
  useEffect(() => {
    const now = `${enabled}|${auth}`;
    // The first look is the query's own; a "restoring" in between changes nothing yet.
    if (seen.current !== null && seen.current !== now && auth !== "restoring") {
      void queryClient.invalidateQueries({ queryKey: CLOUD_KEY });
    }
    seen.current = now;
  }, [auth, enabled, queryClient]);
}

/** The overview; `skewMs` is the service's clock minus this computer's (for the state lines). */
export function useCloudPage() {
  const query = useQuery({
    queryKey: CLOUD_KEY,
    queryFn: () => cloudApi.overview(),
    staleTime: 15_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
  const data = query.data;
  const skewMs = data?.overview.now ? data.overview.now * 1000 - query.dataUpdatedAt : 0;
  return { ...query, skewMs };
}

/** The cloud account with a desktop account's name on its server, if 7/24 Bulut shows. */
export function twinOf(data: CloudPageData | undefined, desktopId: string): CloudAccount | null {
  if (!data?.overview.available) return null;
  const twin = data.twins.find((t) => t.desktopId === desktopId);
  return twin ? (data.overview.accounts.find((a) => a.id === twin.cloudId) ?? null) : null;
}

export function useCloudTwin(desktopId: string): CloudAccount | null {
  const { data } = useCloudPage();
  return twinOf(data, desktopId);
}

/** Puts an account the service answered with into the overview (no extra read). */
export function storeAccount(queryClient: QueryClient, account: CloudAccount) {
  queryClient.setQueryData<CloudPageData>(CLOUD_KEY, (data) =>
    data
      ? {
          ...data,
          overview: {
            ...data.overview,
            accounts: data.overview.accounts.map((a) => (a.id === account.id ? account : a)),
          },
        }
      : data,
  );
}

/** The cloud twin wants to run there now (stopping it first makes sense). */
export function runsInCloud(account: CloudAccount | null): account is CloudAccount {
  return Boolean(account && account.desired === "run" && !account.hold && !account.expired);
}

/** A desktop account is busy here (anything but idle). */
function activeHere(account: AfkAccount): boolean {
  return !["stopped", "error", "clientFailed"].includes(account.state.kind);
}

type GuardRequest =
  | { where: "cloud"; desktop: AfkAccount; cloud: CloudAccount }
  | { where: "here"; desktop: AfkAccount; cloud: CloudAccount }
  /** "Bu bilgisayarda çalıştır" while the account runs in the cloud (AFK-CLEAR). */
  | { where: "move"; desktop: AfkAccount; cloud: CloudAccount };

/** The account panel's tabs (the inventory's only while the service offers it). */
export type PanelTab = "console" | "inventory" | "movement";

interface CloudUi {
  consoleId: string | null;
  /** The panel's tab: the card's "Envanter" opens it on the inventory. */
  consoleTab: PanelTab;
  settingsId: string | null;
  /** "Buluta taşı" is open for this desktop account. */
  moveFor: string | null;
  introOpen: boolean;
  guard: GuardRequest | null;
  openConsole: (id: string | null, tab?: PanelTab) => void;
  setConsoleTab: (tab: PanelTab) => void;
  openSettings: (id: string | null) => void;
  openMove: (desktopId: string | null) => void;
  setIntroOpen: (open: boolean) => void;
  setGuard: (guard: GuardRequest | null) => void;
}

export const useCloudUi = create<CloudUi>()((set) => ({
  consoleId: null,
  consoleTab: "console",
  settingsId: null,
  moveFor: null,
  introOpen: false,
  guard: null,
  openConsole: (consoleId, consoleTab = "console") => set({ consoleId, consoleTab }),
  setConsoleTab: (consoleTab) => set({ consoleTab }),
  openSettings: (settingsId) => set({ settingsId }),
  openMove: (moveFor) => set({ moveFor }),
  setIntroOpen: (introOpen) => set({ introOpen }),
  setGuard: (guard) => set({ guard }),
}));

/**
 * "Bağla" on a desktop account: asks first when the same account runs in the cloud ("Bu hesap
 * bulutta çalışıyor."), since two clients with one name on one server keep kicking each other.
 */
export function useGuardedConnect() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return (account: AfkAccount) => {
    const twin = twinOf(queryClient.getQueryData<CloudPageData>(CLOUD_KEY), account.id);
    if (runsInCloud(twin)) {
      useCloudUi.getState().setGuard({ where: "cloud", desktop: account, cloud: twin });
      return;
    }
    api.afkConnect(account.id).catch((error: unknown) => afkFailed(error, t("afk.connectFailed")));
  };
}

/**
 * Starts a cloud account, or asks first (the two-places question) while the same account runs on
 * this computer; true once it started. Says nothing itself: the caller's toast does ("Kaydet ve
 * başlat" says it with the save's).
 */
export async function startOrAsk(
  queryClient: QueryClient,
  account: CloudAccount,
): Promise<boolean> {
  const data = queryClient.getQueryData<CloudPageData>(CLOUD_KEY);
  const twin = data?.twins.find((t) => t.cloudId === account.id);
  const desktop = queryClient.getQueryData<AfkAccount[]>(["afk"]) ?? [];
  const here = twin ? desktop.find((d) => d.id === twin.desktopId) : undefined;
  if (here && activeHere(here)) {
    useCloudUi.getState().setGuard({ where: "here", desktop: here, cloud: account });
    return false;
  }
  storeAccount(queryClient, await cloudApi.start(account.id));
  return true;
}

/** "Başlat" on a cloud account: asks first when the same account runs on this computer. */
export function useGuardedStart() {
  const queryClient = useQueryClient();
  const start = useCloudAction("start");
  return async (account: CloudAccount, desktop: AfkAccount[]) => {
    const data = queryClient.getQueryData<CloudPageData>(CLOUD_KEY);
    const twin = data?.twins.find((t) => t.cloudId === account.id);
    const here = twin ? desktop.find((d) => d.id === twin.desktopId) : undefined;
    if (here && activeHere(here)) {
      useCloudUi.getState().setGuard({ where: "here", desktop: here, cloud: account });
      return;
    }
    await start(account);
  };
}

/**
 * "Tümünü bağla" of the "Bu bilgisayarda" list: the accounts it shows. An account that is also a
 * 7/24 Bulut account has its card in the cloud list (AFK-CLEAR), so it is left out whatever it does
 * there (the backend's `cloud_afk_connect_here` leaves out every twin).
 */
export function useConnectAll() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return () => {
    const data = queryClient.getQueryData<CloudPageData>(CLOUD_KEY);
    const connect = foldedIds(data).size > 0 ? cloudApi.connectHere() : api.afkConnectAll();
    connect.catch((error: unknown) => afkFailed(error, t("afk.connectFailed")));
  };
}

/**
 * "Bu bilgisayarda çalıştır" on the card of an account in both places: this computer connects. While
 * the cloud one runs it asks first (the move dialog: it stops in the cloud, its time keeps running,
 * it leaves the game when this computer does), then the cloud one stops (AFK-CLEAR).
 */
export function useRunHere() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return async (cloud: CloudAccount, here: AfkAccount) => {
    if (runsInCloud(cloud)) {
      useCloudUi.getState().setGuard({ where: "move", desktop: here, cloud });
      return;
    }
    try {
      await api.afkConnect(here.id);
      toast.success(t("cloudAfk.location.toHere", { name: here.nickname }));
    } catch (error) {
      afkFailed(error, t("cloudAfk.actions.failed"));
      void queryClient.invalidateQueries({ queryKey: CLOUD_KEY });
    }
  };
}

/** "Bulutta çalıştır": the copy here stops if it is busy, then the cloud one starts. */
export function useRunInCloud() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return async (cloud: CloudAccount, here: AfkAccount) => {
    try {
      if (activeHere(here)) await api.afkStop(here.id);
      storeAccount(queryClient, await cloudApi.start(cloud.id));
      toast.success(t("cloudAfk.location.toCloud", { name: cloud.name }));
    } catch (error) {
      afkFailed(error, t("cloudAfk.actions.failed"));
      void queryClient.invalidateQueries({ queryKey: CLOUD_KEY });
    }
  };
}

type Action = "start" | "stop" | "restart";

/** One action on a cloud account: the answer goes into the overview; a failure is a toast. */
export function useCloudAction(action: Action) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return async (account: CloudAccount) => {
    try {
      const next = await cloudApi[action](account.id);
      storeAccount(queryClient, next);
      const done = { start: "started", stop: "stopped", restart: "reconnecting" } as const;
      toast.success(t(`cloudAfk.actions.${done[action]}`, { name: account.name }));
    } catch (error) {
      afkFailed(error, t("cloudAfk.actions.failed"));
      // What the service holds now may differ from what was shown.
      void queryClient.invalidateQueries({ queryKey: CLOUD_KEY });
    }
  };
}
