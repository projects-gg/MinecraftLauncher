// Friends ("Arkadaşlar"): UI-facing shapes. The Tauri adapter (social-tauri.ts) maps the generated
// bindings onto them; the mock (mock-social.ts) fakes every state for browser previews.

/** Çevrimiçi / Rahatsız etmeyin / Görünmez. */
export type StatusMode = "online" | "dnd" | "invisible";
export type AuthPhase = "signedOut" | "signingIn" | "restoring" | "signedIn";
/** The presence connection as the player should understand it. */
export type SocialConnection =
  | "idle"
  | "connecting"
  | "online"
  | "reconnecting"
  | "unavailable"
  | "replaced";
export type PresenceState = "offline" | "launcher" | "playing";
export type GameWhere = "projects" | "other" | "singleplayer";
/** Coarse "last seen": today, yesterday, this week, longer ago. */
export type LastSeen = "today" | "yesterday" | "week" | "long";
export type Relation = "none" | "friend" | "outgoing" | "incoming" | "blocked" | "self";
/** Why a sign-in did not finish: `restricted` (suspended or banned; `restriction` says until
 * when) and `signupsClosed` (no new accounts right now) come from the service. */
export type SignInFailure = "expired" | "unavailable" | "failed" | "restricted" | "signupsClosed";

export interface SocialGame {
  where: GameWhere;
  /** Another server's address, only when the friend shares it. */
  host?: string;
}

/**
 * A mark Projects' staff gave someone in Studio (SOC-VERIFY): `verified` ("Doğrulanmış hesap",
 * blue) or `admin` ("Projects yöneticisi", red), right of the name wherever they show.
 */
export type ProfileMark = "verified" | "admin";

/** Someone shown with name and picture. */
export interface SocialProfile {
  id: string;
  name: string;
  username: string;
  /** `discord`, `preset:<slug>` or `minecraft:<texture hash>`. */
  avatar: string;
  /** Discord picture when `avatar` is `discord`. */
  avatarUrl?: string;
  /** Their mark (SOC-VERIFY); none without one and from a service without marks. */
  mark?: ProfileMark;
}

/**
 * The crew tag a player wears before their name (TEAM6): the card of one of their crews, made from
 * the crew's own row at every read (never a copy). Only friends and crewmates get it, and only
 * while the crew exists, is not hidden, its texts are not locked and Studio has tags on.
 */
export interface CrewBadge extends CrewLook {
  /** The crew's id. */
  crew: string;
  /** 2-4 letters or digits, upper case. */
  tag: string;
  name: string;
  /** Empty when there is none. */
  motto: string;
  /** `YYYY-MM-DD`. */
  createdOn: string;
  /** How many are in the crew. */
  members: number;
  /** The crew takes join requests from its tag now: its setting, Studio's switch, not full and
   * fewer than 10 such requests today (the service decides; the player's own conditions are the
   * launcher's). */
  requests: boolean;
}

export interface Friend extends SocialProfile {
  /** `YYYY-MM-DD`. */
  since?: string;
  lastSeen?: LastSeen;
  state: PresenceState;
  dnd: boolean;
  game?: SocialGame;
  /** Their Discord profile ("Discord'dan mesaj at"). */
  discordUrl: string;
  /** The crew tag they wear (TEAM6); none while they wear none. */
  badge?: CrewBadge;
}

export interface FriendRequest extends SocialProfile {
  /** Unix seconds. */
  sentAt: number;
}

/** "Birlikte oyna". */
export interface SocialInvite {
  id: string;
  from: string;
  name?: string;
  avatar?: string;
  avatarUrl?: string;
  /** Their mark (SOC-VERIFY), from the friend list. */
  mark?: ProfileMark;
  /** Unix seconds. */
  at: number;
  server?: SocialGame;
  /** It waited (offline or do-not-disturb): listed quietly, never a toast. */
  quiet: boolean;
}

export interface BackupMeta {
  version: number;
  /** Unix seconds. */
  updatedAt: number;
  device: string;
  size: number;
}

export interface QrCode {
  size: number;
  path: string;
}

export interface SocialMe extends SocialProfile {
  /** The Discord picture (or Discord's default), for the picker. */
  discordAvatarUrl: string;
  /** `PRJ-XXXX-XXXX`. */
  friendCode: string;
  shareLink?: string;
  qr?: QrCode;
  status: StatusMode;
  /** `shareRead` ("Görüldü bilgisi") and `shareTyping` ("Yazıyor bilgisi"): CHAT2, both ways
   * (off: neither shown nor seen); absent on a service without them. */
  privacy: {
    shareServer: boolean;
    shareLastSeen: boolean;
    shareRead?: boolean;
    shareTyping?: boolean;
  };
  backup?: BackupMeta;
  /** A limited account ("Sınırlı hesap", a new Discord account); only the player sees it. */
  limited?: LimitedAccount;
  /** The crew tag the player wears (TEAM6); none while they wear none. */
  badge?: CrewBadge;
}

/** What a limited account may not do, or does less of. */
export type LimitedKey =
  | "crewFound"
  | "crewInvites"
  | "codeLookups"
  | "settingsBackup"
  | "chatMessages"
  /** Making or renewing a crew's invite link (TEAM5). */
  | "crewLink"
  /** Making crew plans; answering them is fine (TEAM5). */
  | "crewPlans"
  /** CHAT2: links in chat messages. */
  | "chatLinks"
  /** CHAT2: 20 stickers a day (`perDay`). */
  | "chatStickers"
  /** CHAT2: 100 reaction changes a day (`perDay`). */
  | "chatReactions"
  /** SKIN1: changing the Projects skin (the service's `skin_change`). */
  | "skinChange";

