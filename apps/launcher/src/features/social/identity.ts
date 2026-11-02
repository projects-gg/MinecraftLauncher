// SOC-ID: the ways into the friends account, as the screens say them (pure rules; identity.test.ts):
// whose Discord a profile has, the player's own linked ways and the ones they could link, the
// "25 Ekim'de bağlandı" date, and the profile name's rules (the service checks them again).
import type {
  IdentityFailure,
  SocialIdentity,
  SocialMe,
  SocialProfile,
  SocialProvider,
} from "@/lib/api/social-types";

/** Every way the launcher knows, in the order the screens list them. */
export const PROVIDERS: readonly SocialProvider[] = ["discord", "microsoft"];

/**
 * The Discord account behind a profile, null without one. A profile that does not say (every
 * account opened with Discord, an older service) has its own id as its Discord id.
 */
export function discordIdOf(profile: Pick<SocialProfile, "id" | "discordId">): string | null {
  return profile.discordId === undefined ? profile.id : profile.discordId;
}

/**
 * The account's number is its Discord id: it was opened with Discord and keeps that Discord
 * account. The Projects skin and 7/24 Bulut work only for these for now (`discord_required`).
 */
export function discordIsAccount(me: Pick<SocialProfile, "id" | "discordId"> | undefined): boolean {
  return me !== undefined && discordIdOf(me) === me.id;
}

/**
 * The account has a Discord account linked, whether it was opened with Discord or linked one later
 * (MS-REST): the Projects skin works with that Discord id.
 */
export function hasDiscord(me: Pick<SocialProfile, "id" | "discordId"> | undefined): boolean {
  return me !== undefined && discordIdOf(me) !== null;
}

/**
 * A limited account that is new itself (opened with Microsoft): its lines say "Hesabın yeni
 * olduğu için…" instead of "Discord hesabın yeni olduğu için…".
 */
export function newAccount(me: Pick<SocialMe, "limited"> | undefined): boolean {
  return me?.limited?.reason === "account";
}

/** The player's ways in, Discord first; a profile that does not say: Discord alone. */
export function identitiesOf(me: Pick<SocialMe, "identities" | "username">): SocialIdentity[] {
  const ways = me.identities ?? [
    { provider: "discord" as const, linkedOn: "", isNew: false, hint: me.username },
  ];
  return [...ways].sort((a, b) => PROVIDERS.indexOf(a.provider) - PROVIDERS.indexOf(b.provider));
}

/** The ways the player could link now: offered for signing in, and not linked yet. */
export function linkableOf(
  me: Pick<SocialMe, "identities" | "username">,
  providers: readonly SocialProvider[] | undefined,
): SocialProvider[] {
  const linked = new Set(identitiesOf(me).map((way) => way.provider));
  return (providers ?? ["discord"]).filter((provider) => !linked.has(provider));
}

/** The ways in as the service offers them (`providers`, `signUpWays`; absent: Discord alone). */
export interface Ways {
  providers?: readonly SocialProvider[];
  signUpWays?: readonly SocialProvider[];
}

/**
 * How the sign-in offers Microsoft: `none` (the service does not), `linked` (it signs in, but new
 * accounts open with Discord only: it stands under "Bağlı hesabınla gir") or `open` (it opens
 * accounts too: an equal of Discord, and the invitations say "Giriş yap" without Discord's mark).
 */
export type MicrosoftOffer = "none" | "linked" | "open";

export function microsoftOffer(ways: Ways): MicrosoftOffer {
  if (!ways.providers?.includes("microsoft")) return "none";
  return ways.signUpWays?.includes("microsoft") ? "open" : "linked";
}

/**
 * "Profil adı" is offered: the service takes a name of the player's own (`ownNames`), and an
 * account with Discord picks one only while Microsoft signs in (`403 provider_off` otherwise); one
 * without Discord, or still with its placeholder, always names itself.
 */
export function ownNameOffered(
  me: Pick<SocialMe, "id" | "discordId" | "nameSource" | "ownNames">,
  offer: MicrosoftOffer,
): boolean {
  if (!me.ownNames) return false;
  return offer !== "none" || discordIdOf(me) === null || me.nameSource === "unset";
}

