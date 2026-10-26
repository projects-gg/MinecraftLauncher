// Pure rules of the friends UI: grouping and ordering, presence wording, friend codes, which
// notices deserve the player's attention, and the settings backup document. No React here, so
// every rule has a unit test (model.test.ts).
import type { TFunction } from "i18next";
import type {
  CrewCall,
  CrewNotifications,
  Friend,
  ProfileMark,
  SocialGame,
  SocialNotice,
  SocialSnapshot,
  SocialTeam,
  StatusMode,
  TeamMember,
} from "@/lib/api/social-types";
import { unansweredCall } from "./calls";
import { type ChatMutes, unreadDirect } from "./chat/conversations";
import { waitingRequests } from "./crew-joining";
import { callHeardIn, crewCallsHeard, crewMutedNow, crewmateHeardIn, mutedNow } from "./mute";

const collator = new Intl.Collator("tr", { sensitivity: "base" });

export function isJoinable(game: SocialGame | undefined): boolean {
  return game?.where === "projects";
}

export function isOnline(friend: Pick<Friend, "state">): boolean {
  return friend.state !== "offline";
}

export interface FriendGroups {
  playing: Friend[];
  online: Friend[];
  offline: Friend[];
}

const SEEN_ORDER = { today: 0, yesterday: 1, week: 2, long: 3 } as const;

/**
 * In game, in the launcher, offline. Favourites lead each group; in game, projects.gg comes
 * before other places; offline friends seen recently come first; then names in Turkish order.
 */
export function groupFriends(
  friends: readonly Friend[],
  favorites: readonly string[],
): FriendGroups {
  const favorite = new Set(favorites);
  const byName = (a: Friend, b: Friend) => collator.compare(a.name, b.name);
  const favoriteFirst = (a: Friend, b: Friend) =>
    Number(favorite.has(b.id)) - Number(favorite.has(a.id));
  const playing = friends
    .filter((f) => f.state === "playing")
    .sort(
      (a, b) =>
        favoriteFirst(a, b) ||
        Number(isJoinable(b.game)) - Number(isJoinable(a.game)) ||
        byName(a, b),
    );
  const online = friends
    .filter((f) => f.state === "launcher")
    .sort((a, b) => favoriteFirst(a, b) || byName(a, b));
  const offline = friends
    .filter((f) => f.state === "offline")
    .sort(
      (a, b) =>
        favoriteFirst(a, b) ||
        (a.lastSeen ? SEEN_ORDER[a.lastSeen] : 9) - (b.lastSeen ? SEEN_ORDER[b.lastSeen] : 9) ||
        byName(a, b),
    );
  return { playing, online, offline };
}

/** Friends who are online, for the title bar: in game first, then the launcher. */
export function onlineFriends(friends: readonly Friend[], favorites: readonly string[]): Friend[] {
  const { playing, online } = groupFriends(friends, favorites);
  return [...playing, ...online];
}

/** Home's invitations, closed for good with ×: sign in with Discord, then add a first friend. */
export type HomeHint = "connect" | "add";
/** What the friends line on Home shows. */
export type HomeStrip = HomeHint | "playing" | "call" | "crew" | "callCrew" | "plan";

/** What else decides Home's line: calls closed there, a game being open, the call clock. */
export interface HomeContext {
  closedCalls: readonly string[];
  /** A game is being prepared or running: the crew's lines wait (the game is what matters). */
  gameBusy: boolean;
  now: number;
  /** "Ekibini çağır" would go right now: the player plays on a server a call may start from,
   * may call a crew, has no call on and is not waiting out the five minutes (CALL1). */
  callable?: boolean;
  /** A plan of the player's crews under way or starting within the hour (`homePlan`, TEAM5). */
  plan?: "live" | "soon" | null;
}

const NO_CONTEXT: HomeContext = { closedCalls: [], gameBusy: false, now: 0 };

/** Crewmates on projects.gg now ("Katıl") across the player's crews, each person once, the
 * player and anyone blocked left out. */
