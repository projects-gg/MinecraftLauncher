import { useMotionPrefs } from "@pl/ui";
import { useEffect } from "react";
import { useSocialPrefs } from "@/features/social/store";
import { usePrefs } from "@/stores/prefs";
import { overlayApi } from "./api";
import { useOverlay } from "./store";

/**
 * Keeps the launcher's in-game overlay in step with the page (mounted once, with the friends
 * feature's host): the switch, the friends notification choices its toasts follow (crew calls,
 * crew chat, conversations muted here) and reduced motion go to Rust at the start and on every
 * change, since the page may rest while a game runs. Whether the game's mod is connected comes
 * back (`shownInGame`), and so does a switch the crash help turned off.
 */
export function useOverlaySync() {
  const enabled = usePrefs((s) => s.inGameOverlay);
  const crewCalls = useSocialPrefs((s) => s.notifyCrewCalls);
  const crewChat = useSocialPrefs((s) => s.notifyCrewChat);
  const chatMutes = useSocialPrefs((s) => s.chatMutes);
  // Performance mode or the system's reduced motion: the overlay's toasts appear in place.
  const reducedMotion = !useMotionPrefs().layout;

  useEffect(() => {
    overlayApi
      .configure({
        enabled,
        reducedMotion,
        notify: { crewCalls, crewChat, chatMutes: { ...chatMutes } },
      })
      .catch(() => {
        // The next change (or the next start) tells it again.
      });
  }, [enabled, crewCalls, crewChat, chatMutes, reducedMotion]);

  useEffect(() => {
    const { setConnected } = useOverlay.getState();
    const stopStatus = overlayApi.onStatus((status) => setConnected(status.connected));
    const stopEnabled = overlayApi.onEnabled((on) => usePrefs.getState().set("inGameOverlay", on));
    overlayApi.state().then(
      (state) => setConnected(state.connected),
      () => {},
    );
    return () => {
      stopStatus();
      stopEnabled();
    };
  }, []);
}
