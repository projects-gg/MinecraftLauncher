// LIB1: the official Projects instance once the player can delete it. Nothing makes it again by
// itself after a deletion (the backend remembers it, `official_status.removed`); the library and
// the actions that need it (OYNA with nothing to play, "Katıl", the tray's "Oyna") offer
// "Resmî paketi kur" instead. Kept free of UI imports: the join helpers use it too.
import { create } from "zustand";
import { api, type InstanceSummary, type OfficialStatus } from "@/lib/api";

/** What the offer goes on with once the official pack is in ("Kur ve oyna", "Kur ve katıl"). */
export type OfficialIntent =
  /** OYNA, the tray's "Oyna" or `projects://play` with no instance to start. */
  | { kind: "play" }
  /** "Katıl" to projects.gg (event card, friends, invites). */
  | { kind: "join" }
  /** An invite to another server. */
  | { kind: "joinServer"; host: string; port: number | null };

interface OfficialOfferState {
  /** The open offer (`OfficialOfferDialog`), if any. */
  intent: OfficialIntent | null;
  show: (intent: OfficialIntent) => void;
  close: () => void;
}

/** The status every view shares (the pack watcher, the library, the offer). */
export const officialStatusQuery = {
  queryKey: ["official-status"],
  queryFn: () => api.officialStatus(),
  staleTime: 5 * 60_000,
} as const;

export const useOfficialOffer = create<OfficialOfferState>()((set) => ({
  intent: null,
  show: (intent) => set({ intent }),
  close: () => set({ intent: null }),
}));

/**
 * The official instance an action needs: the one there, made now when the player never deleted
 * it. After a deletion nothing is made; the offer opens instead and `null` comes back (the caller
 * stops there, without an error).
 */
export async function officialOrOffer(intent: OfficialIntent): Promise<InstanceSummary | null> {
  const official = await api.ensureOfficialInstance();
  if (!official) useOfficialOffer.getState().show(intent);
  return official;
}

/**
 * Whether the official pack is among `instances`: an instance that follows a pack (a copy too),
 * or the one the status names (the vanilla "Projects" before any pack is published). The list
 * answers first, so a freshly installed pack never shows next to its own offer.
 */
export function hasOfficial(
  instances: readonly InstanceSummary[],
  official: OfficialStatus | undefined,
): boolean {
  return instances.some((i) => i.managed || i.id === official?.instanceId);
}

/**
 * What the library offers besides the cards: `empty` when no instance is left (the empty state:
 * "Resmî paketi kur", "Yeni kurulum"), `official` when only the official pack is missing (a quiet
 * "Kurulu değil" card where it was). Nothing while either list is unknown or while searching.
 */
export function libraryOffer(
  instances: readonly InstanceSummary[] | undefined,
  official: OfficialStatus | undefined,
  searching: boolean,
): "empty" | "official" | null {
  if (!instances) return null;
  if (instances.length === 0) return "empty";
  if (searching || !official) return null;
  return hasOfficial(instances, official) ? null : "official";
}

/**
 * Whether deleting `id` leaves no official instance: the confirmation then says the pack can be
 * installed again. A copy left behind keeps the pack, so the usual sentence stays.
 */
export function deletesLastOfficial(
  instances: readonly InstanceSummary[],
  official: OfficialStatus | undefined,
  id: string,
): boolean {
  const target = instances.find((i) => i.id === id);
  if (!target || !hasOfficial([target], official)) return false;
  return !hasOfficial(
    instances.filter((i) => i.id !== id),
    official?.instanceId === id ? { ...official, instanceId: undefined } : official,
  );
}

/** The instance OYNA starts: the selected one while it exists, else the first (the bar's rule). */
export function playTarget(
  instances: readonly InstanceSummary[],
  selectedId: string | null,
): InstanceSummary | undefined {
  return instances.find((i) => i.id === selectedId) ?? instances[0];
}