export interface LimitedAccount {
  /** Unix seconds: the limits lift by themselves then. */
  until: number;
  items: Array<{ key: LimitedKey; perDay?: number }>;
}

/** A one-time note from Projects' moderators. */
export interface ModNotice {
  id: string;
  /** `skinReset` (SKIN1): staff in game reset the player's Projects skin. */
  kind: "warning" | "profileReset" | "skinReset";
  /** Unix seconds. */
  at: number;
  reason?: ReportReason;
  /** `skinReset`: skin changes stay off until then (`until` null: for good); none: no block. */
  blocked?: { until: number | null };
}

/** A report reason, as players pick it. */
export type ReportReason =
  | "name"
  | "profile"
  | "harassment"
  | "inappropriate"
  | "spam"
  | "scam"
  | "other";

/** What the player reports ("Rapor et"). */
export type ReportTarget =
  /**
   * `plan`: reported from a plan card (its maker; FIX2), the plan as the moderators' evidence;
   * `reaction`: from a reaction's list (who gave it; CHAT2), the reaction as the evidence.
   */
  | {
      kind: "player";
      id: string;
      plan?: { crew: string; id: string };
      reaction?: { conv: string; id: number; e: string };
    }
  /** `code`: the crew was looked up by code; `via`: reported from the tag a friend or crewmate wears
   * (TEAM6: that player's id, never kept by the service). */
  | { kind: "crew"; id: string; code?: string; via?: string }
  /** A message as this computer holds it (with the messages before it as context). */
  | { kind: "message"; conv: string; id: number };

// --- chat (CHAT1) ---------------------------------------------------------------------------

/** Why one of the player's messages did not go. */
export type SendFailure =
  | "link"
  | "phone"
  | "words"
  | "invalid"
  | "tooFast"
  | "dailyLimit"
  | "muted"
  | "off"
  | "notAllowed"
  | "connection"
  /** CHAT2: the day's stickers are used up (100; 20 for a limited account). */
  | "stickers"
  /** CHAT2: the day's messages with links are used up (30). */
  | "links"
  /** CHAT2: a limited account sends no links. */
  | "limitedLinks"
  /** CHAT2: Studio switched this extra off (replies, stickers, links). */
  | "featureOff";

/** One message of a conversation. */
export interface ChatMessage {
  /** Stays the same from typing to sent (`n:<nonce>` for the player's own, else `i:<id>`). */
  key: string;
  /** The service's id (its clock, ms): undefined while on its way. */
  id?: number;
  from: string;
  mine: boolean;
  text: string;
  /** Unix ms on this computer's clock. */
  at: number;
  /** "düzenlendi". */
  edited: boolean;
  state: "sent" | "sending" | "failed";
  failure?: SendFailure;
  /** With `failure: "link"`: the link rule that refused it (chat-links.json), for the right words. */
  failureRule?: string;
  /** Unix ms: may be edited until then (the player's own, 15 minutes). */
  editUntil?: number;
  /** Unix ms: may be deleted for everyone until then (own, or any in a crew led or moderated). */
  deleteUntil?: number;
  /** Deleted for everyone: a tombstone in its place (the text is gone, nothing may be done with
   * it). `by`: who deleted it, a user id, `staff` (Projects' moderators) or empty (not known). */
  deleted?: ChatDeletion;
  /** CHAT2: a sticker ("pl1/gg") in place of text (`text` is empty); never edited. */
  sticker?: string;
  /** CHAT2: the message it answers, as this computer holds it. */
  reply?: ChatReply;
  /** CHAT2: its reactions as the player sees them (people they block or who block them left
   * out): kinds in the order they were first given, people in the order they reacted. */
  reactions?: ChatReaction[];
  /** CHAT2: its first link's preview card, kept with it on this computer once asked for. */
  card?: ChatCard;
}

/** The message a reply answers (CHAT2): the service keeps only the link, the text is this
 * computer's own copy. */
export interface ChatReply {
  id: number;
  from: string;
  /** Its key on this computer, when it holds it (to scroll to it). */
  key?: string;
  /** What it said, as held here. */
  text?: string;
  sticker?: string;
  /** Deleted for everyone: "Mesaj silindi". */
  deleted: boolean;
  /** Not on this computer (older than its history, or from someone no longer seen): "Önceki bir
   * mesaj", nothing to scroll to. */
  missing: boolean;
}

/** One kind of reaction on a message (CHAT2). */
export interface ChatReaction {
  /** One fully qualified emoji. */
  e: string;
  /** User ids, in the order they reacted. */
  by: string[];
  /** The player is among them. */
  mine: boolean;
}

/** The site icons a card may name (the launcher's own, never a site's favicon). */
export type ChatCardIcon =
  | "projects"
  | "youtube"
  | "twitch"
  | "kick"
  | "modrinth"
  | "discord"
  | "instagram"
  | "tiktok"
  | "x"
  | "globe";

