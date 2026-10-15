import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { AfkPage } from "./features/afk/afk-page";
import { HomePage } from "./features/home/home-page";
import { InstancePage } from "./features/library/instance-page";
import { NewsPage } from "./features/news/news-page";
import { LibraryPage } from "./features/library/library-page";
import { SettingsPage } from "./features/settings/settings-page";
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
  validateSearch: (search: Record<string, unknown>): { create?: boolean } => ({
    create: search.create === true || search.create === "true" ? true : undefined,
  }),
  component: LibraryPage,
});

const instanceRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/library/$instanceId",
  validateSearch: (search: Record<string, unknown>): { tab?: string } => ({
    tab: typeof search.tab === "string" ? search.tab : undefined,
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

// Hash history: the webview serves static files, so deep paths must not hit the asset server.
export const router = createRouter({ routeTree, history: createHashHistory(), defaultPreload: "intent" });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
