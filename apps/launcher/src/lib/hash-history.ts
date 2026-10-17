// The launcher's address lives in the hash ("#/library/x?tab=mods"): the webview serves static
// files, so deep paths must not reach the asset server. TanStack's own hash history also reads
// the page's query string into the route when the page loads: "?emptyContent=1#/library/x?tab=mods"
// gave the tab "mods?emptyContent=1", and no tab was chosen (links inside the app were fine).
// This history reads the route from the hash alone and leaves the page's query string (the
// browser preview's switches) where it is.
import {
  createBrowserHistory,
  type HistoryLocation,
  type ParsedPath,
  type RouterHistory,
} from "@tanstack/react-router";

/** Tabs and line breaks go, other control characters are escaped (as TanStack does). */
function withoutControls(text: string): string {
  let out = "";
  for (const char of text) {
    const code = char.charCodeAt(0);
    if (code === 9 || code === 10 || code === 13) continue;
    out += code < 32 || code === 127 ? encodeURIComponent(char) : char;
  }
  return out;
}

/** The route in an address's hash: "#/library/x?tab=mods" is "/library/x" with "?tab=mods". */
export function hashRoute(hash: string): ParsedPath {
  const raw = withoutControls(hash.startsWith("#") ? hash.slice(1) : hash);
  // One leading slash: "//host" must never read as another site.
  const href = `/${raw.replace(/^[\\/]+/, "")}`;
  const hashAt = href.indexOf("#");
  const searchAt = href.indexOf("?");
  const searchEnd = hashAt === -1 ? href.length : hashAt;
  return {
    href,
    pathname: href.slice(0, Math.min(searchEnd, searchAt === -1 ? href.length : searchAt)),
    search: searchAt === -1 || searchAt > searchEnd ? "" : href.slice(searchAt, searchEnd),
    hash: hashAt === -1 ? "" : href.slice(hashAt),
  };
}

/** An entry written outside the router (a hash typed into the preview) gets a fresh state. */
function stateOf(state: unknown): HistoryLocation["state"] {
  if (state && typeof state === "object") return state as HistoryLocation["state"];
  const key = Math.random().toString(36).slice(2, 9);
  return { __TSR_index: 0, key, __TSR_key: key };
}

/** The router's history: the route from the hash alone; the page's query string stays put. */
export function createHashRouteHistory(win: Window = window): RouterHistory {
  return createBrowserHistory({
    window: win,
    parseLocation: () => ({ ...hashRoute(win.location.hash), state: stateOf(win.history.state) }),
    createHref: (href) => `${win.location.pathname}${win.location.search}#${href}`,
  });
}
