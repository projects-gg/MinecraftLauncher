import { useEffect } from "react";
import { useOwnLaunchKind } from "@/features/accounts/use-own-launch";
import { api } from "@/lib/api";
import { isTauri } from "@/lib/platform";

/**
 * The tray's "Oyna" reads "Oyunda" and does nothing while the selected account's game runs, as
 * OYNA does (PLAY-ACCT). With another account selected it stays "Oyna": the window comes forward
 * with the question before the other game closes (features/game-switch).
 */
export function useTrayInGame() {
  const inGame = useOwnLaunchKind() === "running";
  useEffect(() => {
    if (!isTauri()) return;
    void api.setTrayInGame(inGame).catch(() => {});
  }, [inGame]);
}
