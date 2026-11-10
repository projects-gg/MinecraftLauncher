import { useMotionPrefs } from "@pl/ui";
import { useEffect, useMemo } from "react";
import type { PeopleChange } from "@/bindings";
import { attempt } from "@/features/social/actions";
import { useSocNext } from "@/features/social/next/flag";
import { useSocNextPrefs } from "@/features/social/next/prefs";
import { useStreamerOn } from "@/features/social/next/streamer";
import { useWaitPrefs } from "@/features/social/next/wait-prefs";
import { plainTemplates, usePlanTemplates } from "@/features/social/plan-templates";
import { changeStatus, changeStatusFor } from "@/features/social/status-menu";
import { useSocialPrefs, useSocialUi } from "@/features/social/store";
import { api } from "@/lib/api";
import { usePrefs } from "@/stores/prefs";
import { overlayApi } from "./api";
import { useOverlay } from "./store";
import { clientTweaks, TWEAK_KEYS } from "./tweaks";

/** The crew mute's durations a status may come for from the game (SOC-NEXT §3.3). */
const PRESETS = ["hour", "eightHours", "morning", "forever"] as const;

/**
 * Keeps the launcher's in-game overlay in step with the page (mounted once, with the friends
 * feature's host): the switch, the chat's emoji picker (CHAT-EMOJI), the mod's client tweaks
 * (DIALOG-FOCUS; a switch the crash help took off comes back too), the friends notification
 * choices its toasts follow (crew calls, crew chat, conversations muted here), the friend list's
 * favourites and "Çevrimiçi olunca bildir" (the game's person menu shows and changes them,
 * OVL-UX2), reduced motion and, with SOC-NEXT on (OVL-SOC 2), the nicknames, who is ignored,
 * streamer mode as it stands and a timed status's end go to Rust at the start and on every
 * change, since the page may rest while a game runs. Whether the game's mod is connected comes
 * back (`shownInGame`), and so do a switch the crash help turned off, the emoji picker switched
 * in the game, the marks the game's person menu changed and the status its switcher picked.
 */
