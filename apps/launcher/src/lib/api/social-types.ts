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

/** "founder" is the crew's leader ("Lider"); the wire keeps the first name. */
export type TeamRole = "founder" | "member";
/** How a team code relates to the player. */
export type TeamRelation = "none" | "member" | "elsewhere" | "blocked" | "full";
/** Why the player is no longer in their team. */
export type TeamExit = "left" | "kicked" | "disbanded" | "other";
/** Who may do something in a crew: the lowest role that may (every role above may too). */
export type PermissionLevel = "members" | "moderators" | "leader";

/** A crew's permissions (defaults from a service without roles: everyone invites, only the
 * leader removes and edits). */
export interface TeamPermissions {
  invite: PermissionLevel;
  call: PermissionLevel;
  /** `moderators` or `leader`: removing plain members. */
  kick: PermissionLevel;
  /** `moderators` or `leader`: the emblem, colour, frame and motto. */
  edit: PermissionLevel;
}

/** What the service offers of crews beyond the first version. */
export interface CrewFeatures {
  /** Colours, frames, mottos and the newer emblems. */
  look: boolean;
  /** "Ekibi çağır". */
  calls: boolean;
  /** Moderators and permissions. */
  roles: boolean;
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
  /** `EKP-XXXX-XXXX` */
  code: string;
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
  /** `join`: the caller plays on projects.gg; `ready`: "Hazır mısın?". */
  kind: "join" | "ready";
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
  | { status: "active" };

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
  /** The player's team (only while `teams`). */
  team?: SocialTeam;
  /** Invites to teams, newest first (only while `teams`). */
  teamInvites: TeamInvite[];
  /** Open crew calls, oldest first (only while the service offers calls). */
  calls: CrewCall[];
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
  | { kind: "teamGone"; reason: TeamExit; name?: string }
  /** A teammate called the crew, live. */
  | { kind: "crewCall"; call: CrewCall }
  /** The caller of a call the player is coming to is on projects.gg now. */
  | { kind: "crewCallJoin"; call: CrewCall }
  /** A crewmate started playing on projects.gg. */
  | { kind: "crewmateOnProjects"; id: string };

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
  crew: { look: false, calls: false, roles: false },
  teamInvites: [],
  calls: [],
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
  /** A crew; the look (colour, frame, motto) only reaches a service that offers looks. */
  createTeam(team: {
    name: string;
    tag: string;
    emblem: string;
    color?: string;
    frame?: string;
    motto?: string;
  }): Promise<void>;
  /** Unset fields stay; `motto: ""` takes the motto away. */
  editTeam(change: TeamChange): Promise<void>;
  /** Makes a member a moderator, or a moderator a member again (leader). */
  setModerator(memberId: string, moderator: boolean): Promise<void>;
  /** "Ekibi çağır". */
  callCrew(): Promise<CallOutcome>;
  /** An answer to a teammate's call; false when the call is gone. */
  answerCall(callId: string, answer: CallAnswer): Promise<boolean>;
  /** "Bitir": ends the player's own call. */
  endCall(callId: string): Promise<void>;
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
