// Tauri implementation of `api.social`: maps the generated bindings onto the UI shapes.
import { listen } from "@tauri-apps/api/event";
import {
  type CmdError,
  commands,
  type FriendGroupsView,
  type FriendView,
  type InviteView,
  type MeView,
  type SocialNotice as NoticeBinding,
  type ProfileView,
  type RequestView,
  type SocialView,
  type TeamInviteView,
  type TeamMemberView,
  type TeamView,
} from "@/bindings";
import type {
  Friend,
  FriendGroups,
  FriendRequest,
  SocialApi,
  SocialGame,
  SocialInvite,
  SocialMe,
  SocialNotice,
  SocialProfile,
  SocialSnapshot,
  SocialTeam,
  TeamInvite,
  TeamMember,
} from "./social-types";

/** A failed friends command: friendly `message`, plus `code`/`details` for "copy details". */
export class SocialCommandError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(error: CmdError) {
    super(error.message);
    this.code = error.code;
    this.details = error.details;
  }
}

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new SocialCommandError(result.error);
  return result.data;
}

const opt = <T>(value: T | null | undefined): T | undefined => value ?? undefined;

function game(value: { where: SocialGame["where"]; host?: string | null } | null | undefined) {
  return value ? { where: value.where, host: opt(value.host) } : undefined;
}

function profile(v: ProfileView): SocialProfile {
  return {
    id: v.id,
    name: v.name,
    username: v.username,
    avatar: v.avatar,
    avatarUrl: opt(v.avatarUrl),
  };
}

function friend(v: FriendView): Friend {
  return {
    id: v.id,
    name: v.name,
    username: v.username,
    avatar: v.avatar,
    avatarUrl: opt(v.avatarUrl),
    since: v.since || undefined,
    lastSeen: opt(v.lastSeen),
    state: v.state,
    dnd: v.dnd,
    game: game(v.game),
    discordUrl: v.discordUrl,
  };
}

function request(v: RequestView): FriendRequest {
  return { ...profile(v), sentAt: v.sentAt };
}

function invite(v: InviteView): SocialInvite {
  return {
    id: v.id,
    from: v.from,
    name: opt(v.name),
    avatar: opt(v.avatar),
    avatarUrl: opt(v.avatarUrl),
    at: v.at,
    server: game(v.server),
    quiet: v.quiet,
  };
}

function me(v: MeView): SocialMe {
  return {
    id: v.id,
    name: v.name,
    username: v.username,
    avatar: v.avatar,
    avatarUrl: opt(v.avatarUrl),
    discordAvatarUrl: v.discordAvatarUrl,
    friendCode: v.friendCode,
    shareLink: opt(v.shareLink),
    qr: opt(v.qr),
    status: v.status,
    privacy: { shareServer: v.privacy.shareServer, shareLastSeen: v.privacy.shareLastSeen },
    backup: opt(v.backup),
  };
}

function groups(v: FriendGroupsView): FriendGroups {
  return {
    version: v.version,
    groups: v.groups,
    members: Object.fromEntries(v.members.map((m) => [m.friend, m.group])),
  };
}

function teamMember(v: TeamMemberView): TeamMember {
  return {
    id: v.id,
    name: v.name,
    username: v.username,
    avatar: v.avatar,
    avatarUrl: opt(v.avatarUrl),
    role: v.role,
    me: v.me,
    friend: v.friend,
    blocked: v.blocked,
    state: v.state,
    dnd: v.dnd,
    game: game(v.game),
    discordUrl: v.discordUrl,
    joinedAt: v.joinedAt,
  };
}

function team(v: TeamView): SocialTeam {
  return {
    id: v.id,
    name: v.name,
    tag: v.tag,
    emblem: v.emblem,
    code: v.code,
    createdOn: v.createdOn,
    role: v.role,
    members: v.members.map(teamMember),
    invited: v.invited,
    limit: v.limit,
  };
}

function teamInvite(v: TeamInviteView): TeamInvite {
  return {
    teamId: v.teamId,
    name: v.name,
    tag: v.tag,
    emblem: v.emblem,
    members: v.members,
    from: profile(v.from),
    sentAt: v.sentAt,
  };
}

export function snapshot(v: SocialView): SocialSnapshot {
  return {
    seq: v.seq,
    enabled: v.enabled,
    auth: v.auth,
    connection: v.connection,
    me: v.me ? me(v.me) : undefined,
    loaded: v.loaded,
    friends: v.friends.map(friend),
    incoming: v.incoming.map(request),
    outgoing: v.outgoing.map(request),
    blocked: v.blocked.map(profile),
    invites: v.invites.map(invite),
    activity:
      v.activity.state === "playing"
        ? { state: "playing", host: opt(v.activity.host) }
        : { state: v.activity.state },
    groups: v.groups ? groups(v.groups) : undefined,
    teams: v.teams,
    team: v.team ? team(v.team) : undefined,
    teamInvites: v.teamInvites.map(teamInvite),
  };
}

