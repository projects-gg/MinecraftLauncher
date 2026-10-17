import { PerfProvider, Toaster, TooltipProvider } from "@pl/ui";
import { focusManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { commands } from "@/bindings";
import { setLanguage } from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
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

// The saved language (always Turkish while English is hidden, see ENGLISH_ENABLED).
setLanguage(usePrefs.getState().language);

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

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 60_000, refetchOnWindowFocus: false, retry: 1 },
  },
});

function Providers() {
  const performanceMode = usePrefs((s) => s.performanceMode);
  // Ambient motion stops while the game runs (the GPU belongs to Minecraft then), while nobody
  // can see it (tray, minimized) and under the mandatory update screen, which covers the app.
  const gameRunning = useLaunch((s) => s.phase.kind === "running");
  const away = useWindowAway();
  const updateLocked = useUpdate((s) => s.mandatory && s.phase !== "idle");
  return (
    <PerfProvider
      performanceMode={performanceMode}
      ambientPaused={gameRunning || away || updateLocked}
      unseen={away}
    >
      <TooltipProvider>
        <QueryClientProvider client={queryClient}>
          <RouterProvider router={router} />
          <Toaster />
        </QueryClientProvider>
      </TooltipProvider>
    </PerfProvider>
  );
}

const root = document.getElementById("root");
if (!root) throw new Error("#root missing");

createRoot(root).render(
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

// Interface sounds are drawn in idle time, not on the first press that plays one.
prepareSounds();