/** The way whose page proves a step: a link's own, a removal's the other (there are two). */
export function proofProvider(change: { provider: SocialProvider; unlink: boolean }) {
  if (!change.unlink) return change.provider;
  return change.provider === "discord" ? "microsoft" : "discord";
}

/** A failure's sentence key (`identity.errors.*`). */
export function failureKey(reason: IdentityFailure): string {
  return `identity.errors.${reason}`;
}

const TR_MONTHS = [
  "Ocak",
  "Şubat",
  "Mart",
  "Nisan",
  "Mayıs",
  "Haziran",
  "Temmuz",
  "Ağustos",
  "Eylül",
  "Ekim",
  "Kasım",
  "Aralık",
];
/** "Ekim'de": a month with the ending of "in" (Ocak'ta, Şubat'ta, … Eylül'de, Ekim'de). */
const TR_MONTHS_IN = [
  "Ocak'ta",
  "Şubat'ta",
  "Mart'ta",
  "Nisan'da",
  "Mayıs'ta",
  "Haziran'da",
  "Temmuz'da",
  "Ağustos'ta",
  "Eylül'de",
  "Ekim'de",
  "Kasım'da",
  "Aralık'ta",
];
const EN_MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** The Turkish ending of a year as it is read: 2026'da, 2023'te, 2030'da, 2040'ta, 2000'de. */
export function yearIn(year: number): string {
  const ones = year % 10;
  const tens = Math.floor(year / 10) % 10;
  // bir, iki, üç, dört, beş, altı, yedi, sekiz, dokuz; on, yirmi, otuz, kırk, elli, altmış,
  // yetmiş, seksen, doksan; yüz and bin.
  if (ones) return ["", "'de", "'de", "'te", "'te", "'te", "'da", "'de", "'de", "'da"][ones] ?? "";
  if (tens) return ["", "'da", "'de", "'da", "'ta", "'de", "'ta", "'te", "'de", "'da"][tens] ?? "";
  return "'de";
}

/**
 * When a way was linked, ready for "{{date}} bağlandı" / "Linked on {{date}}": "25 Ekim'de" /
 * "25 October", with the year when it is not this one ("3 Mart 2025'te"). Null for a date that
 * cannot be read.
 */
export function linkedDate(linkedOn: string, lang: "tr" | "en", nowMs = Date.now()): string | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(linkedOn);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  if (month < 1 || month > 12 || day < 1 || day > 31) return null;
  const sameYear = year === new Date(nowMs).getFullYear();
  if (lang === "en") {
    return sameYear ? `${day} ${EN_MONTHS[month - 1]}` : `${day} ${EN_MONTHS[month - 1]} ${year}`;
  }
  return sameYear
    ? `${day} ${TR_MONTHS_IN[month - 1]}`
    : `${day} ${TR_MONTHS[month - 1]} ${year}${yearIn(year)}`;
}

/** Turkish letters a name may hold besides `a`–`z`: the service's twelve (no circumflexed ones). */
const TURKISH_LETTERS = "çğıöşüÇĞİÖŞÜ";

function nameLetter(c: string): boolean {
  return /^[A-Za-z0-9]$/.test(c) || TURKISH_LETTERS.includes(c);
}

/**
 * The profile name as the service takes it, or null: 3–16 characters of Latin and Turkish
 * letters, digits, spaces and `_`, with at least two letters or digits. Spaces at the ends go and a
 * run of them becomes one (the same rule as `pl_social::clean_name`).
 */
export function cleanName(name: string): string | null {
  const cleaned = name.split(" ").filter(Boolean).join(" ");
  const chars = [...cleaned];
  if (chars.length < 3 || chars.length > 16) return null;
  if (!chars.every((c) => c === " " || c === "_" || nameLetter(c))) return null;
  return chars.filter(nameLetter).length >= 2 ? cleaned : null;
}

/** The name step's first guess: the active Minecraft account's name, when it passes the rules. */
export function suggestedName(
  accounts: ReadonlyArray<{ username: string; active: boolean }> | undefined,
): string {
  const active = accounts?.find((account) => account.active);
  return (active && cleanName(active.username)) ?? "";
}
