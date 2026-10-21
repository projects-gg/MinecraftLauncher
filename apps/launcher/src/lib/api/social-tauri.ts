// Tauri implementation of `api.social`: maps the generated bindings onto the UI shapes.
import { listen } from "@tauri-apps/api/event";
import {
  type ChatChangeView,
  type ChatMessageView,
  type ChatView,
  type CmdError,
  type ConversationView,
  type CrewCallView,
  commands,
  type FriendGroupsView,
  type FriendView,
  type InviteView,
  type MeView,
  type ModNoticeView,
  type SocialNotice as NoticeBinding,
  type ProfileView,
  type RequestView,
  type SocialView,
  type TeamInviteView,
  type TeamMemberView,
  type TeamView,
} from "@/bindings";
import type {
  ChatChange,
  ChatMessage,
  ChatState,
  Conversation,
  CrewCall,
  CrewFeatures,
  Friend,
  FriendGroups,
  FriendRequest,
  LimitedKey,
  ModNotice,
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
    limited: v.limited
      ? {
          until: v.limited.until,
          items: v.limited.items
            .filter((item) => item.key !== "other")
            .map((item) => ({ key: item.key as LimitedKey, perDay: opt(item.perDay) })),
        }
      : undefined,
  };
}

function chatMessage(v: ChatMessageView): ChatMessage {
  return {
    key: v.key,
    id: opt(v.id),
    from: v.from,
    mine: v.mine,
    text: v.text,
    at: v.at,
    edited: v.edited,
    state: v.state,
    failure: opt(v.failure),
    editUntil: opt(v.editUntil),
    deleteUntil: opt(v.deleteUntil),
  };
}

function conversation(v: ConversationView): Conversation {
  return {
    id: v.id,
    peer: opt(v.peer),
    crew: opt(v.crew),
    title: opt(v.title),
    last: opt(v.last),
    unread: v.unread,
    writable: v.writable,
    failed: v.failed,
  };
}

function chatState(v: ChatView): ChatState {
  return {
    on: v.on,
    mailbox: v.mailbox,
    perDay: v.perDay,
    mutedUntil: opt(v.mutedUntil),
    conversations: v.conversations.map(conversation),
  };
}

function chatChange(v: ChatChangeView): ChatChange {
  return {
    conv: v.conv,
    upsert: v.upsert.map(chatMessage),
    remove: v.remove,
    reset: v.reset,
  };
}

function modNotice(v: ModNoticeView): ModNotice | null {
  if (v.kind === "other") return null;
  return { id: v.id, kind: v.kind, at: v.at, reason: opt(v.reason) };
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
    moderator: v.moderator,
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
    color: v.color,
    frame: v.frame,
    motto: v.motto,
    code: v.code,
    createdOn: v.createdOn,
    role: v.role,
    moderator: v.moderator,
    // A service without roles sends none: everyone invites, only the leader removes and edits.
    permissions: {
      invite: v.permissions.invite ?? "members",
      call: v.permissions.call ?? "members",
      kick: v.permissions.kick ?? "leader",
      edit: v.permissions.edit ?? "leader",
    },
    members: v.members.map(teamMember),
    invited: v.invited,
    limit: v.limit,
    mute: { muted: v.notifications.muted ?? false, muteUntil: v.notifications.muteUntil ?? null },
    textLockedUntil: opt(v.textLockedUntil),
  };
}

function teamInvite(v: TeamInviteView): TeamInvite {
  return {
    teamId: v.teamId,
    name: v.name,
    tag: v.tag,
    emblem: v.emblem,
    color: v.color,
    frame: v.frame,
    motto: v.motto,
    members: v.members,
    from: profile(v.from),
    sentAt: v.sentAt,
  };
}

function call(v: CrewCallView): CrewCall {
  return {
    id: v.id,
    from: v.from,
    mine: v.mine,
    name: opt(v.name),
    avatar: opt(v.avatar),
    avatarUrl: opt(v.avatarUrl),
    kind: v.kind,
    server: game(v.server),
    crews: v.crews,
    startedAt: v.startedAt,
    endsAt: v.endsAt,
    answers: v.answers.map((a) => ({ id: a.id, answer: a.answer })),
    myAnswer: opt(v.myAnswer),
  };
}

