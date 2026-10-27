// What OYNA and the other play actions show for the selected account (PLAY-ACCT, PLAY-AGAIN): its
// launch on its way (or one nobody's yet: its files are prepared), else its newest game on screen,
// else nothing running for it, so its "Oyna" is there to press while other accounts play. The rules
// are in stores/launch-owner.ts.
import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { useShallow } from "zustand/react/shallow";
import { api } from "@/lib/api";
import { type LaunchPhase, useLaunch } from "@/stores/launch";
import { type OwnLaunch, othersInGame, ownLaunch, playingAccountIds } from "@/stores/launch-owner";
import { ACCOUNTS_KEY } from "./use-accounts";

/** The selected account's id: `undefined` while the accounts load, `null` with none. */
export function useSelectedAccountId(): string | null | undefined {
  const { data } = useQuery({
    queryKey: ACCOUNTS_KEY,
    queryFn: () => api.accounts(),
    select: (list) => list.find((a) => a.active)?.id ?? null,
  });
  return data;
}

/**
 * Keeps the launch store's `selected` in step with the account list (mounted once, in the shell):
 * the play rules of code outside React read it there.
 */
export function useSelectedAccountMirror() {
  const selected = useSelectedAccountId();
  useEffect(() => {
    useLaunch.setState({ selected });
  }, [selected]);
}

export type { OwnLaunch };

export function useOwnLaunch(): OwnLaunch {
  const selected = useSelectedAccountId();
  return useLaunch(useShallow((s) => ownLaunch(s, selected)));
}

/**
 * Only the kind of this account's launch phase ("idle" while only other accounts' games run): for
 * views that need no progress figures, so they do not render again at every progress step.
 */
export function useOwnLaunchKind(): LaunchPhase["kind"] {
  const selected = useSelectedAccountId();
  return useLaunch((s) => ownLaunch(s, selected).phase.kind);
}

/** The instance of this account's launch or game (null while it has none). */
export function useOwnLaunchInstance(): string | null {
  const selected = useSelectedAccountId();
  return useLaunch((s) => ownLaunch(s, selected).instanceId);
}

/** The accounts whose game is on screen: their rows say "Oyunda" (account center, account menu). */
export function usePlayingAccountIds(): string[] {
  return useLaunch(useShallow((s) => playingAccountIds(s)));
}

/**
 * The names of the other accounts in game while the selected one has nothing running: the command
 * bar's quiet "EjderAvcisi oyunda" (PLAY-AGAIN).
 */
export function useOthersInGame(): string[] {
  const selected = useSelectedAccountId();
  return useLaunch(useShallow((s) => othersInGame(s, selected)));
}
