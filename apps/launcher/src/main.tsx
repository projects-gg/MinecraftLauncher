import "@/lib/i18n";
import { PerfProvider, Toaster, TooltipProvider } from "@pl/ui";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { isTauri } from "@/lib/platform";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { router } from "./router";
import "./styles.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 60_000, refetchOnWindowFocus: false, retry: 1 },
  },
});

function Providers() {
  const performanceMode = usePrefs((s) => s.performanceMode);
  // Ambient motion stops while the game runs: the GPU belongs to Minecraft then.
  const gameRunning = useLaunch((s) => s.phase.kind === "running");
  return (
    <PerfProvider performanceMode={performanceMode} ambientPaused={gameRunning}>
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

// The window starts hidden to avoid a white flash; reveal it after the first paint.
if (isTauri()) {
  requestAnimationFrame(() => {
    void getCurrentWindow().show();
  });
}