function notice(v: NoticeBinding): SocialNotice {
  switch (v.kind) {
    case "invite":
      return { kind: "invite", invite: invite(v.invite) };
    case "request":
      return { kind: "request", request: request(v.request) };
    case "friendAdded":
      return { kind: "friendAdded", friend: friend(v.friend) };
    case "friendOnline":
      return { kind: "friendOnline", id: v.id };
    case "friendOnProjects":
      return { kind: "friendOnProjects", id: v.id };
    case "signedIn":
      return { kind: "signedIn", restore: opt(v.restore) };
    case "signInFailed":
      return { kind: "signInFailed", reason: v.reason };
    case "signedOutRemotely":
      return { kind: "signedOutRemotely" };
    case "teamInvite":
      return { kind: "teamInvite", invite: teamInvite(v.invite) };
    case "teamGone":
      return { kind: "teamGone", reason: v.reason, name: opt(v.name) };
  }
}

export const tauriSocial: SocialApi = {
  async state() {
    return snapshot(await commands.socialState());
  },
  onView(listener) {
    const unlisten = listen<SocialView>("social-view", (event) =>
      listener(snapshot(event.payload)),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  onNotice(listener) {
    const unlisten = listen<NoticeBinding>("social-notice", (event) =>
      listener(notice(event.payload)),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  signIn: () => ok(commands.socialSignIn()),
  async cancelSignIn() {
    await commands.socialSignInCancel();
  },
  async signOut() {
    await ok(commands.socialSignOut());
  },
  async refresh() {
    await ok(commands.socialRefresh());
  },
  async lookup(code) {
    const found = await ok(commands.socialLookup(code));
    return { profile: profile(found.profile), relation: found.relation, code: found.code };
  },
  sendRequest: (code) => ok(commands.socialSendRequest(code)),
  async accept(id) {
    await ok(commands.socialAccept(id));
  },
  async decline(id) {
    await ok(commands.socialDecline(id));
  },
  async cancelRequest(id) {
    await ok(commands.socialCancelRequest(id));
  },
  async removeFriend(id) {
    await ok(commands.socialRemoveFriend(id));
  },
  async block(id) {
    await ok(commands.socialBlock(id));
  },
  async unblock(id) {
    await ok(commands.socialUnblock(id));
  },
  rotateCode: () => ok(commands.socialRotateCode()),
  async setStatus(status) {
    await ok(commands.socialSetStatus(status));
  },
  async setPrivacy(change) {
    await ok(commands.socialSetPrivacy(change.shareServer ?? null, change.shareLastSeen ?? null));
  },
  async setAvatar(avatar) {
    await ok(commands.socialSetAvatar(avatar));
  },
  invite: (id) => ok(commands.socialInvite(id)),
  async dismissInvite(id) {
    await commands.socialDismissInvite(id);
  },
  sessions: () => ok(commands.socialSessions()),
  async revokeSession(id) {
    await ok(commands.socialRevokeSession(id));
  },
  revokeOtherSessions: () => ok(commands.socialRevokeOtherSessions()),
  async deleteAccount() {
    await ok(commands.socialDeleteAccount());
  },
  uploadBackup: (data) => ok(commands.socialBackupUpload(JSON.stringify(data))),
  async downloadBackup() {
    const text = await ok(commands.socialBackupDownload());
    if (!text) return null;
    const value: unknown = JSON.parse(text);
    return value && typeof value === "object" && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : null;
  },
  async deleteBackup() {
    await ok(commands.socialBackupDelete());
  },
  async saveGroups(list, members) {
    await ok(
      commands.socialGroupsSave(
        list,
        Object.entries(members).map(([friend, group]) => ({ friend, group })),
      ),
    );
  },
  async createTeam({ name, tag, emblem }) {
    await ok(commands.socialTeamCreate(name, tag, emblem));
  },
  async editTeam(change) {
    await ok(
      commands.socialTeamEdit(change.name ?? null, change.tag ?? null, change.emblem ?? null),
    );
  },
  async disbandTeam() {
    await ok(commands.socialTeamDisband());
  },
  async leaveTeam() {
    await ok(commands.socialTeamLeave());
  },
  rotateTeamCode: () => ok(commands.socialTeamRotateCode()),
  async inviteToTeam(friendId) {
    await ok(commands.socialTeamInvite(friendId));
  },
  async removeTeamMember(memberId) {
    await ok(commands.socialTeamRemove(memberId));
  },
  async transferTeam(memberId) {
    await ok(commands.socialTeamTransfer(memberId));
  },
  lookupTeam: (code) => ok(commands.socialTeamLookup(code)),
  async joinTeam(code) {
    await ok(commands.socialTeamJoin(code));
  },
  async acceptTeamInvite(teamId) {
    await ok(commands.socialTeamAccept(teamId));
  },
  async declineTeamInvite(teamId) {
    await ok(commands.socialTeamDecline(teamId));
  },
};