export function crewmatesOnProjects(crews: readonly { members: readonly TeamMember[] }[]) {
  const seen = new Set<string>();
  const out: TeamMember[] = [];
  for (const crew of crews) {
    for (const m of crew.members) {
      if (m.me || m.blocked || seen.has(m.id) || m.state !== "playing" || !isJoinable(m.game)) {
        continue;
      }
      seen.add(m.id);
      out.push(m);
    }
  }
  return out;
}

/**
 * The friends line on Home, most pressing first (one line at a time):
 * 1. a teammate's call the player has not answered (nor closed here), while no game is open;
 * 2. a crew plan under way (TEAM5), while no game is open;
 * 3. two or more crewmates on Projects across the crews, each once ("Ekibinden 4 kişi
 *    Projects'te", "Ekiplerinden 6 kişi…"), while no game is open;
 * 4. a crew plan starting within the hour, while no game is open;
 * 5. in game on a server a call may start from: "Ekibini çağır" (CALL1);
 * 6. friends in game, or the one crewmate on Projects (said as for a friend);
 * 7. signed out (or while the browser is open), the invitation to sign in; signed in with nobody
 *    on the list and no request either way, the invitation to add a first friend.
 * A closed invitation never comes back on this computer; nothing shows while a saved session is
 * checked or the list has not arrived.
 */
export function homeStrip(
  view: Pick<
    SocialSnapshot,
    "enabled" | "auth" | "loaded" | "friends" | "incoming" | "outgoing" | "invites"
  > &
    Partial<Pick<SocialSnapshot, "teams" | "crews" | "calls">>,
  closed: readonly HomeHint[],
  context: HomeContext = NO_CONTEXT,
): HomeStrip | null {
  if (!view.enabled) return null;
  if (view.auth === "signedOut" || view.auth === "signingIn") {
    return closed.includes("connect") ? null : "connect";
  }
  if (view.auth !== "signedIn") return null;
  const crews = view.teams ? (view.crews ?? []) : [];
  const crew = crewmatesOnProjects(crews);
  if (crews.length && !context.gameBusy) {
    if (unansweredCall(view.calls ?? [], context.now, context.closedCalls)) return "call";
    if (context.plan === "live") return "plan";
    if (crew.length >= 2) return "crew";
    if (context.plan === "soon") return "plan";
  }
  if (crews.length && context.callable) return "callCrew";
  if (view.friends.some((f) => f.state === "playing") || crew.length > 0) return "playing";
  const alone =
    view.loaded &&
    view.friends.length === 0 &&
    view.incoming.length === 0 &&
    view.outgoing.length === 0 &&
    view.invites.length === 0;
  return alone && !closed.includes("add") ? "add" : null;
}

/** What waits for the player behind the friends entry's gold dot. */
export interface Waiting {
  /** A friend request or a "Birlikte oyna" invite. */
  asks: boolean;
  /** A crewmate's call the player has not answered. */
  call: CrewCall | null;
  /** Unread direct messages, outside conversations muted here (CHAT1). */
  messages: number;
  /** Join requests waiting in crews whose requests the player answers, muted crews left out
   * (TEAM5). */
  joins: number;
}

type WaitingView = Pick<SocialSnapshot, "incoming" | "invites" | "calls"> &
  Partial<Pick<SocialSnapshot, "crewNotifications" | "crews" | "chat">>;

/**
 * What waits for the player (the friends entry's gold dot): a request or an invite, and a
 * crewmate's call they have not answered, unless crew calls are off or the crews it names are
 * muted (the call itself stays on the crew page and the Home line). The two are told apart: a
 * call is neither a request nor an invite, and the entry says which waits (E2E6). Unread direct
 * messages wait too, unless their conversation is muted here; crew chat only counts in the list.
 * Requests to join a crew the player answers wait as well, unless that crew is muted.
 */
