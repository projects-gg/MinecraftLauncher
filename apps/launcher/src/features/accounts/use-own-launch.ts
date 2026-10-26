// What OYNA and the other play actions show for the selected account (PLAY-ACCT): the launch as it
// is while it is this account's (or not bound to one yet: its files are prepared), else nothing
// running, so another account's "Oyna" is there to press (it asks before closing the other game).
// The rules are in stores/launch-owner.ts.
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { type LaunchPhase, useLaunch } from "@/stores/launch";
import { type LaunchOwner, launchOwner, playingAccountId } from "@/stores/launch-owner";
import { ACCOUNTS_KEY } from "./use-accounts";

/** The phase another account's play actions see: nothing runs for them. */
const IDLE: LaunchPhase = { kind: "idle" };

/** The selected account's id: `undefined` while the accounts load, `null` with none. */
export function useSelectedAccountId(): string | null | undefined {
  const { data } = useQuery({
    queryKey: ACCOUNTS_KEY,
    queryFn: () => api.accounts(),
    select: (list) => list.find((a) => a.active)?.id ?? null,
  });
  return data;
}

export interface OwnLaunch {
  owner: LaunchOwner;
  /** The launch's phase for this account: idle while another account's game runs. */
  phase: LaunchPhase;
  /** The instance of this account's launch (null while it has none). */
  instanceId: string | null;
}

export function useOwnLaunch(): OwnLaunch {
  const selected = useSelectedAccountId();
  const phase = useLaunch((s) => s.phase);
  const instanceId = useLaunch((s) => s.instanceId);
  const account = useLaunch((s) => s.account);
  const owner = launchOwner(phase, account, selected);
  return owner === "other"
    ? { owner, phase: IDLE, instanceId: null }
    : { owner, phase, instanceId };
}

/**
 * Only the kind of this account's launch phase ("idle" while another account's game runs): for
 * views that need no progress figures, so they do not render again at every progress step.
 */
export function useOwnLaunchKind(): LaunchPhase["kind"] {
  const selected = useSelectedAccountId();
  return useLaunch((s) =>
    launchOwner(s.phase, s.account, selected) === "other" ? "idle" : s.phase.kind,
  );
}

/** The account whose game is on screen: its rows say "Oyunda" (account center, account menu). */
export function usePlayingAccountId(): string | null {
  return useLaunch((s) => playingAccountId(s.phase, s.account));
}
