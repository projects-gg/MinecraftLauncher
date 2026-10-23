// SKIN1's data in the interface: the overview (entry points, the link), one account's panel view
// and its default look, read through react-query and refreshed by `projects-skin-changed`. The
// panel reads the service only while it is open (skin1.md §13.6); the backend polls and pushes.
import { useQuery } from "@tanstack/react-query";
import { create } from "zustand";
import { useSocialUi } from "@/features/social/store";
import { projectsSkinApi } from "@/lib/api/projects-skin-api";

export const PROJECTS_SKIN_KEY = ["projects-skin"] as const;
export const overviewKey = [...PROJECTS_SKIN_KEY, "overview"] as const;
export const stateKey = (accountId: string) => [...PROJECTS_SKIN_KEY, "state", accountId] as const;
export const defaultLookKey = (accountId: string) =>
  [...PROJECTS_SKIN_KEY, "default", accountId] as const;

/** Entry points and the link as last known (no network: the backend answers from memory). */
export function useProjectsSkinOverview() {
  // Signing in or out of friends changes what is known: ask again.
  const auth = useSocialUi((s) => s.view.auth);
  return useQuery({
    queryKey: [...overviewKey, auth],
    queryFn: () => projectsSkinApi.overview(),
    staleTime: 30_000,
    refetchOnWindowFocus: false,
  });
}

/** The panel's view of one account: asked when the panel opens, again on every change. */
export function useProjectsSkinState(accountId: string, enabled: boolean) {
  return useQuery({
    queryKey: stateKey(accountId),
    queryFn: () => projectsSkinApi.state(accountId),
    enabled,
    staleTime: 0,
    gcTime: 60_000,
    retry: false,
    refetchOnWindowFocus: false,
  });
}

/** What the account looks like on Projects without a Projects skin (Mojang asked rarely). */
export function useDefaultLook(accountId: string, enabled: boolean) {
  return useQuery({
    queryKey: defaultLookKey(accountId),
    queryFn: () => projectsSkinApi.defaultLook(accountId),
    enabled,
    staleTime: 10 * 60_000,
    gcTime: 10 * 60_000,
    retry: 1,
    refetchOnWindowFocus: false,
  });
}

const TOLD_KEY = "pl.projectsSkin.told";

function readTold(): string[] {
  try {
    const raw = localStorage.getItem(TOLD_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === "string") : [];
  } catch {
    return [];
  }
}

interface ProjectsSkinUi {
  /** Code boxes on screen: a link they started tells its own result. */
  codeBoxes: number;
  holdCodeBox: () => () => void;
  /** Failed jobs the player was told about (kept on this computer, the last 20). */
  told: ReadonlySet<string>;
  markTold: (id: string) => void;
}

export const useProjectsSkinUi = create<ProjectsSkinUi>()((set, get) => ({
  codeBoxes: 0,
  holdCodeBox: () => {
    set((s) => ({ codeBoxes: s.codeBoxes + 1 }));
    let held = true;
    return () => {
      if (!held) return;
      held = false;
      set((s) => ({ codeBoxes: Math.max(0, s.codeBoxes - 1) }));
    };
  },
  told: new Set(readTold()),
  markTold: (id) => {
    if (get().told.has(id)) return;
    const next = [...get().told, id].slice(-20);
    set({ told: new Set(next) });
    try {
      localStorage.setItem(TOLD_KEY, JSON.stringify(next));
    } catch {
      // A private window or blocked storage: told again next time, nothing worse.
    }
  },
}));
