import { useEffect } from "react";
import { usePrefs } from "@/stores/prefs";
import { fastPackApi } from "./api";

/**
 * Keeps "Hızlı kaynak paketi yükleme" in step with the launcher (mounted once, with the command
 * bar): the switch goes to Rust at the start and on every change, so the pack syncs that run while
 * the page rests (background preparation, a game start) follow it.
 */
export function useFastPackSync() {
  const enabled = usePrefs((s) => s.fastPacks);
  useEffect(() => {
    void fastPackApi.configure(enabled).catch(() => {});
  }, [enabled]);
}
