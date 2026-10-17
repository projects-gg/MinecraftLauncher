// Order of the account list (owner's rule): pinned accounts first, in the player's order (their
// order in the saved list); the rest have no manual order: the active account first, then by
// last play, then by when they were added. The list helpers mirror pl-auth's store, so an
// optimistic update looks exactly like the list the backend sends back.

export interface OrderedAccount {
  id: string;
  active: boolean;
  pinned?: boolean;
  lastUsedAt?: string;
  addedAt?: string;
}

const time = (iso?: string) => {
  const value = iso ? Date.parse(iso) : Number.NaN;
  return Number.isNaN(value) ? Number.NEGATIVE_INFINITY : value;
};

/** Display order of a saved list (stable: ties keep their saved order). */
export function sortAccounts<T extends OrderedAccount>(accounts: readonly T[]): T[] {
  const pinned = accounts.filter((a) => a.pinned);
  const rest = accounts
    .map((account, index) => ({ account, index }))
    .filter(({ account }) => !account.pinned)
    .sort(
      (a, b) =>
        Number(b.account.active) - Number(a.account.active) ||
        compareDesc(time(a.account.lastUsedAt), time(b.account.lastUsedAt)) ||
        compareDesc(time(a.account.addedAt), time(b.account.addedAt)) ||
        a.index - b.index,
    )
    .map(({ account }) => account);
  return [...pinned, ...rest];
}

function compareDesc(a: number, b: number) {
  if (a === b) return 0;
  return a > b ? -1 : 1;
}

/** Ids of the pinned accounts, in the player's order. */
export function pinnedIds(accounts: readonly OrderedAccount[]): string[] {
  return accounts.filter((a) => a.pinned).map((a) => a.id);
}

/** The saved list after pinning (joins the end of the pinned ones) or unpinning (stays put). */
export function setPinnedInList<T extends OrderedAccount>(
  list: readonly T[],
  id: string,
  pinned: boolean,
): T[] {
  const index = list.findIndex((a) => a.id === id);
  const account = list[index];
  if (!account || Boolean(account.pinned) === pinned) return [...list];
  const rest = list.filter((a) => a.id !== id);
  const changed = { ...account, pinned };
  if (!pinned) {
    rest.splice(index, 0, changed);
    return rest;
  }
  let at = 0;
  rest.forEach((a, i) => {
    if (a.pinned) at = i + 1;
  });
  rest.splice(at, 0, changed);
  return rest;
}

/** The saved list with the pinned accounts in the order of `ids` (unknown ids are ignored). */
export function reorderPinnedInList<T extends OrderedAccount>(
  list: readonly T[],
  ids: readonly string[],
): T[] {
  const pinned = list.filter((a) => a.pinned);
  const ordered: T[] = [];
  for (const id of ids) {
    const at = pinned.findIndex((a) => a.id === id);
    const [account] = at >= 0 ? pinned.splice(at, 1) : [];
    if (account) ordered.push(account);
  }
  return [...ordered, ...pinned, ...list.filter((a) => !a.pinned)];
}

/** Pinned ids after moving `id` one place up (-1) or down (1); null when it cannot move. */
export function movePinned(ids: readonly string[], id: string, step: -1 | 1): string[] | null {
  const from = ids.indexOf(id);
  const to = from + step;
  if (from < 0 || to < 0 || to >= ids.length) return null;
  const next = [...ids];
  next.splice(from, 1);
  next.splice(to, 0, id);
  return next;
}

/** The display list with the pinned accounts in a temporary order (while one is dragged). */
export function withPinnedOrder<T extends OrderedAccount>(
  sorted: readonly T[],
  order: readonly string[] | null,
): T[] {
  if (!order) return [...sorted];
  return reorderPinnedInList(sorted, order);
}