/** A link's preview card, as the hub made it (CHAT2 §9): never fetched by this computer. */
export interface ChatCard {
  /** `ready`: the card; `plain`: nothing to show (the icon and the domain); `refused`: the link
   * leads somewhere the rules refuse (plain text, not clickable, "Bu bağlantı açılamaz."). */
  state: "ready" | "plain" | "refused";
  url: string;
  /** A name to show ("YouTube"), else the domain. */
  site: string;
  icon: ChatCardIcon;
  /** `video` and `stream`: a 16:9 picture; `project`: a square icon beside the text; `page`:
   * text only. */
  kind: "video" | "stream" | "project" | "page";
  title?: string;
  author?: string;
  desc?: string;
  /** A `data:image/…;base64,` picture (trusted sites only), `w` × `h` pixels. */
  image?: string;
  w?: number;
  h?: number;
  /** With `refused`: the rule. */
  rule?: string;
}

/** What chat offers beyond CHAT1 right now: the service took the capability and Studio has it on
 * (CHAT2 §2). Absent: an older service, none of them. */
export interface ChatExtras {
  replies: boolean;
  stickers: boolean;
  reactions: boolean;
  typing: boolean;
  reads: boolean;
  links: boolean;
}

/** The link rules the service applies now (chat-links.json's version and Studio's lists). */
export interface ChatLinkRules {
  rules: number;
  /** Hosts (with their subdomains) whose links open without the warning. */
  trusted: string[];
  /** Hosts, or a host and the start of a path, that no link may lead to. */
  blocked: string[];
  /** Discord invites may be linked. */
  invites: boolean;
}

/** Someone writes in a conversation ("yazıyor…", CHAT2 §4): shown for 6 seconds after the last. */
export interface ChatTyping {
  conv: string;
  by: string;
}

/** Who deleted a message for everyone, and when (Unix ms on this computer's clock). */
export interface ChatDeletion {
  by: string;
  at?: number;
}

/** One conversation in the list: a crew's chat (`c:<team id>`) or a friend's (`d:<id>:<id>`). */
export interface Conversation {
  id: string;
  /** The friend of a direct conversation. */
  peer?: string;
  /** The crew of a crew's chat. */
  crew?: string;
  /** Who or what it is with, as last known (for one no longer in the lists). */
  title?: string;
  /** The newest message; `deletedBy` (who deleted it, as `ChatDeletion.by`) when it is a
   * tombstone, its text empty then; `sticker` (CHAT2) for a sticker ("Çıkartma · GG"). */
  last?: {
    from: string;
    mine: boolean;
    text: string;
    at: number;
    deletedBy?: string;
    sticker?: string;
  };
  unread: number;
  /** The player may write here now (still friends, still in the crew). */
  writable: boolean;
  /** A message of the player's did not go. */
  failed: boolean;
  /** CHAT2, a direct conversation: the friend has seen up to this message id (shared while both
   * keep "Görüldü bilgisi" on). */
  peerRead?: number;
}

export interface ChatState {
  /** The service's switch: off hides the composer; what this computer holds stays readable. */
  on: boolean;
  /** False: messages reach only those online right now. */
  mailbox: boolean;
  /** The player's daily limit. */
  perDay: number;
  /** Unix seconds: a moderator muted the player until then. */
  mutedUntil?: number;
  conversations: Conversation[];
  /** CHAT2: what is on beyond CHAT1 (absent: none). */
  extras?: ChatExtras;
  /** CHAT2: the sticker sets the service takes ("pl1"). */
  stickerSets?: string[];
  /** CHAT2: the link rules while links are on. */
  links?: ChatLinkRules;
}

/** A conversation's messages, oldest first (`more`: older ones remain on this computer). */
export interface ChatPage {
  conv: string;
  messages: ChatMessage[];
  more: boolean;
}

/** What changed in one conversation: add or replace by key, take keys away, or start over. */
export interface ChatChange {
  conv: string;
  upsert: ChatMessage[];
  remove: string[];
  reset: boolean;
}

/** What this launcher tells friends. */
export type SocialActivity =
  | { state: "launcher" }
  | { state: "playing"; host?: string }
  | { state: "singleplayer" };

/** One of the player's own friend categories ("Okul", "PvP"). */
export interface FriendGroup {
  /** 1-12 lower case letters or digits, made here. */
  id: string;
  /** 1-24 characters. */
  name: string;
}

/** The player's friend categories: their order, and friend id → category id. Friends without an
 * entry are in the implicit "Arkadaşlar" category. Only the player sees them. */
export interface FriendGroups {
  version: number;
  groups: FriendGroup[];
  members: Record<string, string>;
}

/** "founder" is the crew's leader ("Lider"); the wire keeps the first name. */
export type TeamRole = "founder" | "member";
/** How a team code relates to the player; `limit`: in as many crews as they may be. */
export type TeamRelation = "none" | "member" | "elsewhere" | "blocked" | "full" | "limit";
/** Why the player is no longer in their team. */
export type TeamExit = "left" | "kicked" | "disbanded" | "other";
/** Who may do something in a crew: the lowest role that may (every role above may too). */
export type PermissionLevel = "members" | "moderators" | "leader";

/** A crew's permissions (defaults from a service without roles: everyone invites, only the
 * leader removes and edits). */
export interface TeamPermissions {
  invite: PermissionLevel;
  /** Calling the crew and making plans. */
  call: PermissionLevel;
  /** `moderators` or `leader`: removing plain members. */
  kick: PermissionLevel;
  /** `moderators` or `leader`: the emblem, colour, frame and motto. */
  edit: PermissionLevel;
  /** `moderators` or `leader`: answering join requests and the invite link (TEAM5; absent before
   * it, read as `moderators`). */
  admit?: PermissionLevel;
}

