import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import type { InstanceSummary } from "@/lib/api";
import { usePrefs } from "@/stores/prefs";
import { usePrepare } from "@/stores/prepare";
import { prepareApi } from "./api";

/**
 * Keeps the launcher's background preparation on the instance the command bar shows (once the
 * instance list has loaded) and the bar's hint in step with it. A finished preparation refreshes
 * the instance lists: OYNA reads "Oyna" again and a pack update shows its new version.
 */
export function usePrepareSync(selected: InstanceSummary | undefined, loaded: boolean) {
  const enabled = usePrefs((s) => s.backgroundPrepare);
  const queryClient = useQueryClient();
  const instanceId = selected?.id ?? null;
  const version = selected
    ? `${selected.mcVersion}/${selected.loader}/${selected.loaderVersion ?? ""}`
    : null;

  useEffect(() => {
    const { receive } = usePrepare.getState();
    const refresh = () => {
      prepareApi.status().then(receive, () => {});
    };
    const off = prepareApi.onStatus((status) => {
      receive(status);
      if (status.state === "ready") {
        for (const queryKey of [["instances"], ["instance"]]) {
          void queryClient.invalidateQueries({ queryKey });
        }
      }
    });
    refresh();
    // Out of sight the launcher sends no progress: the page asks again when it is back.
    const onVisible = () => {
      if (document.visibilityState === "visible") refresh();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      off();
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [queryClient]);

  useEffect(() => {
    if (!loaded) return;
    prepareApi.configure(enabled, instanceId, version).catch(() => {});
  }, [loaded, enabled, instanceId, version]);
}