export function whatWaits(
  view: WaitingView,
  localCrewCalls: boolean,
  nowMs: number,
  chatMutes: ChatMutes = {},
): Waiting {
  // The newest unanswered call of a crew that is not muted on its own.
  const call = crewCallsHeard(view.crewNotifications, localCrewCalls, nowMs)
    ? unansweredCall(
        view.calls.filter((one) => callHeardIn(one, view.crews ?? [], nowMs)),
        nowMs,
      )
    : undefined;
  const joins = (view.crews ?? [])
    .filter((crew) => !crewMutedNow(crew, view.crewNotifications, nowMs))
    .reduce((count, crew) => count + waitingRequests(crew, nowMs).length, 0);
  return {
    asks: view.incoming.length > 0 || view.invites.length > 0,
    call: call ?? null,
    messages: unreadDirect(view.chat, chatMutes, nowMs),
    joins,
  };
}

/** Something waits for the player (`whatWaits`): the gold dot is on. */
export function somethingWaits(
  view: WaitingView,
  localCrewCalls: boolean,
  nowMs: number,
  chatMutes: ChatMutes = {},
): boolean {
  const { asks, call, messages, joins } = whatWaits(view, localCrewCalls, nowMs, chatMutes);
  return asks || call !== null || messages > 0 || joins > 0;
}

/** Someone on Home's "playing" line: a friend in game, or a crewmate on Projects. */
export interface PlayingPerson {
  id: string;
  name: string;
  avatar?: string;
  avatarUrl?: string;
  game?: SocialGame;
  /** Studio's mark (SOC-VERIFY), shown right after the first person's name. */
  mark?: ProfileMark;
}

/**
 * Who Home's "playing" line is about: friends in game in the list's order, then crewmates on
 * Projects who are not friends (teammates see each other's game, never another server).
 */
export function playingPeople(
  view: Pick<SocialSnapshot, "friends"> & Partial<Pick<SocialSnapshot, "teams" | "crews">>,
  favorites: readonly string[],
): PlayingPerson[] {
  const friends: PlayingPerson[] = groupFriends(view.friends, favorites).playing;
  const known = new Set(view.friends.map((f) => f.id));
  const crew = crewmatesOnProjects(view.teams ? (view.crews ?? []) : []).filter(
    (m) => !known.has(m.id),
  );
  return [...friends, ...crew];
}

/** One short line under a friend's name. */
export function presenceLine(friend: Friend, t: TFunction): string {
  if (friend.state === "playing") {
    switch (friend.game?.where) {
      case "projects":
        return t("social.presence.projects");
      case "singleplayer":
        return t("social.presence.singleplayer");
      case "other":
        return friend.game.host
          ? t("social.presence.otherHost", { host: friend.game.host })
          : t("social.presence.other");
      default:
        return t("social.presence.playing");
    }
  }
  if (friend.state === "launcher") return t("social.presence.launcher");
  return friend.lastSeen
    ? t(`social.presence.seen.${friend.lastSeen}`)
    : t("social.presence.offline");
}

/** What the player's own card says they are doing. */
export function activityLine(snapshot: Pick<SocialSnapshot, "activity">, t: TFunction): string {
  const activity = snapshot.activity;
  if (activity.state === "singleplayer") return t("social.presence.singleplayer");
  if (activity.state === "playing") {
    if (!activity.host) return t("social.presence.playing");
    return isProjectsHost(activity.host)
      ? t("social.presence.projects")
      : t("social.presence.otherHost", { host: activity.host });
  }
  return t("social.presence.launcher");
}

export function isProjectsHost(host: string | undefined): boolean {
  if (!host) return false;
  const name = host.split(":")[0]?.toLowerCase() ?? "";
  return name === "projects.gg" || name.endsWith(".projects.gg");
}

// --- friend codes ----------------------------------------------------------------------------

const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/**
 * What the player typed, shown as a code while typing: upper case, the `PRJ-` prefix, hyphens
 * after every four symbols, letters Crockford does not use mapped (I, L → 1, O → 0). A pasted
 * share link or message ("https://social.projects.gg/f/PRJ-…", "kodum: PRJ-…") gives its code.
 */
export function formatCodeInput(input: string): string {
  let upper = input.toUpperCase();
  const at = upper.lastIndexOf("PRJ");
  if (at > 0) upper = upper.slice(at);
  let raw = upper.replace(/[^0-9A-Z]/g, "");
  if (raw.startsWith("PRJ")) raw = raw.slice(3);
  raw = raw
    .replace(/[IL]/g, "1")
    .replace(/O/g, "0")
    .split("")
    .filter((c) => ALPHABET.includes(c))
    .join("")
    .slice(0, 8);
  if (!raw) return "";
  return `PRJ-${raw.slice(0, 4)}${raw.length > 4 ? `-${raw.slice(4)}` : ""}`;
}