/** What the service offers of crews beyond the first version. */
export interface CrewFeatures {
  /** Colours, frames, mottos and the newer emblems. */
  look: boolean;
  /** "Ekibi çağır". */
  calls: boolean;
  /** Moderators and permissions. */
  roles: boolean;
  /** "Sessize al" and crew calls on or off, kept by the service (the same on every device). */
  mute: boolean;
  /** Several crews per player: their order, a mute per crew, one call to several. */
  multi: boolean;
  /** Calls start only on a multiplayer server and carry it (CALL1). */
  callServer: boolean;
  /** The join setting ("Onaylı"), join requests and who answers them (TEAM5). */
  requests: boolean;
  /** Crew invite links and their page (TEAM5). */
  link: boolean;
  /** Crew plans ("Planlar", TEAM5). */
  plans: boolean;
  /** A plan's short note, changed on its own (FIX2). */
  planNotes: boolean;
  /** "Arkadaş ekle" on a crewmate, without their code (FIX2). */
  crewFriendRequests: boolean;
  /** Crew tags before names, their cards and hidden crews (TEAM6). */
  badges: boolean;
  /** Join requests from a tag's card and the crews' "Etiketten istek" (TEAM6; only with
   * `badges`). */
  badgeRequests: boolean;
}

/** How newcomers with the crew's code or invite link get in: at once, or once approved. */
export type JoinMode = "open" | "approval";

/** What asking to join led to: in the crew now, or a request that waits for its leaders. */
export type JoinResult = "joined" | "requested";

/** Someone who asked to join one of the player's crews. */
export interface JoinRequest extends SocialProfile {
  /** Unix seconds. */
  sentAt: number;
  /** From a tag's card (TEAM6): the member whose tag it was, while they are still in the crew, and
   * their name as the crew lists them ("Ayşe aracılığıyla"). */
  via?: string;
  viaName?: string;
}

/** One of the player's own join requests that waits (it goes by itself after 7 days). */
export interface TeamRequest extends CrewLook {
  teamId: string;
  name: string;
  tag: string;
  motto: string;
  members: number;
  /** Unix seconds. */
  sentAt: number;
}

/** "Geliyorum", "Belki", "Gelemem" to a plan. */
export type PlanAnswer = "yes" | "maybe" | "no";

/** "Planlar": a time to meet on Projects in one of the player's crews, with an optional note. */
export interface CrewPlan {
  id: string;
  /** The crew's id. */
  crew: string;
  /** Who made it, and their name as the crew shows it. */
  by: string;
  byName?: string;
  mine: boolean;
  /** "Planı sil": the player made it, or leads or moderates the crew. */
  canCancel: boolean;
  /** The plan's time (Unix seconds, a whole quarter of an hour): the time to show. */
  at: number;
  /** The same moment and the plan's end on this computer's clock (reminders, hiding it). */
  startsAt: number;
  endsAt: number;
  /** When it was made, on this computer's clock (Unix seconds). */
  createdAt: number;
  /** Who answered what, in the crew's order. */
  answers: Array<{ id: string; answer: PlanAnswer }>;
  myAnswer?: PlanAnswer;
  /** One line of at most 80 characters ("Bedwars, sesli gelin"; FIX2). */
  note?: string;
  /** The player may write or change the note (they made the plan; none from older services). */
  canWriteNote?: boolean;
  /** The player may clear the note (they made the plan, or lead or moderate the crew). */
  canClearNote?: boolean;
}

/** The player's own mute of one crew (several crews); nobody else sees it. */
export interface CrewMute {
  muted: boolean;
  /** Unix seconds; null: until the player unmutes. A timed mute ends by itself. */
  muteUntil: number | null;
}

/** Whether "Ekibi çağır" may start where the player is (CALL1): on projects.gg, or on another
 * server whose address is shared; not in the launcher, singleplayer or an unknown server. */
export type CallPlace = "ok" | "needsServer" | "serverHidden";

/** What the player hears of their crew, kept by the service (all their devices). */
export interface CrewNotifications {
  /** "Ekip çağrıları". */
  calls: boolean;
  /** "Sessizde": until `muteUntil`, or until the player unmutes (`muteUntil` null). */
  muted: boolean;
  /** Unix seconds. A timed mute ends by itself (nothing says so): compare it with the clock. */
  muteUntil: number | null;
}

/** "Sessize al" (`muted: true`, until `muteUntil` or until unmuted without it), "Sesi aç"
 * (`muted: false`), crew calls on or off. */
export interface CrewNotificationsChange {
  calls?: boolean;
  muted?: boolean;
  /** Unix seconds, in the next 30 days; only with `muted: true`. */
  muteUntil?: number;
}

export interface TeamMember extends SocialProfile {
  role: TeamRole;
  /** A moderator (never the leader). */
  moderator: boolean;
  /** The player themself. */
  me: boolean;
  /** Also on the friend list. */
  friend: boolean;
  /** The player blocked them: they see each other offline. */
  blocked: boolean;
  state: PresenceState;
  dnd: boolean;
  /** Never another server's address for teammates. */
  game?: SocialGame;
  discordUrl: string;
  /** Unix seconds. */
  joinedAt: number;
  /** Seconds on Projects together this month, as this computer kept it (0 while it is off). */
  togetherMonth?: number;
  /** The crew tag they wear (TEAM6), of this crew or another; none while they wear none. */
  badge?: CrewBadge;
}

/** How a crew looks: its emblem on a coloured frame. */
export interface CrewLook {
  /** `sword`, `shield`, … (team-emblem.tsx draws it). */
  emblem: string;
  /** A key of the crew palette (`gold` before looks existed). */
  color: string;
  /** `cut`, `shield`, `banner` or `hex` (`cut` before looks existed). */
  frame: string;
}