export function useOverlaySync() {
  const enabled = usePrefs((s) => s.inGameOverlay);
  const emoji = usePrefs((s) => s.chatEmoji);
  // One line per client tweak (features/overlay/tweaks.ts).
  const dialogFocus = usePrefs((s) => s.dialogFocus);
  const crewCalls = useSocialPrefs((s) => s.notifyCrewCalls);
  const crewChat = useSocialPrefs((s) => s.notifyCrewChat);
  // SOC-NEXT: a friend the player ignores sends no card (their conversation counts as muted).
  const { mutes: chatMutes } = useWaitPrefs();
  const favorites = useSocialPrefs((s) => s.favorites);
  const onlineAlerts = useSocialPrefs((s) => s.onlineAlerts);
  const favoritesOnline = useSocialPrefs((s) => s.notifyFavoriteOnline);
  // OVL-SOC2: the game's cards for friends on Projects and for join requests follow these too.
  const favoritesProjects = useSocialPrefs((s) => s.notifyFavoriteProjects);
  const joinRequests = useSocialPrefs((s) => s.notifyJoinRequests);
  // "Animasyonları azalt", Performance mode or the system's reduced motion: the overlay's panel
  // and toasts appear in place too (its own "Animasyonları azalt" adds to it).
  const reducedMotion = !useMotionPrefs().layout;
  const socNext = useSocNext();
  const nicknames = useSocNextPrefs((s) => s.nicknames);
  const ignored = useSocNextPrefs((s) => s.ignored);
  const timer = useSocNextPrefs((s) => s.statusTimer);
  const streamer = useStreamerOn();
  // CREW-PLUS: the player's own crew plan templates, beside the game's built-in ones.
  const ownTemplates = usePlanTemplates((s) => s.own);
  const planTemplates = useMemo(() => plainTemplates(ownTemplates), [ownTemplates]);
  const next = useMemo(
    () =>
      socNext
        ? {
            nicknames: { ...nicknames },
            ignored: [...ignored],
            streamer,
            statusUntil: timer?.until ?? null,
            timedStatus: timer?.status ?? null,
          }
        : null,
    [socNext, nicknames, ignored, streamer, timer],
  );

  useEffect(() => {
    overlayApi
      .configure({
        enabled,
        emoji,
        tweaks: clientTweaks({ dialogFocus }),
        reducedMotion,
        notify: {
          crewCalls,
          crewChat,
          chatMutes: { ...chatMutes },
          favorites: [...favorites],
          onlineAlerts: { ...onlineAlerts },
          favoritesOnline,
          favoritesProjects,
          joinRequests,
        },
        next,
        planTemplates,
      })
      .catch(() => {
        // The next change (or the next start) tells it again.
      });
  }, [
    enabled,
    emoji,
    dialogFocus,
    crewCalls,
    crewChat,
    chatMutes,
    favorites,
    onlineAlerts,
    favoritesOnline,
    favoritesProjects,
    joinRequests,
    reducedMotion,
    next,
    planTemplates,
  ]);

  useEffect(() => {
    const { setConnected } = useOverlay.getState();
    const stopStatus = overlayApi.onStatus((status) =>
      setConnected(status.connected, status.kinds),
    );
    const stopEnabled = overlayApi.onEnabled((on) => usePrefs.getState().set("inGameOverlay", on));
    const stopEmoji = overlayApi.onEmoji((on) => usePrefs.getState().set("chatEmoji", on));
    const stopTweaks = overlayApi.onTweaks((tweaks) => {
      for (const key of TWEAK_KEYS) {
        const on = tweaks[key];
        if (on !== undefined) usePrefs.getState().set(key, on);
      }
    });
    // The game's person menu (OVL-UX2): the same marks as the list's star and its menu's row.
    const stopPeople = overlayApi.onPeople(onPeopleChange);
    overlayApi.state().then(
      (state) => setConnected(state.connected, state.kinds),
      () => {},
    );
    return () => {
      stopStatus();
      stopEnabled();
      stopEmoji();
      stopTweaks();
      stopPeople();
    };
  }, []);
}

/** A notification switch changed in the game's settings (OVL-SOC2): as Settings' own switch. */
function onNotifySwitch(key: string, on: boolean) {
  const prefs = useSocialPrefs.getState();
  switch (key) {
    case "favoritesOnline":
      prefs.set("notifyFavoriteOnline", on);
      break;
    case "favoritesProjects":
      prefs.set("notifyFavoriteProjects", on);
      break;
    case "crewChat":
      prefs.set("notifyCrewChat", on);
      break;
    case "joinRequests":
      prefs.set("notifyJoinRequests", on);
      break;
    case "crewCalls":
      // Where the service keeps the crews' switch, its word (`crew-notify.tsx`).
      if (useSocialUi.getState().view.crewNotifications) {
        void attempt(() => api.social.setCrewNotifications({ calls: on }));
      } else {
        prefs.set("notifyCrewCalls", on);
      }
      break;
  }
}

/** A mark or the status changed in the game: the page keeps it as its own buttons would. */
function onPeopleChange(change: PeopleChange) {
  if (change.kind === "notify") {
    onNotifySwitch(change.key, change.on);
    return;
  }
  if (change.kind === "status") {
    // OVL-SOC 2: the panel's status switcher, as the launcher's own (status-menu.tsx).
    const preset = PRESETS.find((one) => one === change.preset);
    if ((change.status === "dnd" || change.status === "invisible") && preset) {
      changeStatusFor(change.status, preset);
    } else if (
      change.status === "online" ||
      change.status === "dnd" ||
      change.status === "invisible"
    ) {
      // A status without an end drops the timer (as the launcher's own menu does).
      useSocNextPrefs.getState().setStatusTimer(null);
      changeStatus(change.status);
    }
    return;
  }
  const prefs = useSocialPrefs.getState();
  if (change.kind === "favorite") {
    if (prefs.favorites.includes(change.friend) !== change.on) {
      prefs.toggleFavorite(change.friend);
    }
  } else {
    prefs.setOnlineAlert(change.friend, change.on);
  }
}
