// Account data and the actions every account surface shares (center, switcher, palette,
// Settings): optimistic switching, undoable removal, heads, session checks, AFK sessions.
import { toast } from "@pl/ui";
import { type QueryClient, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { type AccountSummary, api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { pinnedIds, reorderPinnedInList, setPinnedInList, sortAccounts } from "./account-order";
import { createRemovalQueue, pickNextActive } from "./removal-queue";
import { useAccountsUi } from "./store";
import { showUndoToast } from "./undo-toast";

export const ACCOUNTS_KEY = ["accounts"] as const;

/** Every account in display order (pinned first, then active, then by last play). */
export function useAccounts() {
  return useQuery({ queryKey: ACCOUNTS_KEY, queryFn: () => api.accounts(), select: sortAccounts });
}

/** A failed command: its friendly message (never a code) and the details for support. */
export interface FriendlyError {
  message: string;
  details: string | null;
  code: string;
}

export function friendlyError(error: unknown, fallback: string): FriendlyError {
  if (error instanceof Error && "details" in error && typeof error.details === "string") {
    const code = "code" in error && typeof error.code === "string" ? error.code : "";
    return { message: error.message, details: `${code}\n${error.details}`.trim(), code };
  }
  // Anything else (IPC failure, programming error) is not meant for the player's eyes.
  return {
    message: fallback,
    details: error instanceof Error ? error.message : String(error),
    code: "",
  };
}

// --- screen reader announcements -----------------------------------------------------------

type Listener = (text: string) => void;
const announceListeners = new Set<Listener>();

/** Says `text` through the polite live region of the account host. */
export function announce(text: string) {
  for (const listener of announceListeners) listener(text);
}

export function onAnnounce(listener: Listener) {
  announceListeners.add(listener);
  return () => {
    announceListeners.delete(listener);
  };
}

// --- switching -------------------------------------------------------------------------------

function setList(queryClient: QueryClient, update: (list: AccountSummary[]) => AccountSummary[]) {
  queryClient.setQueryData<AccountSummary[]>(ACCOUNTS_KEY, (list) => (list ? update(list) : list));
}

/** Makes `account` the one OYNA uses. The selection moves at once and rolls back on failure. */
export async function switchAccount(
  queryClient: QueryClient,
  account: AccountSummary,
  options: { notify?: boolean } = {},
) {
  if (account.active) return;
  const t = i18n.t.bind(i18n);
  await queryClient.cancelQueries({ queryKey: ACCOUNTS_KEY });
  const previous = queryClient.getQueryData<AccountSummary[]>(ACCOUNTS_KEY);
  setList(queryClient, (list) => list.map((a) => ({ ...a, active: a.id === account.id })));
  try {
    await api.setActiveAccount(account.id);
    const text = t("accounts.center.switched", { name: account.username });
    if (options.notify) toast.success(text);
    else announce(text);
  } catch (error) {
    queryClient.setQueryData(ACCOUNTS_KEY, previous);
    toast.error(friendlyError(error, t("accounts.center.switchFailed")).message);
  } finally {
    void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
  }
}

export function useSwitchAccount() {
  const queryClient = useQueryClient();
  return (account: AccountSummary, options?: { notify?: boolean }) =>
    switchAccount(queryClient, account, options);
}

// --- removal with undo -----------------------------------------------------------------------

interface RemovedEntry {
  account: AccountSummary;
  index: number;
  toastId: string | number;
}

const removals = createRemovalQueue<RemovedEntry>((id) => {
  void api.forgetRemovedAccount(id).catch(() => {});
});

/**
 * Removes an account with no confirmation: it leaves the list at once and an undo toast keeps
 * it restorable for a few seconds. Microsoft sign-ins leave this computer's keychain right away.
 */
export async function removeAccount(queryClient: QueryClient, account: AccountSummary) {
  const t = i18n.t.bind(i18n);
  await queryClient.cancelQueries({ queryKey: ACCOUNTS_KEY });
  const list = queryClient.getQueryData<AccountSummary[]>(ACCOUNTS_KEY) ?? [];
  const index = Math.max(
    0,
    list.findIndex((a) => a.id === account.id),
  );
  const nextActive = pickNextActive(list, account.id);
  setList(queryClient, (current) =>
    current.filter((a) => a.id !== account.id).map((a) => ({ ...a, active: a.id === nextActive })),
  );
  try {
    await api.removeAccount(account.id);
  } catch (error) {
    queryClient.setQueryData(ACCOUNTS_KEY, list);
    toast.error(friendlyError(error, t("accounts.center.removeFailed")).message);
    return;
  } finally {
    void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
  }
  const toastId = showUndoToast(account, {
    onUndo: () => void undoRemoval(queryClient, account.id),
    onClose: () => removals.settle(account.id),
  });
  removals.add(account.id, { account, index, toastId });
  const next = nextActive ? list.find((a) => a.id === nextActive) : null;
  announce(
    next && account.active
      ? t("accounts.center.removedNowActive", { name: account.username, next: next.username })
      : t("accounts.center.removedAnnounce", { name: account.username }),
  );
}

/** Brings back a removal whose undo window is still open. */
export async function undoRemoval(queryClient: QueryClient, id: string) {
  const entry = removals.undo(id);
  if (!entry) return false;
  const t = i18n.t.bind(i18n);
  toastDismiss(entry.toastId);
  const { account, index } = entry;
  setList(queryClient, (list) => {
    const rest = list.filter((a) => a.id !== account.id);
    const back = account.active ? rest.map((a) => ({ ...a, active: false })) : rest;
    back.splice(Math.min(index, back.length), 0, account);
    return back;
  });
  useAccountsUi.getState().markFresh(account.id);
  try {
    await api.undoRemoveAccount(account.id);
    announce(t("accounts.center.restored", { name: account.username }));
  } catch (error) {
    toast.error(friendlyError(error, t("accounts.center.undoFailed")).message);
  } finally {
    void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
  }
  return true;
}

/** Ctrl+Z in the account center: undoes the most recent removal still in its window. */
export function undoLatestRemoval(queryClient: QueryClient) {
  const id = removals.latest();
  return id ? undoRemoval(queryClient, id) : Promise.resolve(false);
}

function toastDismiss(id: string | number) {
  toast.dismiss(id);
}

// --- pins, pinned order, pictures ---------------------------------------------------------------

/** Changes the saved list at once, then the backend; rolls back and says so on failure. */
async function optimistic(
  queryClient: QueryClient,
  update: (list: AccountSummary[]) => AccountSummary[],
  run: () => Promise<unknown>,
  failed: string,
) {
  await queryClient.cancelQueries({ queryKey: ACCOUNTS_KEY });
  const previous = queryClient.getQueryData<AccountSummary[]>(ACCOUNTS_KEY);
  setList(queryClient, update);
  try {
    await run();
    return true;
  } catch (error) {
    queryClient.setQueryData(ACCOUNTS_KEY, previous);
    toast.error(friendlyError(error, failed).message);
    return false;
  } finally {
    void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
  }
}

/** Pins `account` (it joins the end of the pinned ones) or unpins it. */
export async function pinAccount(
  queryClient: QueryClient,
  account: AccountSummary,
  pinned: boolean,
) {
  const t = i18n.t.bind(i18n);
  const done = await optimistic(
    queryClient,
    (list) => setPinnedInList(list, account.id, pinned),
    () => api.setAccountPinned(account.id, pinned),
    t("accounts.center.pinFailed"),
  );
  if (done) {
    announce(
      t(pinned ? "accounts.center.pinnedAnnounce" : "accounts.center.unpinnedAnnounce", {
        name: account.username,
      }),
    );
  }
}

/** Saves a new order of the pinned accounts. */
export async function reorderPinned(queryClient: QueryClient, ids: readonly string[]) {
  const current = pinnedIds(queryClient.getQueryData<AccountSummary[]>(ACCOUNTS_KEY) ?? []);
  if (current.join() === ids.join()) return true;
  return optimistic(
    queryClient,
    (list) => reorderPinnedInList(list, ids),
    () => api.reorderPinnedAccounts([...ids]),
    i18n.t("accounts.center.reorderFailed"),
  );
}

/** Shows a bundled picture (`preset:<slug>`) for an offline account, or its head again (null). */
export async function setAccountAvatar(
  queryClient: QueryClient,
  account: AccountSummary,
  avatar: string | null,
) {
  if ((account.avatar ?? null) === avatar) return true;
  return optimistic(
    queryClient,
    (list) => list.map((a) => (a.id === account.id ? { ...a, avatar: avatar ?? undefined } : a)),
    () => api.setAccountAvatar(account.id, avatar),
    i18n.t("accounts.avatar.failed"),
  );
}

// --- heads, sessions, AFK ----------------------------------------------------------------------

/**
 * Asks the backend to cut the heads that are missing (once per set of missing accounts; a
 * rename or a new skin makes a new set), then refreshes the list when some arrived.
 */
export function useAccountHeads(accounts: AccountSummary[] | undefined) {
  const queryClient = useQueryClient();
  const missing =
    accounts
      ?.filter((a) => !a.headUrl)
      .map((a) => `${a.id}:${a.username}`)
      .sort()
      .join(",") ?? "";
  useQuery({
    queryKey: ["account-heads", missing],
    queryFn: async () => {
      const added = await api.ensureAccountHeads();
      if (added > 0) void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
      return added;
    },
    enabled: missing.length > 0,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
}

/**
 * Checks the saved sessions of Microsoft accounts while an account view is open (at most every
 * ten minutes). An expired one gets flagged by the backend; a working one may have re-read its
 * profile (a skin or name changed elsewhere). The list is then refreshed.
 */
export function useSessionChecks(accounts: AccountSummary[] | undefined, enabled: boolean) {
  const queryClient = useQueryClient();
  const ids =
    accounts?.filter((a) => a.kind === "microsoft" && !a.needsRelogin).map((a) => a.id) ?? [];
  useQueries({
    queries: ids.map((id) => ({
      queryKey: ["account-session", id],
      queryFn: async () => {
        const state = await api.checkAccountSession(id);
        if (state !== "unknown") void queryClient.invalidateQueries({ queryKey: ACCOUNTS_KEY });
        return state;
      },
      enabled,
      staleTime: 10 * 60_000,
      retry: false,
      refetchOnWindowFocus: false,
    })),
  });
}

/** AFK sessions (same cache as the AFK page), live while `enabled`. */
export function useAfkSessions(enabled = true) {
  const queryClient = useQueryClient();
  const { data } = useQuery({
    queryKey: ["afk"],
    queryFn: () => api.afkAccounts(),
    enabled,
  });
  useEffect(() => {
    if (!enabled) return;
    return api.onAfkChange(() => void queryClient.invalidateQueries({ queryKey: ["afk"] }));
  }, [enabled, queryClient]);
  return data ?? [];
}
