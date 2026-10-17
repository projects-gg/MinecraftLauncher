// The mod health flow: one dialog that moves through a crash, mods that stop a launch, a
// repair ("Onar") and a reinstall ("Yeniden yükle") without closing in between, so every step
// glides out of the one before it.
import { toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { create } from "zustand";
import { openModCenter } from "@/features/modcenter/notices";
import { api, type ModCheck, type RepairScope } from "@/lib/api";
import i18n from "@/lib/i18n";

/** Where the flow started: it decides whether the end offers "Oyna". */
export type FlowOrigin = "crash" | "blocked" | "center";

export type FlowStep =
  | { kind: "crash" }
  | { kind: "blocked"; check: ModCheck }
  | { kind: "repair"; scope: RepairScope }
  | { kind: "reinstall" };

interface ModFlowState {
  instanceId: string | null;
  origin: FlowOrigin | null;
  step: FlowStep | null;
  /** Steps to go back to ("Vazgeç" in the reinstall confirmation). */
  history: FlowStep[];
  /** Counts the flows opened: a new one starts its jobs afresh, going back does not. */
  session: number;
  open: (instanceId: string, origin: FlowOrigin, step: FlowStep) => void;
  go: (step: FlowStep) => void;
  back: () => boolean;
  close: () => void;
}

export const useModFlow = create<ModFlowState>()((set, get) => ({
  instanceId: null,
  origin: null,
  step: null,
  history: [],
  session: 0,
  open: (instanceId, origin, step) =>
    set((s) => ({ instanceId, origin, step, history: [], session: s.session + 1 })),
  go: (step) => {
    const { step: current, history } = get();
    set({ step, history: current ? [...history, current] : history });
  },
  back: () => {
    const { history } = get();
    const previous = history[history.length - 1];
    if (!previous) return false;
    set({ step: previous, history: history.slice(0, -1) });
    return true;
  },
  close: () => set({ step: null, history: [] }),
}));

/** Query key of an instance's mod check. */
export const modCheckKey = (instanceId: string) => ["mod-check", instanceId] as const;

/** Keys whose refresh means the mods folder may have changed. */
const related = (instanceId: string, key: readonly unknown[]) =>
  key[1] === instanceId &&
  (key[0] === "mod-center" ||
    key[0] === "instance" ||
    (key[0] === "content-scan" && key[2] === "mod"));

/**
 * The mod check of an instance, checked again whenever the mod center, the instance or its
 * mods list is refreshed (any change to the mods folder goes through one of them) and when the
 * page opens. Cheap: the jars are cached by size and time.
 */
export function useModCheck(instanceId: string, enabled = true) {
  const queryClient = useQueryClient();
  useEffect(() => {
    const cache = queryClient.getQueryCache();
    return cache.subscribe((event) => {
      if (
        event.type === "updated" &&
        event.action.type === "invalidate" &&
        related(instanceId, event.query.queryKey)
      ) {
        void queryClient.invalidateQueries({ queryKey: modCheckKey(instanceId) });
      }
    });
  }, [queryClient, instanceId]);
  return useQuery({
    queryKey: modCheckKey(instanceId),
    queryFn: () => api.modHealth.check(instanceId),
    enabled,
    staleTime: 0,
    refetchOnMount: "always",
    retry: false,
  });
}

/** After a repair or reinstall: everything that shows the mods folder is read again. */
export function refreshAfterRepair(
  queryClient: ReturnType<typeof useQueryClient>,
  instanceId: string,
) {
  for (const key of [
    modCheckKey(instanceId),
    ["mod-center", instanceId],
    ["instance", instanceId],
    ["instances"],
    ["content-scan", instanceId, "mod"],
  ]) {
    void queryClient.invalidateQueries({ queryKey: key });
  }
}

/** Warnings found right before a launch: a quiet toast, details in the mod center. */
export function announceModWarnings(instanceId: string) {
  toast(i18n.t("modHealth.toast.warnings"), {
    duration: 8000,
    action: {
      label: i18n.t("modHealth.toast.details"),
      onClick: () => openModCenter(instanceId),
    },
  });
}