/** `PRJ-XXXX-XXXX` when the input is a complete code, else null (same rules as the service). */
export function normalizeCode(input: string): string | null {
  const formatted = formatCodeInput(input);
  return /^PRJ-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$/.test(formatted) ? formatted : null;
}

// --- notices ---------------------------------------------------------------------------------

export interface NoticePrefs {
  favorites: readonly string[];
  notifyFavoriteOnline: boolean;
  notifyFavoriteProjects: boolean;
  /** "Çevrimiçi olunca bildir" chosen for one friend (on or off); absent: the favourites' rule. */
  onlineAlerts?: Readonly<Record<string, boolean>>;
  /** "Ekip çağrıları" (on unless turned off). */
  notifyCrewCalls?: boolean;
  /** "Ekip arkadaşların Projects'e girince" (off unless turned on). */
  notifyCrewProjects?: boolean;
  /** "Katılma istekleri" (on unless turned off; TEAM5). */
  notifyJoinRequests?: boolean;
}

/**
 * Whether a crewmate joining Projects is said: when the player asked for it, and not when the
 * favourite friend notice already says it about the same person.
 */
export function notifiesCrewProjects(prefs: NoticePrefs, id: string): boolean {
  if (!prefs.notifyCrewProjects) return false;
  return !(prefs.notifyFavoriteProjects && prefs.favorites.includes(id));
}

/**
 * Whether the player hears about `id` coming online: their choice for that friend ("Çevrimiçi
 * olunca bildir"), else favourites while "Favorilerim çevrimiçi olunca" is on.
 */
export function notifiesOnline(prefs: NoticePrefs, id: string): boolean {
  return prefs.onlineAlerts?.[id] ?? (prefs.notifyFavoriteOnline && prefs.favorites.includes(id));
}

/**
 * Whether a notice should reach the player at all. Do-not-disturb silences everything (friends
 * still see the status); a friend coming online counts when the player asked for it (for that
 * friend, or for favourites), joining projects.gg only for favourites. Crews muted with "Sessize
 * al" (all of them, or every crew a call names or a crewmate shares, `crews`) say nothing (calls
 * stay on the crew page and the Home line); where the service keeps the crew settings (`crew`),
 * its "Ekip çağrıları" wins over this computer's. A request to join, and a crewmate's new plan,
 * are said unless their crew is muted (requests only while "Katılma istekleri" is on).
 */
export function wantsNotice(
  notice: SocialNotice,
  status: StatusMode | undefined,
  prefs: NoticePrefs,
  crew?: CrewNotifications,
  nowMs: number = Date.now(),
  crews: readonly SocialTeam[] = [],
): boolean {
  if (status === "dnd") {
    // Signing out still has to be said: the panel looks different afterwards (and so does a
    // suspension, which signs the player out).
    return notice.kind === "signedOutRemotely" || notice.kind === "restricted";
  }
  switch (notice.kind) {
    case "friendOnline":
      return notifiesOnline(prefs, notice.id);
    case "friendOnProjects":
      return prefs.notifyFavoriteProjects && prefs.favorites.includes(notice.id);
    case "invite":
      return !notice.invite.quiet;
    case "crewCall":
    case "crewCallJoin":
      return (
        crewCallsHeard(crew, prefs.notifyCrewCalls ?? true, nowMs) &&
        callHeardIn(notice.call, crews, nowMs)
      );
    case "crewmateOnProjects":
      return (
        !mutedNow(crew, nowMs) &&
        crewmateHeardIn(notice.id, crews, nowMs) &&
        notifiesCrewProjects(prefs, notice.id)
      );
    case "joinRequest":
      return (prefs.notifyJoinRequests ?? true) && !crewSilent(notice.crew, crews, crew, nowMs);
    case "crewPlan":
      return !crewSilent(notice.plan.crew, crews, crew, nowMs);
    default:
      return true;
  }
}

