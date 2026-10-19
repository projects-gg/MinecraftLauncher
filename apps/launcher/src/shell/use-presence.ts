import { useQuery } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";
import { api } from "@/lib/api";
import { isTauri } from "@/lib/platform";
import { isOpening, useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";

/** Keeps Discord Rich Presence in sync with what the player is doing in the launcher. */
export function usePresenceSync() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const phase = useLaunch((s) => s.phase);
  const launchInstanceId = useLaunch((s) => s.instanceId);
  const enabled = usePrefs((s) => s.discordEnabled);
  const showServer = usePrefs((s) => s.discordShowServer);
  const autoJoin = usePrefs((s) => s.autoJoin);
  const { data: instances } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
    enabled: isTauri(),
  });
  const { data: server } = useQuery({
    queryKey: ["server-status"],
    queryFn: () => api.serverStatus(),
    enabled: isTauri(),
  });

  useEffect(() => {
    if (!isTauri()) return;
    void api.setPresenceEnabled(enabled);
  }, [enabled]);

  const instance = instances?.find((i) => i.id === launchInstanceId);
  const percent = phase.kind === "working" ? Math.floor(phase.percent) : null;
  // "Playing" from the moment the game's window is on screen, as OYNA says "Oyunda".
  const since = phase.kind === "running" && phase.shown ? phase.since : null;
  const opening = isOpening(phase);

  useEffect(() => {
    if (!isTauri() || !enabled) return;
    const page = pathname === "/" ? "home" : (pathname.split("/")[1] ?? "home");
    const activity =
      since !== null ? "playing" : opening ? "starting" : percent !== null ? "downloading" : page;
    const joinsServer = Boolean(since !== null && autoJoin && instance?.autoJoin);
    void api.updatePresence({
      activity,
      instanceName: instance?.name ?? null,
      mcVersion: instance?.mcVersion ?? null,
      loader: instance?.loader ?? null,
      server: joinsServer ? (instance?.autoJoin ?? null) : null,
      players: joinsServer ? (server?.players ?? null) : null,
      maxPlayers: joinsServer ? (server?.maxPlayers ?? null) : null,
      percent,
      startedAtMs: since,
      showServer,
    });
    // Percent updates are coalesced by the backend's rate limiter.
  }, [pathname, percent, since, opening, enabled, showServer, autoJoin, instance, server]);
}
