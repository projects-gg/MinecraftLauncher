import {
  guardNativeContextMenu,
  guardReducedMotion,
  PerfProvider,
  Toaster,
  TooltipProvider,
} from "@pl/ui";
import { focusManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { commands } from "@/bindings";
import { api } from "@/lib/api";
import i18n, { currentLanguage, startLanguage } from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { playerActive, trackPlayerIdle, usePlayerIdle } from "@/lib/player-idle";
import { prepareSounds } from "@/lib/sound";
import {
  isWindowAway,
  onWindowAwayChange,
  refreshWindowAway,
  trackWindowAway,
  useWindowAway,
} from "@/lib/window-away";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { useUpdate } from "@/stores/update";
import { router } from "./router";
import "./styles.css";

// Polling (server status and the like) and ambient motion pause while the window is in the tray
// or minimized (lib/window-away.ts). WebView2 keeps reporting the page as visible then, and the
// native focus is no guide either: on Windows a blur arrives about a second after the window is
// shown, while it is still in front (G6-R), and polls stopped while the player looked at them.
if (isTauri()) {
  trackWindowAway(getCurrentWindow());
  focusManager.setEventListener((handleFocus) => {
    handleFocus(!isWindowAway());
    return onWindowAwayChange((away) => handleFocus(!away));
  });
}

// Ambient motion plays while somebody uses the window (lib/player-idle.ts); the window coming
// back from the tray or the taskbar is the player coming back.
trackPlayerIdle(window);
onWindowAwayChange((away) => {
  if (!away) playerActive();
});

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 60_000, refetchOnWindowFocus: false, retry: 1 },
  },
});

// Made once: a change of the motion state below renders only what reads it, not the whole app
// (the player resting and coming back changes it every few seconds).
const app = (
  <TooltipProvider>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
      <Toaster />
    </QueryClientProvider>
  </TooltipProvider>
);

function Providers() {
  const performanceMode = usePrefs((s) => s.performanceMode);
  // Ambient motion stops while the game runs (the GPU belongs to Minecraft then), while nobody
  // can see it (tray, minimized) and under the mandatory update screen, which covers the app;
  // it winds down while nobody uses the window.
  const gameRunning = useLaunch((s) => s.phase.kind === "running");
  const away = useWindowAway();
  const updateLocked = useUpdate((s) => s.mandatory && s.phase !== "idle");
  const resting = usePlayerIdle();
  return (
    <PerfProvider
      performanceMode={performanceMode}
      ambientPaused={gameRunning || away || updateLocked}
      resting={resting}
      unseen={away}
    >
      {app}
    </PerfProvider>
  );
}

const root = document.getElementById("root");
if (!root) throw new Error("#root missing");

// Under reduced motion, nothing that moves runs as a Web Animation either (@pl/ui reduced-motion).
guardReducedMotion();
// WebView2's own menu (Back, Refresh, Print...) only in text fields and over selected text; our
// menus open as before (@pl/ui native-menu).
guardNativeContextMenu();

/**
 * The backend writes the tray menu, the Discord presence and its errors in the same language,
 * and keeps the player's pick for its next start (AFK accounts that start automatically connect
 * before this page runs).
 */
function tellBackendLanguage() {
  const choice = usePrefs.getState().language;
  void api.setLanguage(currentLanguage(), choice === "auto" ? null : choice).catch(() => {});
}

/**
 * A live language change: the backend follows, and the data the API layer put into words when it
 * was read (AFK drop reasons, the welcome greeting) is read again.
 */
function languageChanged() {
  tellBackendLanguage();
  void queryClient.invalidateQueries({ queryKey: ["afk"] });
  void queryClient.invalidateQueries({ queryKey: ["home-content"] });
}

async function start(rootElement: HTMLElement) {
  // The language first (the player's pick, else automatic; English loaded when needed), so the
  // first paint is already in it: the window is still hidden.
  await startLanguage(usePrefs.getState().language).catch(() => {});
  tellBackendLanguage();
  i18n.on("languageChanged", languageChanged);
  // A pick that keeps the words ("Otomatik" to "Türkçe" on a Turkish system) changes no language,
  // but the backend keeps it all the same.
  usePrefs.subscribe((state, previous) => {
    if (state.language !== previous.language) tellBackendLanguage();
  });

  createRoot(rootElement).render(
    <StrictMode>
      <Providers />
    </StrictMode>,
  );

  // The window starts hidden to avoid a white flash; reveal it after the first paint, unless the
  // OS started us at login (then we wait in the tray).
  if (isTauri()) {
    requestAnimationFrame(() => {
      void commands.appInfo().then(async (info) => {
        if (!info.startMinimized) await getCurrentWindow().show();
        await refreshWindowAway();
      });
    });
  }
}

void start(root);

// Interface sounds are drawn in idle time, not on the first press that plays one.
prepareSounds();
