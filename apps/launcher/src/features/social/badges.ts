// Crew tags (TEAM6, team6.md §3–§5, §7), pure rules of the UI (badges.test.ts): what a tag's card
// offers whoever opens it (its bottom row and its menu), when "Katılma isteği gönder" shows (never a
// greyed button: it is there or it is not), what wearing a tag does and says, how a refusal reads
// and how long this device remembers it (the Turkey day), the crews the player may wear the tag
// of, the "Hoş geldin" lines of a crew's chat, and the corridor that keeps a card open while the
// pointer travels to it. The components word and draw them (crew-badge.tsx).
import type {
  BadgeRefusal,
  CrewBadge,
  CrewFeatures,
  CrewJoin,
  SocialTeam,
} from "@/lib/api/social-types";
import { MAX_OWN_REQUESTS } from "./crew-joining";
import { canAddCrew } from "./crews";

/** The tag a profile shows: only while the service offers tags (an older one, or Studio's switch
 * off, shows none at all). */
export function shownBadge(
  features: Pick<CrewFeatures, "badges">,
  badge: CrewBadge | undefined,
): CrewBadge | undefined {
  return features.badges ? badge : undefined;
}

// --- the card ---------------------------------------------------------------------------------------

/**
 * The card's bottom row: the wearer is the player ("Etiketi çıkar", "Ekibi aç"), the player is in
 * the crew ("Sen de bu ekiptesin", "Ekibi aç"), invited to it ("Kabul et"), waiting for its answer
 * ("Geri çek"), may ask ("Katılma isteği gönder"), or nothing at all (no row, no line).
 */
export type CardRow = "own" | "member" | "invited" | "waiting" | "ask" | null;

export interface CardInput {
  /** The crew of the tag, and whether it takes requests from its tag now. */
  badge: Pick<CrewBadge, "crew" | "requests">;
  /** The tag on the card is the player's own. */
  own: boolean;
  /** Its wearer is on the player's friend list (requests come only from the wearer's friends). */
  friend: boolean;
  /** The player's crews (ids). */
  crews: readonly string[];
  crewLimit: number;
  /** The service has several crews per player. */
  multi: boolean;
  /** Crews that invited the player (ids). */
  invites: readonly string[];
  /** Crews the player's join requests still wait at (ids). */
  waiting: readonly string[];
  /** The service takes requests from tags (`badgeRequests`). */
  requestsOn: boolean;
  /** This device was refused a request to this crew today (Turkey time). */
  refusedToday: boolean;
}

/** Whether "Katılma isteği gönder" shows (team6.md §4.6, "Düğme ne zaman görünür"). */
export function canAsk(input: CardInput): boolean {
  const { crew, requests } = input.badge;
  return (
    input.requestsOn &&
    requests &&
    input.friend &&
    !input.own &&
    !input.crews.includes(crew) &&
    !input.invites.includes(crew) &&
    !input.waiting.includes(crew) &&
    canAddCrew(input.crews, input.crewLimit, input.multi) &&
    input.waiting.length < MAX_OWN_REQUESTS &&
    !input.refusedToday
  );
}

export function cardRow(input: CardInput): CardRow {
  const { crew } = input.badge;
  if (input.own) return "own";
  if (input.crews.includes(crew)) return "member";
  if (input.invites.includes(crew)) return "invited";
  if (input.waiting.includes(crew)) return "waiting";
  return canAsk(input) ? "ask" : null;
}

/** The card's "…" (and a standalone chip's right click): the crew's whole menu for its members,
 * "Ekibi rapor et" for everyone else. */
export function cardMenu(row: CardRow): "crew" | "report" {
  return row === "own" || row === "member" ? "crew" : "report";
}

/**
 * The rows the card's bottom line may turn into while it is open, so they all share one cell and
 * the card keeps its size when one turns into another (a request sent: "İsteğin bekliyor"; refused:
 * one quiet line; an invite taken: "Sen de bu ekiptesin").
 */
export function rowsReachable(row: CardRow): Array<Exclude<CardRow, null> | "refused"> {
  switch (row) {
    case "own":
      return ["own"];
    case "member":
      return ["member"];
    case "invited":
      return ["invited", "member"];
    case "waiting":
    case "ask":
      return ["ask", "waiting", "refused"];
    default:
      return [];
  }
}

// --- refusals --------------------------------------------------------------------------------------

/**
 * The words of a refusal (the backend's answer to "Katılma isteği gönder", never an error): the
 * card says it in one quiet line and this device keeps the button away for that crew until the
 * answer's `until` (the end of the Turkey day); nothing is tried again. The player's own limits are
 * said in TEAM4's and TEAM5's words. `member`: in the crew already (the list shows it; no words).
 */
export function refusalWords(reason: BadgeRefusal): { key: string; count?: "crewLimit" } | null {
  switch (reason) {
    case "closed":
    case "today":
    case "waitingFull":
    case "full":
    case "blocked":
    case "ownDaily":
      return { key: `team6.card.refused.${reason}` };
    case "ownWaiting":
      return { key: "crewWave2.join.askLimit" };
    case "ownCrews":
      return { key: "crews.limit", count: "crewLimit" };
    case "member":
      return null;
  }
}

