import { observeSkippedViewTransitions } from "@pl/ui";
import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { AfkPage } from "./features/afk/afk-page";
import { HomePage } from "./features/home/home-page";
import { InstancePage } from "./features/library/instance-page";
import { LibraryPage } from "./features/library/library-page";
import { NewsPage } from "./features/news/news-page";
import { SettingsPage } from "./features/settings/settings-page";
import { createHashRouteHistory } from "./lib/hash-history";
import { AppShell } from "./shell/app-shell";

interface RootSearch {
  /** Open news article overlay (works on every page, also for projects:// deep links). */
  news?: string;
}

const rootRoute = createRootRoute({
  component: AppShell,
  validateSearch: (search: Record<string, unknown>): RootSearch => ({
    news: typeof search.news === "string" ? search.news : undefined,
  }),
});

const homeRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: HomePage });

const libraryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/library",
  validateSearch: (
    search: Record<string, unknown>,
  ): { create?: boolean; source?: "import" | "sets" } => ({
    create: search.create === true || search.create === "true" ? true : undefined,
    // The create dialog opens on its file import ("İçe aktar") or its ready-made sets.
    source: search.source === "import" || search.source === "sets" ? search.source : undefined,
  }),
  component: LibraryPage,
});

const instanceRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/library/$instanceId",
  validateSearch: (
    search: Record<string, unknown>,
  ): { tab?: string; browse?: string; log?: string; focus?: string } => ({
    tab: typeof search.tab === "string" ? search.tab : undefined,
    // Modrinth browser for "mod" | "resourcePack" | "shaderPack".
    browse: typeof search.browse === "string" ? search.browse : undefined,
    // Logs tab: the captured session to show first (the crash dialog's "show log").
    log: typeof search.log === "string" ? search.log : undefined,
    // Settings tab: the group the command palette came for ("graphics" | "memory"), taken once.
    focus: typeof search.focus === "string" ? search.focus : undefined,
  }),
  component: InstancePage,
});

const newsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/news",
  component: NewsPage,
});

const afkRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/afk",
  component: AfkPage,
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  validateSearch: (search: Record<string, unknown>): { section?: string } => ({
    section: typeof search.section === "string" ? search.section : undefined,
  }),
  component: SettingsPage,
});

const routeTree = rootRoute.addChildren([
  homeRoute,
  libraryRoute,
  instanceRoute,
  newsRoute,
  afkRoute,
  settingsRoute,
]);

interface LocationChange {
  fromLocation?: unknown;
  pathChanged: boolean;
}

/**
 * Page changes cross-fade as a view transition of type "page" (styles.css): the browser keeps a
 * picture of the leaving page, so only the new page is ever mounted. Not for search-only changes
 * (tabs, the news overlay, settings sections) or the first render, which animate on their own,
 * and not while a dialog, sheet or menu is open or closing: the page picture is drawn above the
 * rest of the window for the moment it runs, and would cover the overlay's own exit.
 * Engines without view transition types change pages in place.
 */
function pageTransition(): false | { types: (change: LocationChange) => string[] | false } {
  if (typeof document === "undefined" || typeof document.startViewTransition !== "function") {
    return false;
  }
  if (!globalThis.CSS?.supports?.("selector(:active-view-transition-type(a))")) return false;
  return {
    types: ({ fromLocation, pathChanged }) => {
      if (!fromLocation || !pathChanged) return false;
      // A tooltip (a rail link's label, still fading out after the click) does not count.
      const overlay = document.querySelector(
        '[role="dialog"], [role="alertdialog"], [role="menu"], [data-radix-popper-content-wrapper]:not(:has([role="tooltip"]))',
      );
      return overlay ? false : ["page"];
    },
  };
}

// A page change that starts mid-transition skips the older one; its rejection is expected and
// observed (`@pl/ui`), not reported as an uncaught "AbortError: Transition was skipped".
observeSkippedViewTransitions();

// Hash history: the webview serves static files, so deep paths must not hit the asset server.
// The route is read from the hash alone (lib/hash-history.ts): deep links to a tab work on load.
export const router = createRouter({
  routeTree,
  history: createHashRouteHistory(),
  defaultPreload: "intent",
  defaultViewTransition: pageTransition(),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
