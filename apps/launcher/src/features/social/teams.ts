// Teams: pure rules of the UI (no React), each with a unit test (teams.test.ts): emblems, team
// codes as the player types them, names and tags checked the way the service checks them, and
// the order members are shown in.
import type { SocialTeam, TeamMember } from "@/lib/api/social-types";
import { isJoinable } from "./model";

/** The emblems the service accepts, in the picker's order (team-emblem.tsx draws them). */
export const EMBLEMS = [
  "sword",
  "shield",
  "crown",
  "flame",
  "bolt",
  "star",
  "skull",
  "diamond",
  "heart",
  "rocket",
  "tree",
  "paw",
  "ghost",
  "cube",
  "axe",
  "trophy",
  "moon",
  "flag",
  "target",
  "planet",
] as const;

export type Emblem = (typeof EMBLEMS)[number];

export const TEAM_NAME_MIN = 3;
export const TEAM_NAME_MAX = 24;
export const TEAM_TAG_MIN = 2;
export const TEAM_TAG_MAX = 4;

const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/**
 * What the player typed, shown as a team code while typing: upper case, the `TKM-` prefix,
 * hyphens after every four symbols, I/L → 1 and O → 0. A pasted message ("kod: TKM-…") gives
 * its code.
 */
export function formatTeamCodeInput(input: string): string {
  let upper = input.toUpperCase();
  const at = upper.lastIndexOf("TKM");
  if (at > 0) upper = upper.slice(at);
  let raw = upper.replace(/[^0-9A-Z]/g, "");
  if (raw.startsWith("TKM")) raw = raw.slice(3);
  raw = raw
    .replace(/[IL]/g, "1")
    .replace(/O/g, "0")
    .split("")
    .filter((c) => ALPHABET.includes(c))
    .join("")
    .slice(0, 8);
  if (!raw) return "";
  return `TKM-${raw.slice(0, 4)}${raw.length > 4 ? `-${raw.slice(4)}` : ""}`;
}

/** `TKM-XXXX-XXXX` when the input is a complete team code, else null. */
export function normalizeTeamCode(input: string): string | null {
  const formatted = formatTeamCodeInput(input);
  return /^TKM-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$/.test(formatted) ? formatted : null;
}

/** Spaces collapsed, invisible characters dropped (as the service stores the name). */
export function cleanTeamName(input: string): string {
  return input
    .normalize("NFC")
    .replace(/[\p{Cc}\p{Cf}\p{Co}\p{Cs}]/gu, "")
    .replace(/\s+/gu, " ")
    .trim();
}

const NAME_CHARS = /^[\p{L}\p{M}\p{N} '&!_-]+$/u;

/** Why a team name cannot be sent yet, or null. The service checks the word list itself. */
export function teamNameProblem(input: string): "short" | "long" | "chars" | null {
  const name = cleanTeamName(input);
  const size = [...name].length;
  if (size > TEAM_NAME_MAX) return "long";
  if (name && !NAME_CHARS.test(name)) return "chars";
  if (size < TEAM_NAME_MIN || (name.match(/[\p{L}\p{N}]/gu)?.length ?? 0) < 2) return "short";
  return null;
}

/** The tag as the player types it: Turkish upper case, letters and digits only, at most 4. */
export function formatTeamTag(input: string): string {
  return [
    ...input
      .replaceAll("i", "İ")
      .replaceAll("ı", "I")
      .toUpperCase()
      .replace(/[^A-Z0-9ÇĞİÖŞÜ]/g, ""),
  ]
    .slice(0, TEAM_TAG_MAX)
    .join("");
}

export function teamTagValid(tag: string): boolean {
  const size = [...tag].length;
  return size >= TEAM_TAG_MIN && size <= TEAM_TAG_MAX && /^[A-Z0-9ÇĞİÖŞÜ]+$/.test(tag);
}

const STATE_ORDER = { playing: 0, launcher: 1, offline: 2 } as const;
const collator = new Intl.Collator("tr", { sensitivity: "base" });

/**
 * Members other than the player, as the friend list shows people: in game first (projects.gg
 * first), then in the launcher, then offline; names in Turkish order.
 */
export function teammates(team: Pick<SocialTeam, "members">): TeamMember[] {
  return team.members
    .filter((m) => !m.me)
    .sort(
      (a, b) =>
        STATE_ORDER[a.state] - STATE_ORDER[b.state] ||
        Number(isJoinable(b.game)) - Number(isJoinable(a.game)) ||
        collator.compare(a.name, b.name),
    );
}

/** Teammates online now (the player not counted). */
export function teammatesOnline(team: Pick<SocialTeam, "members">): number {
  return team.members.filter((m) => !m.me && m.state !== "offline").length;
}

/** Everyone on the team page: the founder, then the rest as `teammates` orders them. */
export function rosterOrder(team: Pick<SocialTeam, "members">): TeamMember[] {
  const founder = team.members.filter((m) => m.role === "founder");
  const me = team.members.filter((m) => m.me && m.role !== "founder");
  const others = teammates(team).filter((m) => m.role !== "founder");
  return [...founder, ...me, ...others];
}