/** `teamFeatures` as switches (values this launcher does not know mean nothing here). */
export function crewFeatures(list: readonly string[]): CrewFeatures {
  return {
    look: list.includes("look"),
    calls: list.includes("calls"),
    roles: list.includes("roles"),
    mute: list.includes("mute"),
    multi: list.includes("multi"),
    callServer: list.includes("callServer"),
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
    crew: crewFeatures(v.teamFeatures),
    crews: v.crews.map(team),
    crewLimit: v.crewLimit,
    teamInvites: v.teamInvites.map(teamInvite),
    calls: v.calls.map(call),
    callPlace: v.callPlace,
    crewNotifications: v.crewNotifications
      ? {
          calls: v.crewNotifications.calls,
          muted: v.crewNotifications.muted,
          muteUntil: v.crewNotifications.muteUntil ?? null,
        }
      : undefined,
    chatAllowed: v.chatAllowed,
    chat: v.chat ? chatState(v.chat) : undefined,
    notices: v.notices.map(modNotice).filter((n): n is ModNotice => n !== null),
    restriction: v.restriction ? { until: opt(v.restriction.until) } : undefined,
    backupWaitsUntil: opt(v.backupWaitsUntil),
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
      return { kind: "teamGone", reason: v.reason, name: opt(v.name), staff: v.staff };
    case "crewCall":
      return { kind: "crewCall", call: call(v.call) };
    case "crewCallJoin":
      return { kind: "crewCallJoin", call: call(v.call) };
    case "crewmateOnProjects":
      return { kind: "crewmateOnProjects", id: v.id };
    case "chatMessage":
      return {
        kind: "chatMessage",
        conv: v.conv,
        message: chatMessage(v.message),
        name: opt(v.name),
        crew: opt(v.crew),
      };
    case "restricted":
      return { kind: "restricted", until: opt(v.until) };
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
  openInDiscord: (id) => commands.socialOpenInDiscord(id),
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
  async createTeam({ name, tag, emblem, color, frame, motto }) {
    await ok(
      commands.socialTeamCreate(name, tag, emblem, color ?? null, frame ?? null, motto ?? null),
    );
  },
  async editTeam(teamId, change) {
    const permissions = change.permissions;
    await ok(
      commands.socialTeamEdit(
        teamId,
        change.name ?? null,
        change.tag ?? null,
        change.emblem ?? null,
        change.color ?? null,
        change.frame ?? null,
        change.motto ?? null,
        permissions
          ? {
              invite: permissions.invite ?? null,
              call: permissions.call ?? null,
              kick: permissions.kick ?? null,
              edit: permissions.edit ?? null,
            }
          : null,
      ),
    );
  },
  async setModerator(teamId, memberId, moderator) {
    await ok(commands.socialTeamModerator(teamId, memberId, moderator));
  },
  callCrew: (crews) => ok(commands.socialTeamCall(crews)),
  answerCall: (callId, answer) => ok(commands.socialTeamCallAnswer(callId, answer)),
  async endCall(callId) {
    await ok(commands.socialTeamCallEnd(callId));
  },
  async setCrewNotifications(change) {
    await ok(
      commands.socialTeamNotifications({
        calls: change.calls ?? null,
        muted: change.muted ?? null,
        muteUntil: change.muteUntil ?? null,
      }),
    );
  },
  async setCrewMute(teamId, change) {
    await ok(
      commands.socialTeamMute(teamId, {
        muted: change.muted,
        muteUntil: change.muteUntil ?? null,
      }),
    );
  },
  async orderCrews(order) {
    await ok(commands.socialTeamOrder(order));
  },
  async disbandTeam(teamId) {
    await ok(commands.socialTeamDisband(teamId));
  },
  async leaveTeam(teamId) {
    await ok(commands.socialTeamLeave(teamId));
  },
  rotateTeamCode: (teamId) => ok(commands.socialTeamRotateCode(teamId)),
  async inviteToTeam(teamId, friendId) {
    await ok(commands.socialTeamInvite(teamId, friendId));
  },
  async removeTeamMember(teamId, memberId) {
    await ok(commands.socialTeamRemove(teamId, memberId));
  },
  async transferTeam(teamId, memberId) {
    await ok(commands.socialTeamTransfer(teamId, memberId));
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
  onChat(listener) {
    const unlisten = listen<ChatChangeView>("social-chat", (event) =>
      listener(chatChange(event.payload)),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  async chatPage(conv, before) {
    const page = await commands.socialChatPage(conv, before ?? null);
    return { conv: page.conv, messages: page.messages.map(chatMessage), more: page.more };
  },
  chatSend: (conv, text) => ok(commands.socialChatSend(conv, text)),
  async chatRetry(key) {
    await ok(commands.socialChatRetry(key));
  },
  async chatDiscard(key) {
    await commands.socialChatDiscard(key);
  },
  async chatEdit(conv, id, text) {
    await ok(commands.socialChatEdit(conv, id, text));
  },
  async chatDelete(conv, id) {
    await ok(commands.socialChatDelete(conv, id));
  },
  async chatRead(conv) {
    await commands.socialChatRead(conv);
  },
  async chatClear(conv) {
    await commands.socialChatClear(conv);
  },
  async report(target, reason, note, block) {
    const wire =
      target.kind === "crew"
        ? { kind: "crew" as const, id: target.id, code: target.code ?? null }
        : target;
    await ok(commands.socialReport(wire, reason, note ?? null, block));
  },
  async noticesSeen(ids) {
    await ok(commands.socialNoticesSeen(ids));
  },
};
