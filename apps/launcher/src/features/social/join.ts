// "Katıl": start the game on projects.gg. The instance that joins projects.gg by itself is the
// official one (or any instance whose auto-join server is on projects.gg); it is selected and
// launched with auto-join on, whatever the general auto-join setting says. An invite from another
// server starts the instance OYNA would start, straight into that server.
import { showError } from "@/features/content/jobs";
import { api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { isProjectsHost } from "./model";
import { playInstance } from "./play-together";

/** True while a game is being prepared or running: "Katıl" waits. */
export function gameBusy(kind: string): boolean {
  return kind === "working" || kind === "launching" || kind === "running";
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

/** Every "Katıl" to projects.gg (event card, friends list, invites). */
export async function joinProjects(): Promise<void> {
  try {
    await join();
  } catch (error) {
    failed(error);
  }
}

/**
 * "Katıl" on an invite from another server: the instance OYNA would start (the selected one, else
 * the Projects instance), straight into that server whatever the auto-join setting says. A server
 * joins on every version (`--server` before 1.20); the AFK question of a same-name account comes
 * with the launch, as for OYNA.
 */
export async function joinServer(address: { host: string; port: number | null }): Promise<void> {
  try {
    const launch = useLaunch.getState();
    if (gameBusy(launch.phase.kind)) return;
    const prefs = usePrefs.getState();
    const id =
      playInstance(await api.instances(), prefs.selectedInstanceId) ??
      (await api.ensureOfficialInstance()).id;
    prefs.selectInstance(id);
    await launch.play(id, {
      autoJoin: false,
      quickPlay: { kind: "multiplayer", host: address.host, port: address.port },
    });
  } catch (error) {
    failed(error);
  }
}

async function join(): Promise<void> {
  const launch = useLaunch.getState();
  if (gameBusy(launch.phase.kind)) return;
  const instances = await api.instances();
  const target =
    instances.find((i) => i.managed && isProjectsHost(i.autoJoin)) ??
    instances.find((i) => isProjectsHost(i.autoJoin)) ??
    (await api.ensureOfficialInstance());
  usePrefs.getState().selectInstance(target.id);
  await launch.play(target.id, { autoJoin: true });
}
