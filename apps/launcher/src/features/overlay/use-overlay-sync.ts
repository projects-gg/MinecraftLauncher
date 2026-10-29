import { useMotionPrefs } from "@pl/ui";
import { useEffect } from "react";
import { useSocialPrefs } from "@/features/social/store";
import { usePrefs } from "@/stores/prefs";
import { overlayApi } from "./api";
import { useOverlay } from "./store";

/**
 * Keeps the launcher's in-game overlay in step with the page (mounted once, with the friends
 * feature's host): the switch, the chat's emoji picker (CHAT-EMOJI), the friends notification
 * choices its toasts follow (crew calls, crew chat, conversations muted here), the friend list's
 * favourites and "Çevrimiçi olunca bildir" (the game's person menu shows and changes them,
 * OVL-UX2) and reduced motion go to Rust at the start and on every change, since the page may
 * rest while a game runs. Whether the game's mod is connected comes back (`shownInGame`), and so
 * do a switch the crash help turned off, the emoji picker switched in the game and the marks the
 * game's person menu changed.
 */
export function useOverlaySync() {
  const enabled = usePrefs((s) => s.inGameOverlay);
  const emoji = usePrefs((s) => s.chatEmoji);
  const crewCalls = useSocialPrefs((s) => s.notifyCrewCalls);
  const crewChat = useSocialPrefs((s) => s.notifyCrewChat);
  const chatMutes = useSocialPrefs((s) => s.chatMutes);
  const favorites = useSocialPrefs((s) => s.favorites);
  const onlineAlerts = useSocialPrefs((s) => s.onlineAlerts);
  const favoritesOnline = useSocialPrefs((s) => s.notifyFavoriteOnline);
  // "Animasyonları azalt", Performance mode or the system's reduced motion: the overlay's panel
  // and toasts appear in place too (its own "Animasyonları azalt" adds to it).
  const reducedMotion = !useMotionPrefs().layout;

  useEffect(() => {
    overlayApi
      .configure({
        enabled,
        emoji,
        reducedMotion,
        notify: {
          crewCalls,
          crewChat,
          chatMutes: { ...chatMutes },
          favorites: [...favorites],
          onlineAlerts: { ...onlineAlerts },
          favoritesOnline,
        },
      })
      .catch(() => {
        // The next change (or the next start) tells it again.
      });
  }, [
    enabled,
    emoji,
    crewCalls,
    crewChat,
    chatMutes,
    favorites,
    onlineAlerts,
    favoritesOnline,
    reducedMotion,
  ]);

  useEffect(() => {
    const { setConnected } = useOverlay.getState();
    const stopStatus = overlayApi.onStatus((status) => setConnected(status.connected));
    const stopEnabled = overlayApi.onEnabled((on) => usePrefs.getState().set("inGameOverlay", on));
    const stopEmoji = overlayApi.onEmoji((on) => usePrefs.getState().set("chatEmoji", on));
    // The game's person menu (OVL-UX2): the same marks as the list's star and its menu's row.
    const stopPeople = overlayApi.onPeople((change) => {
      const prefs = useSocialPrefs.getState();
      if (change.kind === "favorite") {
        if (prefs.favorites.includes(change.friend) !== change.on) {
          prefs.toggleFavorite(change.friend);
        }
      } else {
        prefs.setOnlineAlert(change.friend, change.on);
      }
    });
    overlayApi.state().then(
      (state) => setConnected(state.connected),
      () => {},
    );
    return () => {
      stopStatus();
      stopEnabled();
      stopEmoji();
      stopPeople();
    };
  }, []);
}