export interface SocialTeam extends CrewLook {
  id: string;
  name: string;
  /** 2-4 letters or digits, upper case. */
  tag: string;
  /** Empty when there is none. */
  motto: string;
  /** `EKP-XXXX-XXXX` */
  code: string;
  /** `YYYY-MM-DD` */
  createdOn: string;
  /** The player's own role. */
  role: TeamRole;
  /** The player is a moderator. */
  moderator: boolean;
  permissions: TeamPermissions;
  /** Founder first, then by joining time; the player is among them. */
  members: TeamMember[];
  /** Friends the player invited who have not answered yet. */
  invited: string[];
  /** How many may be in a team. */
  limit: number;
  /** The player's own mute of this crew (never muted on a service with one crew per player). */
  mute: CrewMute;
  /** Moderators reset the crew's name, tag and motto; they stay locked until then (Unix seconds). */
  textLockedUntil?: number;
  /** How newcomers with the code or the invite link get in (`open` before TEAM5). */
  join?: JoinMode;
  /** The crew's invite link (`XXXX-XXXX-XXXX`) and its page, when it has one. */
  link?: string;
  linkUrl?: string;
  /** Players who asked to join, oldest first: only for a player whose role may answer them. */
  requests?: JoinRequest[];
  /** Seconds on Projects this week with someone of this crew there too (this computer only). */
  togetherWeek?: number;
  /** Its leader hid it (TEAM6): its tag shows nowhere and cannot be worn. */
  hidden?: boolean;
  /** "Etiketten istek" (TEAM6): friends of its wearers may ask to join from the tag's card (on
   * unless its leader turned it off). */
  badgeRequests?: boolean;
  /** How many of its members wear its tag (TEAM6: "Etiketi takan 8 üyenin…"). */
  wearers?: number;
}

/** Why a join request from a tag's card did not go (TEAM6): the crew's refusals and the player's
 * own limits, as answers (never errors). */
export type BadgeRefusal =
  | "closed"
  | "today"
  | "waitingFull"
  | "full"
  | "blocked"
  | "ownWaiting"
  | "ownCrews"
  | "ownDaily"
  | "member";

/** What "Katılma isteği gönder" on a tag's card led to (TEAM6): the request waits, or why not and
 * until when (Unix seconds: the end of the Turkey day) the button stays away on this computer. */
export type BadgeAsk =
  | { status: "sent" }
  | { status: "refused"; reason: BadgeRefusal; until: number };

/** Someone who came into one of the player's crews while this launcher was connected (TEAM6 "Hoş
 * geldin"): a quiet line in its chat; nothing keeps it. */
export interface CrewJoin {
  crew: string;
  /** Who came in. */
  id: string;
  /** Their name as the crew listed them. */
  name: string;
  /** The player themself ("Ekibe katıldın"). */
  mine: boolean;
  /** Unix seconds, this computer's clock. */
  at: number;
  /** "Selam ver" may be pressed until then (Unix seconds); none on the player's own and once it
   * was pressed. */
  greetUntil?: number;
}

export interface TeamInvite extends CrewLook {
  teamId: string;
  name: string;
  tag: string;
  motto: string;
  members: number;
  from: SocialProfile;
  /** Unix seconds. */
  sentAt: number;
}

export interface TeamLookup extends CrewLook {
  teamId: string;
  name: string;
  tag: string;
  motto: string;
  members: number;
  relation: TeamRelation;
  /** `EKP-XXXX-XXXX` (empty when it was looked up by its invite link). */
  code: string;
  /** The invite link it was looked up by (`XXXX-XXXX-XXXX`). */
  link?: string;
  /** `approval`: "Katıl" sends a join request (absent: `open`). */
  join?: JoinMode;
  /** The player's request to it waits since then (Unix seconds). */
  requestedAt?: number;
}

/** "Geliyorum", "Birazdan", "Gelemem". */
export type CallAnswer = "yes" | "soon" | "no";

/** "Ekibi çağır": an open crew call (the player's own too). */
export interface CrewCall {
  id: string;
  /** The caller's id. */
  from: string;
  mine: boolean;
  /** The caller's name and picture (from the crew list). */
  name?: string;
  avatar?: string;
  avatarUrl?: string;
  /** The caller's mark (SOC-VERIFY), from the crew list. */
  mark?: ProfileMark;
  /** `join`: "Geliyorum" can join the caller on `server`; `ready`: "Hazır mısın?" (a call from
   * before CALL1: an answer, nothing to join). */
  kind: "join" | "ready";
  /** Where the call is: projects.gg, or another server with its address. */
  server?: SocialGame;
  /** The player's crews it names, in the player's order. */
  crews: string[];
  /** Unix seconds on this computer's clock. */
  startedAt: number;
  /** It ends by itself then: hide it. */
  endsAt: number;
  /** Who answered what, in the crew's order. */
  answers: Array<{ id: string; answer: CallAnswer }>;
  myAnswer?: CallAnswer;
}

/** What "Ekibi çağır" led to. */
export type CallOutcome =
  | { status: "called" }
  | { status: "wait"; seconds: number }
  | { status: "active" }
  /** Not on a multiplayer server (CALL1). */
  | { status: "needsServer" }
  /** On another server whose address is not shared (CALL1). */
  | { status: "serverHidden" };

