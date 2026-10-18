// "Katıl": start the game on projects.gg. The instance that joins projects.gg by itself is the
// official one (or any instance whose auto-join server is on projects.gg); it is selected and
// launched with auto-join on, whatever the general auto-join setting says.
import { showError } from "@/features/content/jobs";
import { api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { isProjectsHost } from "./model";

/** True while a game is being prepared or running: "Katıl" waits. */
export function gameBusy(kind: string): boolean {
  return kind === "working" || kind === "launching" || kind === "running";
}

/**
 * Every "Katıl" (event card, friends list, invite toast). A failure shows the error toast instead
 * of nothing: with no Projects instance yet and no connection, its pack cannot be fetched, for
 * example. The backend's message is already in the player's language; anything without one gets
 * a plain line, its details kept for "Ayrıntıları kopyala".
 */
export async function joinProjects(): Promise<void> {
  try {
    await join();
  } catch (error) {
    showError(
      error instanceof Error && error.message
        ? error
        : Object.assign(new Error(i18n.t("eventJoin.failed")), { details: String(error) }),
    );
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