/** The end of today in Turkey (UTC+3 all year), in Unix seconds: a refusal lasts until then. */
export function turkeyDayEnd(nowMs: number): number {
  const day = 86_400_000;
  const offset = 3 * 3_600_000;
  return ((Math.floor((nowMs + offset) / day) + 1) * day) / 1000 - offset / 1000;
}

/** The crews this device was refused a request from a tag, and until when (Unix seconds). */
export type RefusalMemory = Record<string, number>;

export function refusedNow(memory: RefusalMemory, crew: string, nowMs: number): boolean {
  return (memory[crew] ?? 0) * 1000 > nowMs;
}

/** The memory with one more refused crew (those whose time passed are forgotten). */
export function withRefusal(
  memory: RefusalMemory,
  crew: string,
  until: number,
  nowMs: number,
): RefusalMemory {
  const kept = Object.entries(memory).filter(([, end]) => end * 1000 > nowMs);
  return Object.fromEntries([...kept.filter(([id]) => id !== crew), [crew, until]].slice(-50));
}

// --- wearing ---------------------------------------------------------------------------------------

/**
 * One of the player's crews and its tag: may be put on, is worn, or may not be worn now (its leader
 * hid the crew; moderators reset its texts and they stay locked).
 */
export type WearState = "wear" | "worn" | "hidden" | "locked";

export function wearState(
  team: Pick<SocialTeam, "id" | "hidden" | "textLockedUntil">,
  worn: string | null | undefined,
  nowMs: number,
): WearState {
  if (team.hidden) return "hidden";
  if (team.textLockedUntil && team.textLockedUntil * 1000 > nowMs) return "locked";
  return worn === team.id ? "worn" : "wear";
}

/** What a change of the worn tag says: put on, switched from another crew's, taken off. */
export function wearOutcome(
  before: string | null,
  after: string | null,
): "wore" | "switched" | "off" | null {
  if (before === after) return null;
  if (after === null) return "off";
  return before === null ? "wore" : "switched";
}

/** How many of a crew's members wear its tag ("Etiketi takan 8 üyenin etiketi çıkarılacak."). */
export function wearersOf(team: Pick<SocialTeam, "id" | "members">): number {
  return team.members.filter((member) => member.badge?.crew === team.id).length;
}

/** The crews whose tag the player may pick, in their order (the first is the primary crew); a crew
 * that cannot be worn now says why. */
export interface TagChoice {
  crew: SocialTeam;
  main: boolean;
  blocked: "hidden" | "locked" | null;
}

export function tagChoices(crews: readonly SocialTeam[], nowMs: number): TagChoice[] {
  return crews.map((crew, index) => {
    const state = wearState(crew, null, nowMs);
    return {
      crew,
      main: index === 0 && crews.length > 1,
      blocked: state === "hidden" || state === "locked" ? state : null,
    };
  });
}

// --- "Hoş geldin" (§7) ---------------------------------------------------------------------------

/** "Selam ver" shows beside the line: not on the player's own, once, until its ten minutes end. */
export function canGreet(join: Pick<CrewJoin, "mine" | "greetUntil">, nowMs: number): boolean {
  return !join.mine && join.greetUntil !== undefined && join.greetUntil * 1000 > nowMs;
}

/** When the next "Selam ver" of these goes by itself (Unix ms), for one timer; null: none waits. */
export function nextGreetEnd(joins: readonly CrewJoin[], nowMs: number): number | null {
  const ends = joins
    .filter((join) => canGreet(join, nowMs))
    .map((join) => (join.greetUntil ?? 0) * 1000);
  return ends.length ? Math.min(...ends) : null;
}

// --- the corridor (§4.3) ---------------------------------------------------------------------------

export interface Point {
  x: number;
  y: number;
}

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/**
 * The triangle between where the pointer left the chip and the card's near edge: while the pointer
 * moves inside it, it is on its way to the card and the card stays open. Null when the point is
 * over the card already.
 */
export function corridor(from: Point, card: Box): [Point, Point, Point] | null {
  if (from.y <= card.top) {
    return [from, { x: card.left, y: card.top }, { x: card.right, y: card.top }];
  }
  if (from.y >= card.bottom) {
    return [from, { x: card.left, y: card.bottom }, { x: card.right, y: card.bottom }];
  }
  if (from.x <= card.left) {
    return [from, { x: card.left, y: card.top }, { x: card.left, y: card.bottom }];
  }
  if (from.x >= card.right) {
    return [from, { x: card.right, y: card.top }, { x: card.right, y: card.bottom }];
  }
  return null;
}

/** The point lies inside the triangle (or on its edge). */
export function inTriangle(p: Point, [a, b, c]: [Point, Point, Point]): boolean {
  const side = (u: Point, v: Point, w: Point) =>
    (u.x - w.x) * (v.y - w.y) - (v.x - w.x) * (u.y - w.y);
  const d1 = side(p, a, b);
  const d2 = side(p, b, c);
  const d3 = side(p, c, a);
  const negative = d1 < 0 || d2 < 0 || d3 < 0;
  const positive = d1 > 0 || d2 > 0 || d3 > 0;
  return !(negative && positive);
}
