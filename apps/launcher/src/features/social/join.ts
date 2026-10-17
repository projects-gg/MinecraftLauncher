// "Katıl": start the game on projects.gg. The instance that joins projects.gg by itself is the
// official one (or any instance whose auto-join server is on projects.gg); it is selected and
// launched with auto-join on, whatever the general auto-join setting says.
import { api } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { isProjectsHost } from "./model";

/** True while a game is being prepared or running: "Katıl" waits. */
export function gameBusy(kind: string): boolean {
  return kind === "working" || kind === "launching" || kind === "running";
}

export async function joinProjects(): Promise<void> {
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
