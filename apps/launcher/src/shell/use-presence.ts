import { useQuery } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";
import { api } from "@/lib/api";
import { isTauri } from "@/lib/platform";
import { isOpening, useLaunch } from "@/stores/launch";
import { runningPhase } from "@/stores/launch-owner";
import { usePrefs } from "@/stores/prefs";

/**
 * Keeps Discord Rich Presence in sync with what the player is doing in the launcher. Where the
 * game is (Projects, another server, singleplayer) the backend reads from the game's log itself
 * (OVL-RPC), so it follows the game while this page rests in the tray.
 */
export function usePresenceSync() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  // The followed launch; with a game on screen while another launch is on its way, that game
  // (PLAY-AGAIN: the player is playing, not waiting).
  const shownGame = useLaunch((s) => s.games.findLast((game) => game.shown) ?? null);
  const followedPhase = useLaunch((s) => s.phase);
  const followedId = useLaunch((s) => s.instanceId);
  const phase =
    shownGame && followedPhase.kind !== "running" ? runningPhase(shownGame) : followedPhase;
  const launchInstanceId =
    shownGame && followedPhase.kind !== "running" ? shownGame.instanceId : followedId;
  const enabled = usePrefs((s) => s.discordEnabled);
  const showServer = usePrefs((s) => s.discordShowServer);
  const showAddress = usePrefs((s) => s.discordShowAddress);
  const showCrew = usePrefs((s) => s.discordShowCrew);
  const showCrewCount = usePrefs((s) => s.discordShowCrewCount);
  const showCrewName = usePrefs((s) => s.discordShowCrewName);
  const showPack = usePrefs((s) => s.discordShowPack);
  const showElapsed = usePrefs((s) => s.discordShowElapsed);
  const { data: instances } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
    enabled: isTauri(),
  });
  // Projects' player count: the backend shows it only while the game is on Projects.
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
    const playing = since !== null;
    void api.updatePresence({
      activity,
      // RPC-PLUS: only the official pack's name; a name the player gave their instance stays home.
      packName: instance?.managed ? instance.name : null,
      mcVersion: instance?.mcVersion ?? null,
      loader: instance?.loader ?? null,
      players: playing && server?.online ? server.players : null,
      maxPlayers: playing && server?.online ? server.maxPlayers : null,
      percent,
      startedAtMs: since,
      showServer,
      showAddress,
      showCrew,
      showCrewCount,
      showCrewName,
      showPack,
      showElapsed,
    });
    // Percent updates are coalesced by the backend's rate limiter.
  }, [
    pathname,
    percent,
    since,
    opening,
    enabled,
    showServer,
    showAddress,
    showCrew,
    showCrewCount,
    showCrewName,
    showPack,
    showElapsed,
    instance,
    server,
  ]);
}
