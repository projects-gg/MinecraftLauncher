import { create } from "zustand";
import type { PrepareStatusView } from "@/bindings";

/**
 * What the command bar shows under the instance's name while it prepares in the background:
 * "Hazırlanıyor %42", then "Oynamaya hazır" for a moment.
 */
export type PrepareHint =
  | { instanceId: string; kind: "preparing"; percent: number }
  | { instanceId: string; kind: "ready" };

/** A preparation shows only once it has run this long (a quick look never flickers the bar). */
export const SHOW_AFTER_MS = 1200;
/** How long "Oynamaya hazır" stays. */
export const READY_FOR_MS = 2400;

interface PrepareState {
  hint: PrepareHint | null;
  /** A status from the launcher (`prepare-status`, `prepare_status`). */
  receive: (status: PrepareStatusView) => void;
}

/** The latest status while its line waits to show, and the timers of the two delays. */
let pending: Extract<PrepareStatusView, { state: "preparing" }> | null = null;
let showTimer: ReturnType<typeof setTimeout> | undefined;
let readyTimer: ReturnType<typeof setTimeout> | undefined;

function clearShow() {
  clearTimeout(showTimer);
  showTimer = undefined;
  pending = null;
}

/** Background preparation as the command bar shows it; quiet by design. */
export const usePrepare = create<PrepareState>()((set, get) => ({
  hint: null,
  receive(status) {
    const hint = get().hint;
    switch (status.state) {
      case "preparing": {
        const { instanceId } = status;
        const percent = status.percent ?? 0;
        if (hint?.kind === "preparing" && hint.instanceId === instanceId) {
          set({ hint: { ...hint, percent } });
          return;
        }
        pending = status;
        if (showTimer === undefined) {
          showTimer = setTimeout(() => {
            const latest = pending;
            clearShow();
            if (latest) {
              clearTimeout(readyTimer);
              set({
                hint: {
                  instanceId: latest.instanceId,
                  kind: "preparing",
                  percent: latest.percent ?? 0,
                },
              });
            }
          }, SHOW_AFTER_MS);
        }
        return;
      }
      case "ready": {
        clearShow();
        // Only a preparation the player saw says it is done.
        if (hint?.kind !== "preparing" || hint.instanceId !== status.instanceId) return;
        set({ hint: { instanceId: status.instanceId, kind: "ready" } });
        clearTimeout(readyTimer);
        readyTimer = setTimeout(() => {
          if (get().hint?.kind === "ready") set({ hint: null });
        }, READY_FOR_MS);
        return;
      }
      default:
        clearShow();
        if (hint?.kind === "preparing") set({ hint: null });
    }
  },
}));
