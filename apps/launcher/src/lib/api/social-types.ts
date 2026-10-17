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
}

export type SocialNotice =
  | { kind: "invite"; invite: SocialInvite }
  | { kind: "request"; request: FriendRequest }
  | { kind: "friendAdded"; friend: Friend }
  | { kind: "friendOnline"; id: string }
  | { kind: "friendOnProjects"; id: string }
  | { kind: "signedIn"; restore?: BackupMeta }
  | { kind: "signInFailed"; reason: SignInFailure }
  | { kind: "signedOutRemotely" };

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
  sessions(): Promise<SocialSession[]>;
  revokeSession(id: string): Promise<void>;
  revokeOtherSessions(): Promise<number>;
  deleteAccount(): Promise<void>;
  /** Launcher preferences only (an allow-list; never secrets). */
  uploadBackup(data: Record<string, unknown>): Promise<BackupMeta>;
  downloadBackup(): Promise<Record<string, unknown> | null>;
  deleteBackup(): Promise<void>;
}
