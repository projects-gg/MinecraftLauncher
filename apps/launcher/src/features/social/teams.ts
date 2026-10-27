// Crews ("Ekip"; teams inside): pure rules of the UI (no React), each with a unit test
// (teams.test.ts): crew codes as the player types them, names, tags and mottos checked the way
// the service checks them, who sits where on the crew page, what the player's role may do, and the
// crew's days. The look's data lives in crew-look.ts.
import type {
  PermissionLevel,
  SocialTeam,
  TeamMember,
  TeamPermissions,
} from "@/lib/api/social-types";
import { isJoinable } from "./model";

export { EMBLEMS, type Emblem } from "./crew-look";

export const TEAM_NAME_MIN = 3;
export const TEAM_NAME_MAX = 24;
export const TEAM_TAG_MIN = 2;
export const TEAM_TAG_MAX = 4;
export const MOTTO_MAX = 40;

const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/** The prefix players see. The service still sends `TKM-` (what launchers before crews take). */
export const CREW_PREFIX = "EKP";
/** The last whole crew code in a pasted message, with today's prefix or the one before crews. */
const PASTED_CODE = /(EKP|TKM)[\s_-]*([0-9A-Z]{4})[\s_-]*([0-9A-Z]{4})/g;

/**
 * What the player typed, shown as a crew code while typing: upper case, the `EKP-` prefix,
 * hyphens after every four symbols, I/L → 1 and O → 0. A pasted message ("Ekip kodu: EKP-…", an
 * older "TKM-…") gives its last whole code.
 */
export function formatTeamCodeInput(input: string): string {
  const upper = input.toUpperCase();
  const pasted = [...upper.matchAll(PASTED_CODE)].at(-1);
  let raw = pasted ? `${pasted[2]}${pasted[3]}` : upper.replace(/[^0-9A-Z]/g, "");
  if (!pasted && (raw.startsWith(CREW_PREFIX) || raw.startsWith("TKM"))) raw = raw.slice(3);
  raw = raw
    .replace(/[IL]/g, "1")
    .replace(/O/g, "0")
    .split("")
    .filter((c) => ALPHABET.includes(c))
    .join("")
    .slice(0, 8);
  if (!raw) return "";
  return `${CREW_PREFIX}-${raw.slice(0, 4)}${raw.length > 4 ? `-${raw.slice(4)}` : ""}`;
}

/** A friend code (`PRJ-…`) pasted where a crew code goes. */
const FRIEND_CODE = /PRJ[\s_-]*[0-9A-Z]{4}[\s_-]*[0-9A-Z]{4}/;

/** `EKP-XXXX-XXXX` when the input is a whole crew code, else null (a friend code is not one). */
export function normalizeTeamCode(input: string): string | null {
  const upper = input.toUpperCase();
  if (FRIEND_CODE.test(upper) && !upper.match(PASTED_CODE)) return null;
  const formatted = formatTeamCodeInput(input);
  return /^EKP-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$/.test(formatted) ? formatted : null;
}

/**
 * NFC, without control, format (zero-width, direction-changing), private-use, line and paragraph
 * separator characters, spaces collapsed and trimmed: how the service stores names and mottos.
 */
export function cleanCrewText(input: string): string {
  return input
    .normalize("NFC")
    .replace(/[\p{Cc}\p{Cf}\p{Co}\p{Cs}\p{Zl}\p{Zp}]/gu, "")
    .replace(/\s+/gu, " ")
    .trim();
}

/** Spaces collapsed, invisible characters dropped (as the service stores the name). */
export const cleanTeamName = cleanCrewText;