/** One of the player's crews is muted now (on its own, or every crew at once). */
function crewSilent(
  id: string,
  crews: readonly SocialTeam[],
  all: CrewNotifications | undefined,
  nowMs: number,
): boolean {
  const team = crews.find((one) => one.id === id);
  return team ? crewMutedNow(team, all, nowMs) : mutedNow(all, nowMs);
}

/** Remembers when each friend last caused a notification, so flapping friends stay quiet. */
export class NoticeThrottle {
  private readonly last = new Map<string, number>();
  constructor(private readonly gapMs: number) {}

  /** True when `key` may notify at `now` (and records it). */
  allow(key: string, now: number): boolean {
    const previous = this.last.get(key);
    if (previous !== undefined && now - previous < this.gapMs) return false;
    this.last.set(key, now);
    return true;
  }
}

// --- settings backup -------------------------------------------------------------------------

/**
 * Launcher preferences that travel with the account. Machine-specific or one-time things
 * (selected instance, memory, JVM arguments, start with the computer, first-run flags) stay
 * home; secrets never get here at all. Key names avoid every word the service rejects
 * (`session`, `token`, `password`, `secret`, `cookie`, `apiKey`, `afk`, `loginCommand`).
 */
export const BACKUP_PREFS = {
  performanceMode: "boolean",
  particles: "boolean",
  sounds: "boolean",
  soundVolume: "volume",
  autoJoin: "boolean",
  onGameStart: ["keep", "minimize", "tray", "close"],
  // Since TRAY1; a backup's older `closeToTray` (on by default, so no choice) stays behind.
  closeBehavior: ["quit", "tray"],
  updateChannel: ["stable", "beta"],
  autoDownloadUpdates: "boolean",
  discordEnabled: "boolean",
  discordShowServer: "boolean",
  concurrentDownloads: "downloads",
  verifyOnLaunch: "boolean",
  showSnapshots: "boolean",
} as const satisfies Record<string, "boolean" | "volume" | "downloads" | readonly string[]>;

export type BackupPrefKey = keyof typeof BACKUP_PREFS;

export interface BackupFriends {
  favorites: string[];
  notifyFavoriteOnline: boolean;
  notifyFavoriteProjects: boolean;
  /** Per-friend "Çevrimiçi olunca bildir"; left out while empty (older backups have none). */
  onlineAlerts?: Record<string, boolean>;
  /** "Ekip bildirimleri" (older backups have none: the defaults stay). */
  notifyCrewCalls?: boolean;
  notifyCrewProjects?: boolean;
  /** "Ekip sohbeti bildirimleri" (CHAT1; older backups have none). */
  notifyCrewChat?: boolean;
  /** "Katılma istekleri" and "Plan hatırlatmaları" (TEAM5; older backups have none). */
  notifyJoinRequests?: boolean;
  notifyPlans?: boolean;
}

/** Friend ids with a yes or no, at most 200 (what a backup may carry). */
function cleanAlerts(value: unknown): Record<string, boolean> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>)
      .filter(([id, on]) => isUserId(id) && typeof on === "boolean")
      .slice(0, 200),
  ) as Record<string, boolean>;
}

export interface BackupDocument {
  v: 1;
  prefs: Partial<Record<BackupPrefKey, unknown>>;
  friends: BackupFriends;
}

