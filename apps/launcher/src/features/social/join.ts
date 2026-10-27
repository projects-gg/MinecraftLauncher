// "Katıl": start the game on projects.gg. The instance that joins projects.gg by itself is the
// official one (or any instance whose auto-join server is on projects.gg); it is selected and
// launched with auto-join on, whatever the general auto-join setting says. An invite from another
// server starts the instance OYNA would start, straight into that server. Once the player deleted
// the official instance nothing makes it again here: "Resmî paketi kur" is offered (LIB1).
import { showError } from "@/features/content/jobs";
import { officialOrOffer } from "@/features/library/official";
import { api, type GameAccount } from "@/lib/api";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { ownLaunch } from "@/stores/launch-owner";
import { usePrefs } from "@/stores/prefs";
import { isProjectsHost } from "./model";
import { playInstance } from "./play-together";

/** True while a game is being prepared or running: "Katıl" waits. */
export function gameBusy(kind: string): boolean {
  return kind === "working" || kind === "launching" || kind === "running";
}

/**
 * True while the selected account's launch is on its way or its game runs (PLAY-AGAIN: other
 * accounts' games run beside a new one). Code outside React reads it from the launch store.
 */
export function ownGameBusy(): boolean {
  const launch = useLaunch.getState();
  return gameBusy(ownLaunch(launch, launch.selected).phase.kind);
}

/**
 * A failure shows the error toast instead of nothing: with no Projects instance yet and no
 * connection, its pack cannot be fetched, for example. The backend's message is already in the
 * player's language; anything without one gets a plain line, its details kept for "Ayrıntıları
 * kopyala".
 */
function failed(error: unknown) {
  showError(
    error instanceof Error && error.message
      ? error
      : Object.assign(new Error(i18n.t("eventJoin.failed")), { details: String(error) }),
  );
}

/**
 * Every "Katıl" to projects.gg (event card, friends list, invites). `prefer`: the instance a game
 * closed for "Bağlan" ran (FIX2), when it joins projects.gg by itself; `account`: the account it
 * played with, which it starts again with (PLAY-ACCT).
 */
export async function joinProjects(prefer?: string, account?: GameAccount): Promise<void> {
  try {
    await join(prefer, account);
  } catch (error) {
    failed(error);
  }
}

/**
 * "Katıl" on an invite from another server: the instance OYNA would start (the selected one, else
 * the Projects instance), straight into that server whatever the auto-join setting says. A server
 * joins on every version (`--server` before 1.20); the AFK question of a same-name account comes
 * with the launch, as for OYNA. `prefer`: the instance a game closed for "Bağlan" ran (FIX2);
 * `account`: the account it played with (PLAY-ACCT).
 */
export async function joinServer(
  address: { host: string; port: number | null },
  prefer?: string,
  account?: GameAccount,
): Promise<void> {
  try {
    const launch = useLaunch.getState();
    if (ownGameBusy()) return;
    const prefs = usePrefs.getState();
    const instances = await api.instances();
    const id =
      playInstance(instances, prefer ?? null) ??
      playInstance(instances, prefs.selectedInstanceId) ??
      (await officialOrOffer({ kind: "joinServer", host: address.host, port: address.port }))?.id;
    if (!id) return;
    prefs.selectInstance(id);
    await launch.play(id, {
      autoJoin: false,
      quickPlay: { kind: "multiplayer", host: address.host, port: address.port },
      account,
    });
  } catch (error) {
    failed(error);
  }
}

async function join(prefer?: string, account?: GameAccount): Promise<void> {
  const launch = useLaunch.getState();
  if (ownGameBusy()) return;
  const instances = await api.instances();
  const target =
    instances.find((i) => i.id === prefer && isProjectsHost(i.autoJoin)) ??
    instances.find((i) => i.managed && isProjectsHost(i.autoJoin)) ??
    instances.find((i) => isProjectsHost(i.autoJoin)) ??
    (await officialOrOffer({ kind: "join" }));
  if (!target) return;
  usePrefs.getState().selectInstance(target.id);
  await launch.play(target.id, { autoJoin: true, account });
}
