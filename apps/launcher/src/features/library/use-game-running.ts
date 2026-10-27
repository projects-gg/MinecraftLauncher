import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useSelectedAccountId } from "@/features/accounts/use-own-launch";
import { api } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { inProgress, ownLaunch } from "@/stores/launch-owner";

/**
 * True while a game of `instanceId` is starting or running, whichever account plays it (its files
 * are in use). The launch store covers every launch (the tray's and shortcuts' go through it too);
 * the backend answers once for a game that was already running when the page loaded, and again
 * whenever a launch starts or ends. It used to be asked every 4 s as well, for as long as a
 * worlds, logs or pack update view was open (OPT2: no polling while nothing happens).
 */
export function useGameRunning(instanceId: string): boolean {
  const queryClient = useQueryClient();
  const launching = useLaunch(
    (s) =>
      (s.instanceId === instanceId && inProgress(s.phase)) ||
      s.games.some((game) => game.instanceId === instanceId),
  );
  const { data: running } = useQuery({
    queryKey: ["game-running"],
    queryFn: () => api.library.runningInstances(),
  });
  // Ask again right away when a launch starts or ends.
  useEffect(() => {
    void launching;
    void queryClient.invalidateQueries({ queryKey: ["game-running"] });
  }, [launching, queryClient]);
  return launching || (running?.includes(instanceId) ?? false);
}

/**
 * True while the selected account has a launch or a game of `instanceId` (PLAY-AGAIN). An
 * instance's play actions ("Devam et", a world's or a server's "Oyna") are off then: one more game
 * of it is OYNA's "Oyunda". Another account's game leaves them on: this account's starts beside it
 * (a world open in that game is refused before anything loads). Its files stay locked either way
 * (`useGameRunning`).
 */
export function useOwnGameOf(instanceId: string): boolean {
  const selected = useSelectedAccountId();
  return useLaunch((s) => {
    const own = ownLaunch(s, selected);
    return own.owner === "mine" && own.instanceId === instanceId;
  });
}
