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
export type SignInFailure = "expired" | "unavailable" | "failed";

export interface SocialGame {
  where: GameWhere;
  /** Another server's address, only when the friend shares it. */
  host?: string;
}

/** Someone shown with name and picture. */
export interface SocialProfile {
  id: string;
  name: string;
  username: string;
  /** `discord`, `preset:<slug>` or `minecraft:<texture hash>`. */
  avatar: string;
  /** Discord picture when `avatar` is `discord`. */
  avatarUrl?: string;
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
  privacy: { shareServer: boolean; shareLastSeen: boolean };
  backup?: BackupMeta;
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

export type TeamRole = "founder" | "member";
/** How a team code relates to the player. */
export type TeamRelation = "none" | "member" | "elsewhere" | "blocked" | "full";
/** Why the player is no longer in their team. */
export type TeamExit = "left" | "kicked" | "disbanded" | "other";

export interface TeamMember extends SocialProfile {
  role: TeamRole;
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
}

export interface SocialTeam {
  id: string;
  name: string;
  /** 2-4 letters or digits, upper case. */
  tag: string;
  /** `sword`, `shield`, … (team-emblem.tsx draws it). */
  emblem: string;
  /** `TKM-XXXX-XXXX` */
  code: string;
  /** `YYYY-MM-DD` */
  createdOn: string;
  /** The player's own role. */
  role: TeamRole;
  /** Founder first, then by joining time; the player is among them. */
  members: TeamMember[];
  /** Friends the player invited who have not answered yet. */
  invited: string[];
  /** How many may be in a team. */
  limit: number;
}

export interface TeamInvite {
  teamId: string;
  name: string;
  tag: string;
  emblem: string;
  members: number;
  from: SocialProfile;
  /** Unix seconds. */
  sentAt: number;
}

export interface TeamLookup {
  teamId: string;
  name: string;
  tag: string;
  emblem: string;
  members: number;
  relation: TeamRelation;
  /** `TKM-XXXX-XXXX` */
  code: string;
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
  /** The player's team (only while `teams`). */
  team?: SocialTeam;
  /** Invites to teams, newest first (only while `teams`). */
  teamInvites: TeamInvite[];
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
  | { kind: "teamGone"; reason: TeamExit; name?: string };

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
  teamInvites: [],
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
  accept(id: string): Promise<void>;
  decline(id: string): Promise<void>;
  cancelRequest(id: string): Promise<void>;
  removeFriend(id: string): Promise<void>;
  block(id: string): Promise<void>;
  unblock(id: string): Promise<void>;
  rotateCode(): Promise<string>;
  setStatus(status: StatusMode): Promise<void>;
  setPrivacy(change: { shareServer?: boolean; shareLastSeen?: boolean }): Promise<void>;
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
  createTeam(team: { name: string; tag: string; emblem: string }): Promise<void>;
  editTeam(change: { name?: string; tag?: string; emblem?: string }): Promise<void>;
  /** Ends the team for everyone (founder). */
  disbandTeam(): Promise<void>;
  leaveTeam(): Promise<void>;
  /** A new team code (founder); the old one stops working. */
  rotateTeamCode(): Promise<string>;
  inviteToTeam(friendId: string): Promise<void>;
  /** Removes a member (founder); the code is renewed with it. */
  removeTeamMember(memberId: string): Promise<void>;
  /** Hands the founder role to another member. */
  transferTeam(memberId: string): Promise<void>;
  lookupTeam(code: string): Promise<TeamLookup>;
  joinTeam(code: string): Promise<void>;
  acceptTeamInvite(teamId: string): Promise<void>;
  declineTeamInvite(teamId: string): Promise<void>;
}