/** A change to the crew: its texts, look or permissions. */
export interface TeamChange {
  name?: string;
  tag?: string;
  emblem?: string;
  color?: string;
  frame?: string;
  motto?: string;
  permissions?: Partial<TeamPermissions>;
}

export interface SocialSnapshot {
  /** Grows with every snapshot; older ones are ignored. */
  seq: number;
  /** False: the feature is off and nothing of it is shown. */
  enabled: boolean;
  auth: AuthPhase;
  connection: SocialConnection;
  me?: SocialMe;
  /** The friend list arrived (skeletons until then). */
  loaded: boolean;
  friends: Friend[];
  incoming: FriendRequest[];
  outgoing: FriendRequest[];
  blocked: SocialProfile[];
  invites: SocialInvite[];
  activity: SocialActivity;
  /** The player's friend categories; undefined while the service does not offer them. */
  groups?: FriendGroups;
  /** Teams are on (signed config) and the service offers them. */
  teams: boolean;
  /** What the service offers of crews (all off while `teams` is off or on an older service). */
  crew: CrewFeatures;
  /** The player's crews in their order, the first being the primary crew (only while `teams`;
   * at most one on a service without several crews). */
  crews: SocialTeam[];
  /** How many crews the player may be in (1 on a service without several crews). */
  crewLimit: number;
  /** Invites to teams, newest first (only while `teams`). */
  teamInvites: TeamInvite[];
  /** The player's own join requests that wait, newest first (TEAM5). */
  teamRequests: TeamRequest[];
  /** The plans of the player's crews, soonest first, those under way included (TEAM5). */
  plans: CrewPlan[];
  /** "Birlikte oynanan süre" is kept on this computer. */
  together: boolean;
  /** Open crew calls, oldest first (only while the service offers calls). */
  calls: CrewCall[];
  /** Whether "Ekibi çağır" may start where the player is (CALL1). */
  callPlace: CallPlace;
  /** The player's crew notification settings (only while the service keeps them: `crew.mute`;
   * otherwise the launcher's own switches decide). */
  crewNotifications?: CrewNotifications;
  /** Chat may show (signed config): the sign-in screen says what chat keeps. */
  chatAllowed: boolean;
  /** Crew chat and direct messages, while the service offers them on this connection. */
  chat?: ChatState;
  /** One-time notes from moderators still to be shown, oldest first. */
  notices: ModNotice[];
  /** Suspended (`until`, Unix seconds) or banned (no `until`) in this run. */
  restriction?: { until?: number };
  /** The settings backup waits until then (Unix seconds): the service protects its storage. */
  backupWaitsUntil?: number;
  /** Who came into the player's crews while this launcher was connected (TEAM6 "Hoş geldin"). */
  crewJoins?: CrewJoin[];
}

export type SocialNotice =
  | { kind: "invite"; invite: SocialInvite }
  | { kind: "request"; request: FriendRequest }
  | { kind: "friendAdded"; friend: Friend }
  | { kind: "friendOnline"; id: string }
  | { kind: "friendOnProjects"; id: string }
  | { kind: "signedIn"; restore?: BackupMeta }
  | { kind: "signInFailed"; reason: SignInFailure }
  | { kind: "signedOutRemotely" }
  | { kind: "teamInvite"; invite: TeamInvite }
  /** `staff`: Projects' moderators did it (community rules). */
  | { kind: "teamGone"; reason: TeamExit; name?: string; staff: boolean }
  /** A teammate called the crew, live. */
  | { kind: "crewCall"; call: CrewCall }
  /** The caller of a call the player is coming to is on projects.gg now. */
  | { kind: "crewCallJoin"; call: CrewCall }
  /** A crewmate started playing on projects.gg. */
  | { kind: "crewmateOnProjects"; id: string }
  /** Someone asked to join one of the player's crews, live (TEAM5). */
  | { kind: "joinRequest"; crew: string; request: JoinRequest }
  /** A join request of the player's was taken: they are in that crew now. */
  | { kind: "requestAccepted"; crew: string; name: string }
  /** A crewmate made a plan, live. */
  | { kind: "crewPlan"; plan: CrewPlan }
  /** A new chat message from someone else, live (never a sync's). */
  | { kind: "chatMessage"; conv: string; message: ChatMessage; name?: string; crew?: string }
  /** Moderators suspended (`until`) or banned the account: every session ended. */
  | { kind: "restricted"; until?: number }
  /** Cloud AFK's notice about one of the player's cloud accounts (AFKPLUS; worded by the launcher). */
  | { kind: "afkNotice"; notice: AfkNotice };

/**
 * One Cloud AFK notice (services/social README "AFK notices"): `event` (`approval`, `deal_done`…),
 * `data` as text (account id `acct`, the bot's own `name`, item ids, amounts, times; never a
 * counterpart's name or a message), `quiet` in the player's quiet hours (no system notification),
 * `tag` shared by notices that replace each other, the item's names when the launcher knew them.
 */
export interface AfkNotice {
  id: string;
  event: string;
  kind: string;
  priority: string;
  at: number;
  tag?: string;
  quiet: boolean;
  data: Partial<Record<string, string>>;
  itemName?: { tr: string; en: string };
}

export interface CodeLookup {
  profile: SocialProfile;
  relation: Relation;
  /** Normalized `PRJ-XXXX-XXXX`. */
  code: string;
}

export interface SocialSession {
  id: string;
  device: string;
  /** `YYYY-MM-DD`. */
  createdOn: string;
  lastUsedOn: string;
  /** This computer. */
  current: boolean;
}

