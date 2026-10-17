// Opening the links that content carries (news, the hero, banners, events, Studio's links, mod
// pages). `projects://` links move inside the launcher, as one opened from outside would: the
// system opener only takes web pages, so handing it the launcher's own scheme did nothing. Web
// pages open in the system browser, `https://` only. Anything else is left alone.
import { api, type DeepLink } from "@/lib/api";
import { isWebLink, parseProjectsLink } from "@/lib/news";

/** A link that leaves the launcher (it may wear the external-link mark). */
export { isWebLink };

type Listener = (link: DeepLink) => void;
const listeners = new Set<Listener>();

/** Follows in-app links (the shell, shell/use-desktop-events.ts). */
export function onAppLink(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Whether `openLink` does something with `href` (otherwise it is shown as text). */
export function canOpenLink(href: string | undefined): href is string {
  return Boolean(href && (parseProjectsLink(href) || isWebLink(href)));
}

/** Opens a link from content: inside the launcher or in the system browser. */
export function openLink(href: string) {
  const link = parseProjectsLink(href);
  if (link) {
    for (const listener of listeners) listener(link);
  } else {
    openWebLink(href);
  }
}

/**
 * Opens a web page from text the launcher does not control (a mod's description or links, a
 * modpack's list): the system browser, `https://` only, never inside the launcher.
 */
export function openWebLink(href: string) {
  if (isWebLink(href)) api.openUrl(href).catch(() => undefined);
}
