// Joining a crew beyond its code (TEAM5): invite links as players paste them, what "Katıl" does on
// a crew whose leaders approve newcomers, who answers join requests and how long one waits. Pure
// rules of the UI (no React), each with a unit test (crew-joining.test.ts).
import type {
  CrewFeatures,
  JoinRequest,
  SocialTeam,
  TeamLookup,
  TeamRequest,
} from "@/lib/api/social-types";
import { allows, formatTeamCodeInput, normalizeTeamCode } from "./teams";

/** The code alphabet (Crockford's): links use it too. */
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
export const CREW_LINK_LENGTH = 12;
/** A join request waits this long (seconds), then goes by itself. */
export const REQUEST_LIFETIME = 7 * 86_400;
/** A player has at most this many join requests waiting. */
export const MAX_OWN_REQUESTS = 3;

/** ASCII letters upper-cased, nothing else touched (as the service reads links). */
const asciiUpper = (text: string) => text.replace(/[a-z]/g, (c) => c.toUpperCase());

/** Codes the player types start like this; a link never has a separator after three symbols. */
const PREFIXED = /^\s*(EKP|TKM|PRJ)[\s_-]/i;

/**
 * `XXXX-XXXX-XXXX` when the input is a whole invite link (its 12 symbols in any case, spaces,
 * hyphens and underscores ignored, `O` read as 0 and `I`/`L` as 1), else null: the service's rule
 * (`pl_social::normalize_crew_link`). A crew or friend code is never one.
 */
export function normalizeCrewLink(input: string): string | null {
  if (input.length > 40 || PREFIXED.test(input)) return null;
  const raw = asciiUpper(input.replace(/[\s_-]/g, ""));
  if (raw.length !== CREW_LINK_LENGTH) return null;
  const symbols = raw.replace(/O/g, "0").replace(/[IL]/g, "1");
  if (![...symbols].every((c) => ALPHABET.includes(c))) return null;
  return `${symbols.slice(0, 4)}-${symbols.slice(4, 8)}-${symbols.slice(8)}`;
}

/** A link inside an address: its page (`…/e/<link>`) or the launcher's (`projects://crew/<link>`). */
const ROUTED_LINK =
  /(?:\/e\/|(?:crew|ekip)\/)([0-9a-z]{4}[\s_-]?[0-9a-z]{4}[\s_-]?[0-9a-z]{4})(?![0-9a-z])/gi;

/**
 * The invite link in what the player pasted: the last one in a message with its page's address or
 * a `projects://crew/…` link, or the whole text when it is one. Null when there is none.
 */
export function crewLinkIn(text: string): string | null {
  const routed = [...text.matchAll(ROUTED_LINK)].at(-1)?.[1];
  if (routed) return normalizeCrewLink(routed);
  return normalizeCrewLink(text.trim());
}

/** The field shows a link (or part of one) rather than a code (which always starts `EKP-`). */
export function showsLink(value: string): boolean {
  return value !== "" && !value.startsWith("EKP-");
}

/**
 * What the join field shows as the player types or pastes: a crew code as before (`EKP-…`), or an
 * invite link's symbols (`XXXX-XXXX-XXXX`) once a link was pasted (its address, a message with it,
 * or its symbols); editing a shown link keeps it a link.
 */
export function formatJoinInput(input: string, wasLink = false): string {
  const link = crewLinkIn(input);
  if (link) return link;
  if (wasLink && !PREFIXED.test(input) && !/[:/]/.test(input)) {
    const symbols = [...asciiUpper(input).replace(/O/g, "0").replace(/[IL]/g, "1")]
      .filter((c) => ALPHABET.includes(c))
      .slice(0, CREW_LINK_LENGTH)
      .join("");
    return symbols.match(/.{1,4}/g)?.join("-") ?? "";
  }
  return formatTeamCodeInput(input);
}

/** What the player asks to look up: a crew code or an invite link. */
export type CrewKey = { kind: "code"; code: string } | { kind: "link"; link: string };

/** The key in the field (as `formatJoinInput` shows it), or null when it is neither a whole code
 * nor a whole link: part of a link is never read as a code. */
export function crewKeyOf(value: string): CrewKey | null {
  const link = normalizeCrewLink(value);
  if (link) return { kind: "link", link };
  if (showsLink(value)) return null;
  const code = normalizeTeamCode(value);
  return code ? { kind: "code", code } : null;
}

type Admitting = Pick<SocialTeam, "role" | "moderator" | "permissions">;

/** The player's role answers join requests and manages the invite link here (`admit`). */
export function admits(team: Admitting): boolean {
  return allows(team.permissions.admit ?? "moderators", team);
}

/** "Katılma istekleri" for this player in this crew: the service has them and the role answers. */
export function answersRequests(team: Admitting, features: Pick<CrewFeatures, "requests">) {
  return features.requests && admits(team);
}

/** "Bağlantı oluştur", "Yenile", "Kapat": the service has links and the role manages them. */
export function managesLink(team: Admitting, features: Pick<CrewFeatures, "link">) {
  return features.link && admits(team);
}

/** A request still waits (7 days after it was sent it goes, without a word from the service). */
export function stillWaits(request: Pick<JoinRequest | TeamRequest, "sentAt">, nowMs: number) {
  return request.sentAt + REQUEST_LIFETIME > nowMs / 1000;
}

/** The crew's waiting requests the player answers, oldest first. */
export function waitingRequests(team: Pick<SocialTeam, "requests">, nowMs: number): JoinRequest[] {
  return (team.requests ?? []).filter((request) => stillWaits(request, nowMs));
}

/** What the found crew's button does, or why there is none. */
export type JoinOffer =
  | "join"
  | "ask"
  | "waiting"
  | "askLimit"
  | "member"
  | "limit"
  | "elsewhere"
  | "blocked"
  | "full";

/**
 * "Ekibe katıl" (open crews, and approving ones the player has an invite to), "Katılma isteği
 * gönder" (a crew whose leaders approve newcomers), "İsteğin bekliyor" (asked already), or why
 * neither goes: the crew's relation to the player, or three requests waiting already.
 */
export function joinOffer(
  lookup: Pick<TeamLookup, "relation" | "join" | "requestedAt">,
  context: { requests: boolean; invited: boolean; ownRequests: number },
): JoinOffer {
  if (lookup.relation !== "none") return lookup.relation;
  if (context.invited || !context.requests || lookup.join !== "approval") return "join";
  if (lookup.requestedAt !== undefined) return "waiting";
  return context.ownRequests >= MAX_OWN_REQUESTS ? "askLimit" : "ask";
}