export const HIDDEN_SNAPSHOT: SocialSnapshot = {
  seq: -1,
  enabled: false,
  auth: "signedOut",
  connection: "idle",
  loaded: false,
  friends: [],
  incoming: [],
  outgoing: [],
  blocked: [],
  invites: [],
  activity: { state: "launcher" },
  teams: false,
  crew: {
    look: false,
    calls: false,
    roles: false,
    mute: false,
    multi: false,
    callServer: false,
    requests: false,
    link: false,
    plans: false,
    planNotes: false,
    crewFriendRequests: false,
    badges: false,
    badgeRequests: false,
  },
  crews: [],
  crewLimit: 1,
  teamInvites: [],
  teamRequests: [],
  plans: [],
  together: false,
  calls: [],
  callPlace: "needsServer",
  chatAllowed: false,
  notices: [],
};

export interface SocialApi {
  state(): Promise<SocialSnapshot>;
  /** Every change, as a full snapshot. Returns an unsubscribe function. */
  onView(listener: (view: SocialSnapshot) => void): () => void;
  /** Moments worth a toast or a notification. Returns an unsubscribe function. */
  onNotice(listener: (notice: SocialNotice) => void): () => void;
  /** Opens Discord in the browser; returns that page (to open it again). */
  signIn(): Promise<string>;
  cancelSignIn(): Promise<void>;
  signOut(): Promise<void>;
  /** "Tekrar dene" (throttled by the backend). */
  refresh(): Promise<void>;
  lookup(code: string): Promise<CodeLookup>;
  sendRequest(code: string): Promise<"requested" | "friends">;
  /** "Arkadaş ekle" on a crewmate who is not a friend, without their code (FIX2). */
  requestCrewmate(userId: string): Promise<"requested" | "friends">;
  accept(id: string): Promise<void>;
  decline(id: string): Promise<void>;
  cancelRequest(id: string): Promise<void>;
  removeFriend(id: string): Promise<void>;
  block(id: string): Promise<void>;
  unblock(id: string): Promise<void>;
  rotateCode(): Promise<string>;
  setStatus(status: StatusMode): Promise<void>;
  setPrivacy(change: {
    shareServer?: boolean;
    shareLastSeen?: boolean;
    shareRead?: boolean;
    shareTyping?: boolean;
  }): Promise<void>;
  setAvatar(avatar: string): Promise<void>;
  invite(id: string): Promise<"delivered" | "queued">;
  dismissInvite(id: string): Promise<void>;
  /**
   * "Discord'dan mesaj at": the person's profile in the Discord app when it is installed. False
   * when it is not (or would not open): the caller opens their `discordUrl` in the browser.
   */
  openInDiscord(id: string): Promise<boolean>;
  sessions(): Promise<SocialSession[]>;
  revokeSession(id: string): Promise<void>;
  revokeOtherSessions(): Promise<number>;
  deleteAccount(): Promise<void>;
  /** Launcher preferences only (an allow-list; never secrets). */
  uploadBackup(data: Record<string, unknown>): Promise<BackupMeta>;
  downloadBackup(): Promise<Record<string, unknown> | null>;
  deleteBackup(): Promise<void>;
  /** Saves the friend categories (the whole document; shown at once, saved in the background). */
  saveGroups(groups: FriendGroup[], members: Record<string, string>): Promise<void>;
  /** A crew (one more below the crew limit, on a service with several crews); the look (colour,
   * frame, motto) only reaches a service that offers looks. */
  createTeam(team: {
    name: string;
    tag: string;
    emblem: string;
    color?: string;
    frame?: string;
    motto?: string;
  }): Promise<void>;
  /** One of the player's crews: unset fields stay; `motto: ""` takes the motto away. */
  editTeam(teamId: string, change: TeamChange): Promise<void>;
  /** Makes a member a moderator, or a moderator a member again (leader). */
  setModerator(teamId: string, memberId: string, moderator: boolean): Promise<void>;
  /** "Ekibi çağır" for these crews (empty: the primary one); one call however many. */
  callCrew(crews: string[]): Promise<CallOutcome>;
  /** An answer to a teammate's call; false when the call is gone. */
  answerCall(callId: string, answer: CallAnswer): Promise<boolean>;
  /** "Bitir": ends the player's own call. */
  endCall(callId: string): Promise<void>;
  /** "Sessize al", "Sesi aç", crew calls on or off, for all crews (a service that keeps them). */
  setCrewNotifications(change: CrewNotificationsChange): Promise<void>;
  /** "Sessize al" / "Sesi aç" for one crew (several crews). `muteUntil` only with `muted`. */
  setCrewMute(teamId: string, change: { muted: boolean; muteUntil?: number }): Promise<void>;
  /** The player's crews in a new order (each once; the first becomes the primary crew). */
  orderCrews(order: string[]): Promise<void>;
  /** Ends one of the player's crews for everyone (founder). */
  disbandTeam(teamId: string): Promise<void>;
  leaveTeam(teamId: string): Promise<void>;
  /** A new code for one of the player's crews (founder); the old one stops working. */
  rotateTeamCode(teamId: string): Promise<string>;
  inviteToTeam(teamId: string, friendId: string): Promise<void>;
  /** Removes a member (founder); the code is renewed with it. */
  removeTeamMember(teamId: string, memberId: string): Promise<void>;
  /** Hands the founder role to another member. */
  transferTeam(teamId: string, memberId: string): Promise<void>;
  lookupTeam(code: string): Promise<TeamLookup>;
  /** Joins the crew of a code; a crew that approves newcomers takes a join request instead. */
  joinTeam(code: string): Promise<JoinResult>;
  acceptTeamInvite(teamId: string): Promise<void>;
  declineTeamInvite(teamId: string): Promise<void>;
  /** What a crew's invite link (`XXXX-XXXX-XXXX`) leads to (TEAM5). */
  lookupCrewLink(link: string): Promise<TeamLookup>;
  /** Joins the crew of an invite link, or asks to. */
  joinCrewLink(link: string): Promise<JoinResult>;
  /** Withdraws the player's waiting join request to a crew. */
  withdrawTeamRequest(teamId: string): Promise<void>;
  /** Takes a player who asked into one of the player's crews, or declines (silently). */
  answerTeamRequest(teamId: string, userId: string, accept: boolean): Promise<void>;
  /** A new invite link (`on`, in place of the old one) or none; answers the link it has now. */
  setTeamLink(teamId: string, on: boolean): Promise<string | null>;
  /** How newcomers with the code or the link get in (the leader). */
  setJoinMode(teamId: string, join: JoinMode): Promise<void>;
  /** Puts on the tag of one of the player's crews, in place of any worn before, or takes it off
   * (`null`; TEAM6). */
  setBadge(teamId: string | null): Promise<void>;
  /** "Görünürlük" (`hidden`: the tag shows nowhere, every worn one comes off) and "Etiketten istek"
   * of a crew the player leads (TEAM6). */
  setBadgeSettings(
    teamId: string,
    change: { hidden?: boolean; badgeRequests?: boolean },
  ): Promise<void>;
  /** "Katılma isteği gönder" on a tag's card (TEAM6): a join request that always waits for the
   * crew's answer; `via` is the friend whose tag it is. A refusal is an answer, not an error. */
  requestFromBadge(teamId: string, via: string): Promise<BadgeAsk>;
  /** "Selam ver" on the line of `id`, who came into `crew` (TEAM6): the "Selam" sticker (or
   * "Hoş geldin! 👋" without stickers) goes to the crew's chat as any message; once. */
  greetNewcomer(crew: string, id: string): Promise<void>;
  /** The crew tag the player wore came off because its leader hid the crew (TEAM6; never for the
   * leader's own change). Returns an unsubscribe function. */
  onTagOff(listener: (off: { crew: string; name: string }) => void): () => void;
  /** A new plan at `at` (Unix seconds, a whole quarter of an hour), with its note when the
   * service keeps notes; answers its id. */
  makePlan(crew: string, at: number, note?: string): Promise<string>;
  /** A plan's note: written or changed by its maker; `""` clears it (maker, leader, moderators). */
  setPlanNote(planId: string, note: string): Promise<void>;
  /** An answer to a plan; false when the plan is gone. */
  answerPlan(planId: string, answer: PlanAnswer): Promise<boolean>;
  /** "Planı sil". */
  cancelPlan(planId: string): Promise<void>;
  /** "Birlikte oynanan süre" on or off on this computer (off deletes what was kept). */
  setTogether(on: boolean): Promise<void>;
  /** Deletes the time played together kept on this computer. */
  clearTogether(): Promise<void>;
  /** Changes in chat conversations. Returns an unsubscribe function. */
  onChat(listener: (change: ChatChange) => void): () => void;
  /** A conversation's newest messages, or those before a message id. */
  chatPage(conv: string, before?: number): Promise<ChatPage>;
  /** Sends a message (CHAT2: as a reply to message `reply`): it shows at once (on its way).
   * Returns its key. */
  chatSend(conv: string, text: string, reply?: number): Promise<string>;
  /** CHAT2: sends a sticker ("pl1/gg"), maybe as a reply; it shows at once. Returns its key. */
  chatSendSticker(conv: string, sticker: string, reply?: number): Promise<string>;
  /** CHAT2: adds the player's reaction to a message (`on`), or takes it back. */
  chatReact(conv: string, id: number, emoji: string, on: boolean): Promise<void>;
  /** CHAT2: the player writes here (a key in a box with text and focus); the backend sends it at
   * most every 4 seconds, and only when someone in the conversation can see it. */
  chatTyping(conv: string): void;
  /** CHAT2: the card of a message's first link (once, when it comes on screen; the message comes
   * back with `card`). */
  chatUnfurl(conv: string, key: string): Promise<void>;
  /** CHAT2: someone writes in a conversation. Returns an unsubscribe function. */
  onChatTyping(listener: (typing: ChatTyping) => void): () => void;
  /** CHAT2: the card of a link the player is writing (the composer's preview); null when the
   * hub has nothing to show. */
  chatCard(url: string): Promise<ChatCard | null>;
  /** "Tekrar dene" on a message that did not go. */
  chatRetry(key: string): Promise<void>;
  /** Drops a message that did not go. */
  chatDiscard(key: string): Promise<void>;
  /** Edits the player's own message (15 minutes). */
  chatEdit(conv: string, id: number, text: string): Promise<void>;
  /** "Herkesten sil". */
  chatDelete(conv: string, id: number): Promise<void>;
  /** The conversation is on screen at its newest message. */
  chatRead(conv: string): Promise<void>;
  /** "Geçmişi temizle" (this computer only). */
  chatClear(conv: string): Promise<void>;
  /** "Rapor et". */
  report(
    target: ReportTarget,
    reason: ReportReason,
    note: string | undefined,
    block: boolean,
  ): Promise<void>;
  /** The launcher showed these moderators' notes. */
  noticesSeen(ids: string[]): Promise<void>;
}
