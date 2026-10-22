// Links in chat messages (CHAT2 §9; the service's "How links are checked", rule table version 2),
// as the composer checks them and the conversation draws them: where each link is, the first rule
// it fails, and whether it may be pressed now (it passes the rules and Studio's lists as they are
// now: a site blocked later turns back into plain text in older messages too) and opens without
// the one-line warning (a trusted site).
//
// Finding follows the contract's first step exactly. The checks here are the ones that need no
// rule table (scheme, credentials, IP, port, the look and length of the host, Discord invites while
// Studio keeps them off, Studio's block list); the table's own lists (shorteners, IP grabbers,
// adult domains, brand look-alikes, scams, files) come with CHAT2-R's pre-check in text.ts, which
// this module defers to once it is there. The service decides either way.
import type { ChatLinkRules } from "@/lib/api/social-types";

/** Why a link may not go or be pressed (the contract's `rule`; those checked here). */
export type LinkRule =
  | "http"
  | "credentials"
  | "ip"
  | "port"
  | "lookalike"
  | "form"
  | "long"
  | "invite"
  | "blocked";

export interface FoundLink {
  /** UTF-16 offsets in the text. */
  start: number;
  end: number;
  url: string;
  /** Lower case, without a trailing dot. */
  host: string;
  /** The first rule it fails, or null (a fourth link fails none and is still plain text). */
  rule: LinkRule | null;
  /** It may be pressed. */
  ok: boolean;
  /** A trusted site: it opens without the warning (and may get a picture in its card). */
  trusted: boolean;
}

const URL_CHAR = String.raw`[A-Za-z0-9\-._~:/?#\[\]@!$&'()*+,;=%\p{L}\p{M}\p{N}]`;
const LINK = new RegExp(String.raw`(?<![\p{L}\p{N}])https?:\/\/${URL_CHAR}*`, "giu");
const TRAILING = new Set([".", ",", ";", ":", "!", "?", "'", "*"]);
const LABEL = /^(?!-)[a-z0-9-]{1,63}(?<!-)$/;
/** The table's `invites`: refused while Studio keeps Discord invites off. */
const INVITES = ["discord.gg", "dsc.gg", "discord.com/invite", "discordapp.com/invite"];
/** A bare domain in the text ("youtube.com/x"): the refusal adds "https:// ile yaz.". */
const BARE = /(?<![\p{L}\p{N}@.\-/])[a-z0-9-]+(?:\.[a-z0-9-]+)*\.[a-z]{2,24}(?![\p{L}\p{N}])/iu;
/** The rules whose refusal has words of its own (the contract's table). */
const WORDED = new Set([
  "off",
  "form",
  "formHint",
  "count",
  "http",
  "long",
  "shortener",
  "lookalike",
  "file",
  "invite",
]);

/** A refusal's words: its own, or "Bu bağlantı gönderilemez." (`chat2.links.rules.<key>`). */
export const ruleWords = (rule: string): string => (WORDED.has(rule) ? rule : "other");

/** The text holds a bare domain (for the hint under a refused one). */
export const hasBareDomain = (text: string): boolean => BARE.test(text);

const count = (text: string, ch: string) => text.split(ch).length - 1;

/** A found link without what ends a sentence after it (the contract's step 1). */
function trimEnd(url: string): string {
  let out = url;
  for (;;) {
    const last = out.at(-1) ?? "";
    if (TRAILING.has(last)) out = out.slice(0, -1);
    else if (last === ")" && count(out, "(") < count(out, ")")) out = out.slice(0, -1);
    else if (last === "]" && count(out, "[") < count(out, "]")) out = out.slice(0, -1);
    else {
      // A Turkish case ending: https://youtu.be/x'i izle.
      const ending = /'\p{L}+$/u.exec(out);
      if (ending && ending.index > out.indexOf("://") + 3) out = out.slice(0, ending.index);
      else return out;
    }
  }
}