const NAME_CHARS = /^[\p{L}\p{M}\p{N} '&!_-]+$/u;
const MOTTO_CHARS = /^[\p{L}\p{M}\p{N} '&!_,?-]+$/u;
const LETTER_OR_DIGIT = /[\p{L}\p{N}]/gu;

/** Unicode code points, as the service counts them (combining marks count too). */
const size = (text: string) => [...text].length;

/** Why a crew name cannot be sent yet, or null. The service checks the word list itself. */
export function teamNameProblem(input: string): "short" | "long" | "chars" | null {
  const name = cleanCrewText(input);
  if (size(name) > TEAM_NAME_MAX) return "long";
  if (name && !NAME_CHARS.test(name)) return "chars";
  if (size(name) < TEAM_NAME_MIN || (name.match(LETTER_OR_DIGIT)?.length ?? 0) < 2) return "short";
  return null;
}

/**
 * Why a motto cannot be sent yet, or null (an empty motto is no motto). The same rule as the
 * service: 40 characters at most, the name's characters plus `,` and `?`, a letter or a digit.
 */
export function mottoProblem(input: string): "long" | "chars" | "letters" | null {
  const motto = cleanCrewText(input);
  if (!motto) return null;
  if (size(motto) > MOTTO_MAX) return "long";
  if (!MOTTO_CHARS.test(motto)) return "chars";
  if (!motto.match(LETTER_OR_DIGIT)) return "letters";
  return null;
}

/** The motto's length as the counter shows it ("14/40"). */
export function mottoLength(input: string): number {
  return size(cleanCrewText(input));
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
  const length = [...tag].length;
  return length >= TEAM_TAG_MIN && length <= TEAM_TAG_MAX && /^[A-Z0-9ÇĞİÖŞÜ]+$/.test(tag);
}

// --- who is where ---------------------------------------------------------------------------

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

/** Teammates online now (the player not counted; someone blocked shows offline). */
export function teammatesOnline(team: Pick<SocialTeam, "members">): number {
  return team.members.filter((m) => !m.me && !m.blocked && m.state !== "offline").length;
}

/** Teammates on projects.gg now ("Katıl"), the player not counted. */
export function crewOnProjects(team: Pick<SocialTeam, "members">): TeamMember[] {
  return teammates(team).filter((m) => !m.blocked && m.state === "playing" && isJoinable(m.game));
}

/** Everyone in the crew's order: the leader, the player, then the rest as `teammates` orders them. */
export function rosterOrder(team: Pick<SocialTeam, "members">): TeamMember[] {
  const founder = team.members.filter((m) => m.role === "founder");
  const me = team.members.filter((m) => m.me && m.role !== "founder");
  const others = teammates(team).filter((m) => m.role !== "founder");
  return [...founder, ...me, ...others];
}

/** Where a teammate is, as the crew page groups them. */
export type Whereabouts = "projects" | "playing" | "launcher" | "offline";

const WHERE_ORDER: Whereabouts[] = ["projects", "playing", "launcher", "offline"];

/** Where a teammate is (someone blocked shows offline, as everywhere). */
export function whereabouts(member: TeamMember): Whereabouts {
  if (member.blocked || member.state === "offline") return "offline";
  if (member.state === "launcher") return "launcher";
  return isJoinable(member.game) ? "projects" : "playing";
}

/** Rank inside a group: the leader, the moderators, then everyone else. */
const rank = (m: TeamMember) => (m.role === "founder" ? 0 : m.moderator ? 1 : 2);

/**
 * The crew page's groups of teammates ("Projects'te 4", "Oyunda 1", "Launcher'da 3",
 * "Çevrimdışı 4"), empty ones left out; inside each the leader, the moderators, then names in
 * Turkish order. The player is in none (their own row sits above them): the groups other than
 * "Çevrimdışı" add up to the card's "4 çevrimiçi" and the list header's count (SMALL12: the card
 * said "4 çevrimiçi" over groups of 3 and 2, the player counted in one).
 */
export function rosterGroups(
  team: Pick<SocialTeam, "members">,
): Array<{ where: Whereabouts; members: TeamMember[] }> {
  const crewmates = team.members.filter((m) => !m.me);
  return WHERE_ORDER.map((where) => ({
    where,
    members: crewmates
      .filter((m) => whereabouts(m) === where)
      .sort((a, b) => rank(a) - rank(b) || collator.compare(a.name, b.name)),
  })).filter((group) => group.members.length > 0);
}

// --- what the player's role may do ----------------------------------------------------------

type Role = Pick<SocialTeam, "role" | "moderator">;

/** Whether a role has a right at this level (a level is the lowest role that has it). */
export function allows(level: PermissionLevel, role: Role): boolean {
  if (role.role === "founder") return true;
  if (level === "members") return true;
  return level === "moderators" && role.moderator;
}

export interface CrewRights {
  /** Invite friends ("Arkadaşlarını davet et", "Ekibe davet et"). */
  invite: boolean;
  /** "Ekibi çağır" (the service offers calls). */
  call: boolean;
  /** Change the emblem, colour, frame and motto. */
  look: boolean;
  /** The name and the tag, permissions, moderators, the code, handing on, disbanding. */
  lead: boolean;
}

export function crewRights(
  team: Pick<SocialTeam, "role" | "moderator" | "permissions">,
  features: { calls: boolean; roles: boolean },
): CrewRights {
  const lead = team.role === "founder";
  return {
    invite: allows(team.permissions.invite, team),
    call: features.calls && allows(team.permissions.call, team),
    look: lead || (features.roles && allows(team.permissions.edit, team)),
    lead,
  };
}

/** Whether the player may remove this member: the leader anyone, a moderator plain members. */
export function canRemove(
  team: Pick<SocialTeam, "role" | "moderator" | "permissions">,
  member: Pick<TeamMember, "me" | "role" | "moderator">,
  roles: boolean,
): boolean {
  if (member.me || member.role === "founder") return false;
  if (team.role === "founder") return true;
  return roles && !member.moderator && allows(team.permissions.kick, team);
}

/** The rights the leader hands out, in the order the "Yetkiler" switches show them. */
export const RIGHTS = ["invite", "call", "kick", "edit"] as const;
export type Right = (typeof RIGHTS)[number];

/** Rights that plain members may have too (removing and editing stay with moderators at most). */
export function membersMayHave(right: Right): boolean {
  return right === "invite" || right === "call";
}

/** A level as two switches: "Moderatörler" and "Üyeler". */
export function switchesOf(level: PermissionLevel): { moderators: boolean; members: boolean } {
  return { moderators: level !== "leader", members: level === "members" };
}

/**
 * The level after a switch: members imply moderators (turning members on turns moderators on),
 * and moderators without the right leave members without it too.
 */
export function switchLevel(
  level: PermissionLevel,
  role: "moderators" | "members",
  on: boolean,
): PermissionLevel {
  if (role === "members") return on ? "members" : level === "members" ? "moderators" : level;
  return on ? (level === "leader" ? "moderators" : level) : "leader";
}

export const DEFAULT_PERMISSIONS: TeamPermissions = {
  invite: "members",
  call: "members",
  kick: "moderators",
  edit: "leader",
  admit: "moderators",
};

/** A crew can have this many moderators. */
export const MAX_MODERATORS = 4;

/**
 * Who the player becomes when they hand the leader role on: a moderator while fewer than 4 others
 * are (on a service with roles), else a member.
 */
export function afterHandingOn(
  team: Pick<SocialTeam, "members">,
  to: string,
  roles: boolean,
): "moderator" | "member" {
  if (!roles) return "member";
  const others = team.members.filter((m) => m.moderator && m.id !== to && !m.me).length;
  return others < MAX_MODERATORS ? "moderator" : "member";
}

// --- the crew's days ------------------------------------------------------------------------

/** `YYYY-MM-DD` as a local date (no time zone shift). */
export function localDay(day: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  if (!match) return null;
  return new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
}

/**
 * A month or year anniversary today: "Ekibin bugün 3 aylık", "1 yaşında". A crew founded on the
 * 31st has its day on the month's last day when the month is shorter. Null on other days.
 */
export function crewAnniversary(
  createdOn: string,
  today: Date,
): { months: number } | { years: number } | null {
  const founded = localDay(createdOn);
  if (!founded) return null;
  const lastOfMonth = new Date(today.getFullYear(), today.getMonth() + 1, 0).getDate();
  const day = Math.min(founded.getDate(), lastOfMonth);
  if (today.getDate() !== day) return null;
  const months =
    (today.getFullYear() - founded.getFullYear()) * 12 + today.getMonth() - founded.getMonth();
  if (months <= 0) return null;
  return months % 12 === 0 ? { years: months / 12 } : { months };
}

/** Whole days since `joinedAt` (Unix seconds), for "12 gündür ekipte". */
export function daysSince(joinedAt: number, nowMs: number): number {
  return Math.max(0, Math.floor((nowMs - joinedAt * 1000) / 86_400_000));
}
