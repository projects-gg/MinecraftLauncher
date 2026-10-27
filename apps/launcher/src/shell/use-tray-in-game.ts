import { useEffect } from "react";
import { useOwnLaunchKind } from "@/features/accounts/use-own-launch";
import { api } from "@/lib/api";
import { isTauri } from "@/lib/platform";

/**
 * The tray's "Oyna" reads "Oyunda" and does nothing while the selected account's game runs
 * (PLAY-ACCT): one more game with the same name is asked for in the window, on OYNA's "Oyunda"
 * (PLAY-AGAIN). With another account selected it stays "Oyna": the window comes forward and that
 * account's game starts beside the others (asking first when the free memory is short).
 */
export function useTrayInGame() {
  const inGame = useOwnLaunchKind() === "running";
  useEffect(() => {
    if (!isTauri()) return;
    void api.setTrayInGame(inGame).catch(() => {});
  }, [inGame]);
}
