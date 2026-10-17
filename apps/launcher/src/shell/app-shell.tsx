import { useQuery } from "@tanstack/react-query";
import { Outlet, useNavigate, useRouterState, useSearch } from "@tanstack/react-router";
import { useEffect } from "react";
import heroFallback from "@/assets/art/hero-dragon-1.webp";
import { DropHost } from "@/features/drop/drop-host";
import { EventReminders } from "@/features/home/event-reminders";
import { JavaPromptDialog } from "@/features/java/java-prompt";
import { ScreenshotNotice } from "@/features/library/screenshot-notice";
import { MigrationDialog } from "@/features/migrate/migration-dialog";
import { ModFlowDialog } from "@/features/modhealth/flow-dialog";
import { NewsReader } from "@/features/news/news-reader";
import { Onboarding, shouldShowOnboarding } from "@/features/onboarding/onboarding";
import { useCleanupReminder } from "@/features/settings/cleanup/reminder";
import {
  MandatoryUpdate,
  PackNotesDialog,
  useUpdateChecks,
  WhatsNew,
} from "@/features/updates/update-ui";
import { api } from "@/lib/api";
import { detectOs } from "@/lib/platform";
import {
  hasOpenOverlay,
  isTypingTarget,
  matchShortcut,
  pressPlayButton,
  SHORTCUT_PAGES,
  shortcutAllowed,
} from "@/lib/shortcuts";
import { usePrefs } from "@/stores/prefs";
import { useUi } from "@/stores/ui";
import { Backdrop } from "./backdrop";
import { CommandBar } from "./command-bar";
import { CommandPalette } from "./command-palette";
import { NavRail } from "./nav-rail";
import { TitleBar } from "./title-bar";
import { useDesktopEvents } from "./use-desktop-events";
import { usePresenceSync } from "./use-presence";
import { useTaskbarProgress } from "./use-taskbar";

// Hooks that follow fast-changing state (the launch's phase and progress, add-on downloads) live
// in leaf components that render nothing: called in AppShell they re-rendered the whole app,
// the open page included, on OYNA and on every progress tick (G6-P, item 8).
function PresenceSync() {
  usePresenceSync();
  return null;
}

function TaskbarProgress() {
  useTaskbarProgress();
  return null;
}

/**
 * Global keyboard shortcuts (lib/shortcuts.ts): Ctrl+K the palette, Ctrl+Enter OYNA, Ctrl+1 to
 * Ctrl+4 the rail's pages, Ctrl+, Settings; nothing while the welcome flow runs. The state is
 * read at the key press, so a shortcut never renders the app from here.
 */
function GlobalShortcuts({ disabled }: { disabled: boolean }) {
  const navigate = useNavigate();
  useEffect(() => {
    const os = detectOs();
    const onKey = (event: KeyboardEvent) => {
      const id = matchShortcut(event, os);
      if (!id) return;
      const ui = useUi.getState();
      const allowed = shortcutAllowed(id, {
        blocked: disabled,
        overlay: hasOpenOverlay(document),
        paletteOpen: ui.paletteOpen,
        typing: isTypingTarget(event.target),
        handled: event.defaultPrevented,
      });
      if (!allowed) return;
      // Ctrl+Enter on a focused button or link would press it as well.
      event.preventDefault();
      if (id === "palette") {
        ui.setPaletteOpen(!ui.paletteOpen);
      } else if (id === "play") {
        pressPlayButton(document);
      } else {
        if (ui.paletteOpen) ui.setPaletteOpen(false);
        void navigate({ to: SHORTCUT_PAGES[id] });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [disabled, navigate]);
  return null;
}

export function AppShell() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const search = useSearch({ strict: false }) as { news?: string };
  const navigate = useNavigate();
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const home = pathname === "/";
  const migrationOpen = useUi((s) => s.migrationOpen);
  const setMigrationOpen = useUi((s) => s.setMigrationOpen);
  // First run (account, v1 import, Projects instance) happens in the welcome flow.
  const onboarding = shouldShowOnboarding(usePrefs((s) => s.onboarded));
  const particlesOn = usePrefs((s) => s.particles);
  useUpdateChecks();
  useDesktopEvents();
  useCleanupReminder();

  return (
    <div className="flex h-dvh w-dvw overflow-hidden text-bone-100">
      <PresenceSync />
      <TaskbarProgress />
      <GlobalShortcuts disabled={onboarding} />
      <EventReminders />
      <Backdrop
        imageUrl={data?.hero.imageUrl ?? heroFallback}
        variant={home ? "home" : "page"}
        particles={particlesOn ? (data?.hero.particles ?? "embers") : "none"}
      />
      <NavRail />
      <div className="flex min-w-0 flex-1 flex-col">
        <TitleBar />
        {/* One page at a time: the route change is a view transition (router.tsx, styles.css),
            so the leaving page is a picture, not a second React tree rendering the new route. */}
        <main className="page-transition flex min-h-0 flex-1 flex-col">
          <Outlet />
        </main>
        <CommandBar />
      </div>
      {onboarding ? <Onboarding /> : null}
      <CommandPalette disabled={onboarding} />
      <ModFlowDialog />
      <JavaPromptDialog />
      <DropHost disabled={onboarding} />
      <ScreenshotNotice />
      <WhatsNew />
      <PackNotesDialog />
      <MandatoryUpdate />
      <MigrationDialog open={migrationOpen} onOpenChange={setMigrationOpen} />
      <NewsReader
        newsId={search.news}
        onClose={() =>
          void navigate({
            to: ".",
            search: (prev: Record<string, unknown>) => ({ ...prev, news: undefined }),
          })
        }
      />
    </div>
  );
}