/** The link's host as the rules read it, and the first rule of its form it fails (step 3). */
function readLink(url: string): { host: string; rule: LinkRule | null } {
  const rest = url.slice(url.indexOf("://") + 3);
  const authority = rest.split(/[/?#]/u)[0] ?? "";
  const at = authority.lastIndexOf("@");
  const place = at >= 0 ? authority.slice(at + 1) : authority;
  const host = place.replace(/:.*$/su, "").toLowerCase().replace(/\.$/, "");
  if (!/^https:\/\//i.test(url)) return { host, rule: "http" };
  if (at >= 0) return { host, rule: "credentials" };
  if (place.startsWith("[") || /^[0-9.]+$/.test(host)) return { host, rule: "ip" };
  if (place.includes(":")) return { host, rule: "port" };
  if (!/^[\x21-\x7e]+$/.test(host) || host.split(".").some((l) => l.startsWith("xn--"))) {
    return { host, rule: "lookalike" };
  }
  const labels = host.split(".");
  const tld = labels.at(-1) ?? "";
  if (!(labels.length >= 2 && labels.every((l) => LABEL.test(l)) && /^[a-z]{2,24}$/.test(tld))) {
    return { host, rule: "form" };
  }
  if ([...url].length > 200) return { host, rule: "long" };
  return { host, rule: null };
}

/** The host or one of its parent domains is on the list. */
export function hostIn(host: string, list: readonly string[]): boolean {
  return list.some((entry) => {
    const site = entry.toLowerCase();
    return host === site || host.endsWith(`.${site}`);
  });
}

/**
 * A list names the link: its host (with its subdomains), or its host and the start of its path,
 * as whole segments (`youtube.com/@kanal` names `/@kanal/videos`, not `/@kanallar`).
 */
function listed(url: string, host: string, list: readonly string[]): boolean {
  const rest = url.slice(url.indexOf("://") + 3);
  const from = rest.search(/[/?#]/u);
  const path = decodeURIComponentSafe(
    from < 0 ? "" : (rest.slice(from).split(/[?#]/u)[0] ?? ""),
  ).toLowerCase();
  return list.some((entry) => {
    const [site = "", ...segments] = entry.toLowerCase().split("/");
    if (!(host === site || host.endsWith(`.${site}`))) return false;
    if (!segments.length) return true;
    const prefix = `/${segments.join("/")}`;
    return path === prefix || path.startsWith(`${prefix}/`);
  });
}

function decodeURIComponentSafe(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
}

/**
 * The links of a message's text, in order. Without rules (links off, an older service) none may
 * be pressed; with them, at most the first three that pass.
 */
export function findLinks(text: string, rules: ChatLinkRules | undefined): FoundLink[] {
  const out: FoundLink[] = [];
  let pressable = 0;
  for (const match of text.matchAll(LINK)) {
    const url = trimEnd(match[0]);
    if (url.length <= url.indexOf("://") + 3) continue;
    const start = match.index ?? 0;
    const { host, rule: formRule } = readLink(url);
    const rule =
      formRule ??
      (rules && !rules.invites && listed(url, host, INVITES)
        ? "invite"
        : rules && listed(url, host, rules.blocked)
          ? "blocked"
          : null);
    const ok = Boolean(rules) && rule === null && pressable < 3;
    if (ok) pressable += 1;
    out.push({
      start,
      end: start + url.length,
      url,
      host,
      rule,
      ok,
      trusted: ok && hostIn(host, rules?.trusted ?? []),
    });
  }
  return out;
}

/** The text in pieces: plain runs and links (for drawing). */
export type TextPiece =
  | { kind: "text"; text: string }
  | { kind: "link"; link: FoundLink; text: string };

export function piecesOf(text: string, links: readonly FoundLink[]): TextPiece[] {
  const pieces: TextPiece[] = [];
  let at = 0;
  for (const link of links) {
    if (link.start > at) pieces.push({ kind: "text", text: text.slice(at, link.start) });
    pieces.push({ kind: "link", link, text: text.slice(link.start, link.end) });
    at = link.end;
  }
  if (at < text.length) pieces.push({ kind: "text", text: text.slice(at) });
  return pieces;
}