export function buildBackup(
  prefs: Partial<Record<BackupPrefKey, unknown>>,
  friends: BackupFriends,
): BackupDocument {
  const picked: Partial<Record<BackupPrefKey, unknown>> = {};
  for (const key of Object.keys(BACKUP_PREFS) as BackupPrefKey[]) {
    if (prefs[key] !== undefined) picked[key] = prefs[key];
  }
  const alerts = cleanAlerts(friends.onlineAlerts);
  return {
    v: 1,
    prefs: picked,
    friends: {
      favorites: [...new Set(friends.favorites)].filter(isUserId).slice(0, 200),
      notifyFavoriteOnline: friends.notifyFavoriteOnline,
      notifyFavoriteProjects: friends.notifyFavoriteProjects,
      ...(Object.keys(alerts).length ? { onlineAlerts: alerts } : {}),
      ...(typeof friends.notifyCrewCalls === "boolean"
        ? { notifyCrewCalls: friends.notifyCrewCalls }
        : {}),
      ...(typeof friends.notifyCrewProjects === "boolean"
        ? { notifyCrewProjects: friends.notifyCrewProjects }
        : {}),
      ...(typeof friends.notifyCrewChat === "boolean"
        ? { notifyCrewChat: friends.notifyCrewChat }
        : {}),
      ...(typeof friends.notifyJoinRequests === "boolean"
        ? { notifyJoinRequests: friends.notifyJoinRequests }
        : {}),
      ...(typeof friends.notifyPlans === "boolean" ? { notifyPlans: friends.notifyPlans } : {}),
    },
  };
}

function isUserId(value: unknown): value is string {
  return typeof value === "string" && /^\d{1,20}$/.test(value);
}

function validPref(key: BackupPrefKey, value: unknown): boolean {
  const rule = BACKUP_PREFS[key];
  if (rule === "boolean") return typeof value === "boolean";
  if (rule === "volume") return typeof value === "number" && value >= 0 && value <= 100;
  if (rule === "downloads") {
    return typeof value === "number" && Number.isInteger(value) && value >= 4 && value <= 48;
  }
  return typeof value === "string" && (rule as readonly string[]).includes(value);
}

/** What a downloaded backup may change here: known keys with valid values only. */
export function readBackup(data: unknown): {
  prefs: Partial<Record<BackupPrefKey, unknown>>;
  friends: Partial<BackupFriends>;
} {
  const out = {
    prefs: {} as Partial<Record<BackupPrefKey, unknown>>,
    friends: {} as Partial<BackupFriends>,
  };
  if (!data || typeof data !== "object") return out;
  const document = data as { prefs?: unknown; friends?: unknown };
  if (document.prefs && typeof document.prefs === "object") {
    const prefs = document.prefs as Record<string, unknown>;
    for (const key of Object.keys(BACKUP_PREFS) as BackupPrefKey[]) {
      if (validPref(key, prefs[key])) out.prefs[key] = prefs[key];
    }
  }
  if (document.friends && typeof document.friends === "object") {
    const friends = document.friends as Record<string, unknown>;
    if (Array.isArray(friends.favorites)) {
      out.friends.favorites = friends.favorites.filter(isUserId).slice(0, 200);
    }
    if (typeof friends.notifyFavoriteOnline === "boolean") {
      out.friends.notifyFavoriteOnline = friends.notifyFavoriteOnline;
    }
    if (typeof friends.notifyFavoriteProjects === "boolean") {
      out.friends.notifyFavoriteProjects = friends.notifyFavoriteProjects;
    }
    if (friends.onlineAlerts !== undefined) {
      out.friends.onlineAlerts = cleanAlerts(friends.onlineAlerts);
    }
    if (typeof friends.notifyCrewCalls === "boolean") {
      out.friends.notifyCrewCalls = friends.notifyCrewCalls;
    }
    if (typeof friends.notifyCrewProjects === "boolean") {
      out.friends.notifyCrewProjects = friends.notifyCrewProjects;
    }
    if (typeof friends.notifyCrewChat === "boolean") {
      out.friends.notifyCrewChat = friends.notifyCrewChat;
    }
    if (typeof friends.notifyJoinRequests === "boolean") {
      out.friends.notifyJoinRequests = friends.notifyJoinRequests;
    }
    if (typeof friends.notifyPlans === "boolean") {
      out.friends.notifyPlans = friends.notifyPlans;
    }
  }
  return out;
}

/** A stable fingerprint of a backup, to skip uploads that would change nothing. */
export function backupFingerprint(document: BackupDocument): string {
  const sortKeys = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(sortKeys);
    if (value && typeof value === "object") {
      return Object.fromEntries(
        Object.keys(value as Record<string, unknown>)
          .sort()
          .map((key) => [key, sortKeys((value as Record<string, unknown>)[key])]),
      );
    }
    return value;
  };
  return JSON.stringify(sortKeys(document));
}
