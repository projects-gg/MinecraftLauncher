import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { api } from "@/lib/api";
import { useLaunch } from "@/stores/launch";

/**
 * True while the game of `instanceId` is starting or running. The launch store covers every
 * launch (the tray's and shortcuts' go through it too); the backend answers once for a game that
 * was already running when the page loaded, and again whenever a launch starts or ends. It used
 * to be asked every 4 s as well, for as long as a worlds, logs or pack update view was open
 * (OPT2: no polling while nothing happens).
 */
export function useGameRunning(instanceId: string): boolean {
  const queryClient = useQueryClient();
  const launching = useLaunch(
    (s) =>
      s.instanceId === instanceId &&
      (s.phase.kind === "working" || s.phase.kind === "launching" || s.phase.kind === "running"),
  );
  const { data: runningId } = useQuery({
    queryKey: ["game-running"],
    queryFn: () => api.library.runningInstance(),
  });
  // Ask again right away when a launch starts or ends.
  useEffect(() => {
    void launching;
    void queryClient.invalidateQueries({ queryKey: ["game-running"] });
  }, [launching, queryClient]);
  return launching || runningId === instanceId;
}
