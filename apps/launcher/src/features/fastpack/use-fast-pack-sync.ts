import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { usePrefs } from "@/stores/prefs";
import { fastPackApi, fastPackStateQuery } from "./api";

/**
 * Keeps "Hızlı kaynak paketi yükleme" in step with the launcher (mounted once, with the command
 * bar): the switch goes to Rust at the start and on every change, so the pack syncs that run while
 * the page rests (background preparation, a game start) follow it. It also loads the row's view
 * for the install Play opens once the install list is read, so Settings → Genel opens with the
 * row in place instead of pushing the rows below it when the view arrives.
 */
export function useFastPackSync(playId: string | null, instancesRead: boolean) {
  const enabled = usePrefs((s) => s.fastPacks);
  useEffect(() => {
    void fastPackApi.configure(enabled).catch(() => {});
  }, [enabled]);
  useQuery({ ...fastPackStateQuery(playId), enabled: instancesRead });
}
