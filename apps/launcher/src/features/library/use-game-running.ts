import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { api } from "@/lib/api";
import { useLaunch } from "@/stores/launch";

/**
 * True while the game of `instanceId` is starting or running. The launch store covers launches
 * from this window; the backend answers after a reload of the UI (polled, it is cheap).
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
    refetchInterval: 4000,
  });
  // Ask again right away when a launch starts or ends instead of waiting for the next poll.
  useEffect(() => {
    void launching;
    void queryClient.invalidateQueries({ queryKey: ["game-running"] });
  }, [launching, queryClient]);
  return launching || runningId === instanceId;
}
