// AUTOJOIN in the browser preview: the Projects instance's sub-servers beside "Doğrudan sunucuya
// bağlan" ("play.projects.gg → SkyBlock"), as Studio's list stood on 28 Oct 2026.
// `?subServers=off` (or localStorage `pl.mock.subServers`): Studio's switch is off, nothing is
// offered. `?tester=1`: the player is in the beta program, so "Yalnız testçiler" rows show.
import type { SubServerView } from "@/bindings";
import type { InstanceSummary } from "./types";

const TODAY: SubServerView[] = [
  { id: "skyblock", name: "SkyBlock", testersOnly: false },
  { id: "smp", name: "SMP", testersOnly: false },
  { id: "survival", name: "Survival", testersOnly: false },
  { id: "hg", name: "Hunger Games", testersOnly: false },
  { id: "testserver", name: "TestServer", testersOnly: true },
  { id: "pvp", name: "PvP", testersOnly: false },
];

function flag(key: string): string {
  try {
    return (
      new URLSearchParams(window.location.search).get(key) ??
      localStorage.getItem(`pl.mock.${key}`) ??
      ""
    );
  } catch {
    return "";
  }
}

function isProjects(address: string | undefined): boolean {
  const host = (address ?? "").split(":")[0]?.toLowerCase() ?? "";
  return host === "projects.gg" || host.endsWith(".projects.gg");
}

export async function mockInstanceSubServers(
  instance: InstanceSummary | undefined,
): Promise<SubServerView[]> {
  await new Promise((resolve) => setTimeout(resolve, 80));
  if (flag("subServers") === "off") return [];
  if (instance?.loader !== "fabric" || !isProjects(instance.autoJoin)) return [];
  return TODAY;
}

export async function mockSubServersTester(): Promise<boolean> {
  await new Promise((resolve) => setTimeout(resolve, 240));
  return flag("tester") === "1";
}
