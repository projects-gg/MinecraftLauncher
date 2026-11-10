// Crew discovery ("Ekip bul", CREW-DISCOVER): the listing, joining from its cards and the
// leader's "Keşif" block. The service's README "Crew discovery" is the contract; pl-social reads
// it (discover.rs), the app's commands pass it on (commands/social_discover.rs).
import type { JoinMode } from "./social-types";

/** The tags a crew may pick, in the service's order (their words: `discover.tags.*`). */
export const DISCOVER_TAGS = [
  "survival",
  "skyblock",
  "smp",
  "hg",
  "pvp",
  "build",
  "events",
  "chat",
  "voice",
  "newcomers",
] as const;

export type DiscoverTag = (typeof DISCOVER_TAGS)[number];

/** The discovery text's length (characters), as the service counts it; tags at most. */
export const DISCOVER_TEXT_MIN = 10;
export const DISCOVER_TEXT_MAX = 160;
export const DISCOVER_MAX_TAGS = 3;

/** A kept listing is asked again (with its ETag) once it is this old. */
export const DISCOVER_REVALIDATE_MS = 15 * 60_000;

/** One listed crew: its card, the leader's text and tags, how to join, its size. */
export interface DiscoverCrew {
  id: string;
  name: string;
  tag: string;
  emblem: string;
  color: string;
  frame: string;
  motto: string;
  text: string;
  tags: DiscoverTag[];
  join: JoinMode;
  members: number;
  /**
   * CREW-CAP: its cap when the listing was made (absent from a service before crew caps, whose
   * crews held 30); shown beside `members` once nearly full.
   */
  maxMembers?: number;
  /** "Bu hafta aktif": enough members were on in the last 7 days. */
  active: boolean;
}

/** The listing in the day's order (the same for everyone, a new one each day). */
export interface DiscoverListing {
  ver: number;
  /** Unix seconds: when the wave built it, and the next wave (the copy is good until then). */
  at: number;
  next: number;
  crews: DiscoverCrew[];
}

export interface DiscoverPage {
  listing: DiscoverListing;
  etag: string | null;
}

export type DiscoverState = "off" | "waiting" | "listed" | "refused" | "closed" | "held";

export interface DiscoverText {
  text: string;
  tags: DiscoverTag[];
}

export interface DiscoverCheck {
  need: number;
  have: number;
}

/** The leader's "Keşif" block. */
export interface DiscoverStatus {
  state: DiscoverState;
  /** In discovery now (listed, or hidden by reports). */
  listed: DiscoverText | null;
  /** What the editor starts from: the latest text the leader sent. */
  draft: DiscoverText | null;
  /** A change of a listed crew waits for the wave. */
  changeWaits: boolean;
  /** Unix seconds. */
  waveAt: number;
  retryAt: number | null;
  checks: {
    members: DiscoverCheck;
    crewDays: DiscoverCheck;
    leaderDays: DiscoverCheck;
    visible: boolean;
  };
  editsLeft: number;
  /** Unix seconds: when the week's changes count again. */
  editsReset: number;
}

/** What joining from a card led to (`gone`: the card goes). */
export type DiscoverJoin = "joined" | "requested" | "gone";

export interface DiscoverApi {
  /** The listing, or null while the copy `etag` names is still current. */
  list(etag: string | null): Promise<DiscoverPage | null>;
  join(teamId: string): Promise<DiscoverJoin>;
  status(teamId: string): Promise<DiscoverStatus>;
  /** Asks the next wave; answers the block as it is now (a refusal shows in it). */
  set(teamId: string, text: string, tags: DiscoverTag[]): Promise<DiscoverStatus>;
  /** Out at once; null while the service's switch is off. */
  off(teamId: string): Promise<DiscoverStatus | null>;
}

/** Tags this launcher knows, each once, in the service's order. */
export function knownTags(tags: readonly string[]): DiscoverTag[] {
  return DISCOVER_TAGS.filter((tag) => tags.includes(tag));
}
