// Small text helpers for news content (the home column, the reader and the mock API) and the
// links content carries.
import { normalizeCode } from "@/features/social/model";
import type { DeepLink } from "@/lib/api/types";

/** First few bullet points of a Markdown text, as plain text. */
export function highlights(markdown: string, max = 4): string[] {
  return markdown
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => /^([-*+]|\d+[.)])\s+/.test(line))
    .map((line) =>
      line
        .replace(/^([-*+]|\d+[.)])\s+/, "")
        .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
        .replace(/[*_`]/g, "")
        .trim(),
    )
    .filter(Boolean)
    .slice(0, max);
}

/**
 * Lines of the patch notes box for a news item (under its title, so one line fewer than a
 * pack changelog): its bullet points, else its summary.
 */
export function newsHighlights(body: string, summary: string, max = 2): string[] {
  const bullets = highlights(body, max);
  return bullets.length ? bullets : [summary].filter(Boolean);
}

/**
 * A web page the system browser may open: `https://` with a host and no credentials. With
 * `projects://` links (`parseProjectsLink`) these are the safe links of docs/content-format.md
 * ("News feed"); anything else in content is shown as text.
 */
export function isWebLink(href: string | undefined): href is string {
  if (!href || href !== href.trim()) return false;
  try {
    const url = new URL(href);
    return url.protocol === "https:" && url.hostname !== "" && !url.username && !url.password;
  } catch {
    return false;
  }
}

/** Ids that may travel in a link: 1–64 ASCII letters, digits, `-`, `_` or `.`. */
function validId(id: string): boolean {
  return /^[A-Za-z0-9._-]{1,64}$/.test(id) && id !== "." && id !== "..";
}

/**
 * Where a `projects://<route>[/<id>]` link leads, read as the launcher reads links that come
 * from outside (`pl_desktop::link::parse_link`); null for anything unknown, never guessed.
 */
export function parseProjectsLink(raw: string): DeepLink | null {
  let url: URL;
  try {
    url = new URL(raw.trim());
  } catch {
    return null;
  }
  if (url.protocol !== "projects:") return null;
  // `projects://news/x` puts "news" in the host; `projects:news/x` keeps it in the path.
  const parts = [url.hostname, ...url.pathname.split("/")].filter(Boolean);
  if (parts.length > 2) return null;
  const route = (parts[0] ?? "home").toLowerCase();
  const id = parts[1];
  if (id === undefined) {
    if (route === "home" || route === "open") return { kind: "home" };
    if (route === "play" || route === "join" || route === "oyna") return { kind: "play" };
    if (route === "afk") return { kind: "afk" };
    return null;
  }
  if (route === "friend" || route === "arkadas") {
    const code = normalizeCode(id);
    return code ? { kind: "friend", code } : null;
  }
  if (!validId(id)) return null;
  if (route === "play" || route === "oyna") return { kind: "playInstance", id };
  if (route === "news" || route === "haber") return { kind: "news", id };
  if (route === "instance" || route === "kurulum") return { kind: "instance", id };
  return null;
}
