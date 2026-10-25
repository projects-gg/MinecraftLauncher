// Tauri implementation of `api.social`: maps the generated bindings onto the UI shapes.
import { listen } from "@tauri-apps/api/event";
import {
  type AfkNotice_Serialize,
  type ChatCardView,
  type ChatChangeView,
  type ChatMessageView,
  type ChatTypingView,
  type ChatView,
  type CmdError,
  type ConversationView,
  type CrewBadgeView,
  type CrewCallView,
  type CrewJoinView,
  commands,
  type FriendGroupsView,
  type FriendView,
  type InviteView,
  type JoinRequestView,
  type MeView,
  type ModNoticeView,
  type SocialNotice as NoticeBinding,
  type PlanView,
  type ProfileView,
  type RequestView,
  type SocialView,
  type TagOffView,
  type TeamInviteView,
  type TeamLookupView,
  type TeamMemberView,
  type TeamRequestView,
  type TeamView,
} from "@/bindings";
import type {
  ChatCard,
  ChatChange,
  ChatMessage,
  ChatState,
  Conversation,
  CrewBadge,
  CrewCall,
  CrewFeatures,
  CrewJoin,
  CrewPlan,
  Friend,
  FriendGroups,
  FriendRequest,
  JoinRequest,
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
  TeamLookup,
  TeamMember,
  TeamRequest,
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

/** A crew tag's card (TEAM6): only while the service lists `badges` (else none at all). */
function badge(v: CrewBadgeView | null | undefined): CrewBadge | undefined {
  return v
    ? {
        crew: v.crew,
        tag: v.tag,
        color: v.color,
        frame: v.frame,
        emblem: v.emblem,
        name: v.name,
        motto: v.motto,
        createdOn: v.createdOn,
        members: v.members,
        requests: v.requests,
      }
    : undefined;
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
    badge: badge(v.badge),
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
    privacy: {
      shareServer: v.privacy.shareServer,
      shareLastSeen: v.privacy.shareLastSeen,
      // CHAT2: none from a service without the chat extras (the switches do not show).
      shareRead: opt(v.privacy.shareRead),
      shareTyping: opt(v.privacy.shareTyping),
    },
    backup: opt(v.backup),
    limited: v.limited
      ? {
          until: v.limited.until,
          items: v.limited.items
            .filter((item) => item.key !== "other")
            .map((item) => ({ key: item.key as LimitedKey, perDay: opt(item.perDay) })),
        }
      : undefined,
    badge: badge(v.badge),
  };
}

function chatCard(v: ChatCardView): ChatCard {
  return {
    state: v.state,
    url: v.url,
    site: v.site,
    icon: v.icon,
    kind: v.kind,
    title: opt(v.title),
    author: opt(v.author),
    desc: opt(v.desc),
    image: opt(v.image),
    w: opt(v.w),
    h: opt(v.h),
    rule: opt(v.rule),
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
    deleted: v.deleted ? { by: v.deleted.by, at: opt(v.deleted.at) } : undefined,
    // CHAT2.
    failureRule: opt(v.failureRule),
    sticker: opt(v.sticker),
    reply: v.reply
      ? {
          id: v.reply.id,
          from: v.reply.from,
          key: opt(v.reply.key),
          text: opt(v.reply.text),
          sticker: opt(v.reply.sticker),
          deleted: v.reply.deleted,
          missing: v.reply.missing,
        }
      : undefined,
    reactions: v.reactions.length ? v.reactions : undefined,
    card: v.card ? chatCard(v.card) : undefined,
  };
}

function conversation(v: ConversationView): Conversation {
  return {
    id: v.id,
    peer: opt(v.peer),
    crew: opt(v.crew),
    title: opt(v.title),
    last: v.last
      ? {
          from: v.last.from,
          mine: v.last.mine,
          text: v.last.text,
          at: v.last.at,
          deletedBy: opt(v.last.deletedBy),
          sticker: opt(v.last.sticker),
        }
      : undefined,
    unread: v.unread,
    writable: v.writable,
    failed: v.failed,
    peerRead: opt(v.peerRead),
  };
}

function chatState(v: ChatView): ChatState {
  return {
    on: v.on,
    mailbox: v.mailbox,
    perDay: v.perDay,
    mutedUntil: opt(v.mutedUntil),
    conversations: v.conversations.map(conversation),
    // CHAT2: what the socket took and Studio has on; none from an older service.
    extras: opt(v.extras),
    stickerSets: v.stickerSets,
    links: opt(v.links),
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
  return {
    id: v.id,
    kind: v.kind,
    at: v.at,
    reason: opt(v.reason),
    // SKIN1 `skinReset`: skin changes stay off until then (null: for good); none: no block.
    ...(v.blocked ? { blocked: { until: v.blocked.until } } : {}),
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
    moderator: v.moderator,
    me: v.me,
    friend: v.friend,
    blocked: v.blocked,
    state: v.state,
    dnd: v.dnd,
    game: game(v.game),
    discordUrl: v.discordUrl,
    joinedAt: v.joinedAt,
    togetherMonth: v.togetherMonth,
    badge: badge(v.badge),
  };
}

function joinRequest(v: JoinRequestView): JoinRequest {
  // TEAM6: from a tag's card, the member whose tag it was ("Ayşe aracılığıyla").
  return { ...profile(v), sentAt: v.sentAt, via: opt(v.via), viaName: opt(v.viaName) };
}

function crewJoin(v: CrewJoinView): CrewJoin {
  return {
    crew: v.crew,
    id: v.id,
    name: v.name,
    mine: v.mine,
    at: v.at,
    greetUntil: opt(v.greetUntil),
  };
}

function teamRequest(v: TeamRequestView): TeamRequest {
  return {
    teamId: v.teamId,
    name: v.name,
    tag: v.tag,
    emblem: v.emblem,
    color: v.color,
    frame: v.frame,
    motto: v.motto,
    members: v.members,
    sentAt: v.sentAt,
  };
}

function plan(v: PlanView): CrewPlan {
  return {
    id: v.id,
    crew: v.crew,
    by: v.by,
    byName: opt(v.byName),
    mine: v.mine,
    canCancel: v.canCancel,
    at: v.at,
    startsAt: v.startsAt,
    endsAt: v.endsAt,
    createdAt: v.createdAt,
    answers: v.answers.map((a) => ({ id: a.id, answer: a.answer })),
    myAnswer: opt(v.myAnswer),
    note: opt(v.note),
    canWriteNote: v.canWriteNote,
    canClearNote: v.canClearNote,
  };
}

function teamLookup(v: TeamLookupView): TeamLookup {
  return {
    teamId: v.teamId,
    name: v.name,
    tag: v.tag,
    emblem: v.emblem,
    color: v.color,
    frame: v.frame,
    motto: v.motto,
    members: v.members,
    relation: v.relation,
    code: v.code,
    link: opt(v.link),
    join: v.join,
    requestedAt: opt(v.requestedAt),
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
      admit: v.permissions.admit ?? "moderators",
    },
    members: v.members.map(teamMember),
    invited: v.invited,
    limit: v.limit,
    mute: { muted: v.notifications.muted ?? false, muteUntil: v.notifications.muteUntil ?? null },
    textLockedUntil: opt(v.textLockedUntil),
    join: v.join,
    link: opt(v.link),
    linkUrl: opt(v.linkUrl),
    requests: v.requests.map(joinRequest),
    togetherWeek: v.togetherWeek,
    // TEAM6 (false and 0 while the service has no crew tags).
    hidden: v.hidden,
    badgeRequests: v.badgeRequests,
    wearers: v.wearers,
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
    requests: list.includes("requests"),
    link: list.includes("link"),
    plans: list.includes("plans"),
    planNotes: list.includes("planNotes"),
    crewFriendRequests: list.includes("crewFriendRequests"),
    badges: list.includes("badges"),
    // Requests from a tag only with the tags themselves.
    badgeRequests: list.includes("badges") && list.includes("badgeRequests"),
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
    teamRequests: v.teamRequests.map(teamRequest),
    plans: v.plans.map(plan),
    together: v.together,
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
    crewJoins: v.crewJoins.map(crewJoin),
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
    case "joinRequest":
      return { kind: "joinRequest", crew: v.crew, request: joinRequest(v.request) };
    case "requestAccepted":
      return { kind: "requestAccepted", crew: v.crew, name: v.name };
    case "crewPlan":
      return { kind: "crewPlan", plan: plan(v.plan) };
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
    case "afkNotice": {
      // Sent by the backend: every field is there (the optional ones are its reading side).
      const n = v.notice as AfkNotice_Serialize;
      return {
        kind: "afkNotice",
        notice: {
          id: n.id,
          event: n.event,
          kind: n.kind,
          priority: n.priority,
          at: n.at,
          tag: opt(n.tag),
          quiet: n.quiet,
          data: n.data,
          itemName: opt(n.itemName),
        },
      };
    }
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
  requestCrewmate: (userId) => ok(commands.socialRequestCrewmate(userId)),
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
    if (change.shareServer !== undefined || change.shareLastSeen !== undefined) {
      await ok(commands.socialSetPrivacy(change.shareServer ?? null, change.shareLastSeen ?? null));
    }
    // CHAT2's two switches ("Görüldü bilgisi", "Yazıyor bilgisi").
    if (change.shareRead !== undefined || change.shareTyping !== undefined) {
      await ok(commands.socialSetChatPrivacy(change.shareRead ?? null, change.shareTyping ?? null));
    }
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
              admit: permissions.admit ?? null,
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
  lookupTeam: async (code) => teamLookup(await ok(commands.socialTeamLookup(code))),
  joinTeam: (code) => ok(commands.socialTeamJoin(code)),
  async acceptTeamInvite(teamId) {
    await ok(commands.socialTeamAccept(teamId));
  },
  async declineTeamInvite(teamId) {
    await ok(commands.socialTeamDecline(teamId));
  },
  lookupCrewLink: async (link) => teamLookup(await ok(commands.socialTeamLookupLink(link))),
  joinCrewLink: (link) => ok(commands.socialTeamJoinLink(link)),
  async withdrawTeamRequest(teamId) {
    await ok(commands.socialTeamRequestWithdraw(teamId));
  },
  async answerTeamRequest(teamId, userId, accept) {
    await ok(commands.socialTeamRequestAnswer(teamId, userId, accept));
  },
  setTeamLink: (teamId, on) => ok(commands.socialTeamLink(teamId, on)),
  async setJoinMode(teamId, join) {
    await ok(commands.socialTeamJoinMode(teamId, join));
  },
  // TEAM6 (pl-social): sent only while `teamFeatures` lists them (the interface shows nothing of
  // them otherwise); "Geri al" is the same call with the old value.
  async setBadge(teamId) {
    await ok(commands.socialSetBadge(teamId));
  },
  async setBadgeSettings(teamId, change) {
    if (change.hidden !== undefined) await ok(commands.socialTeamHidden(teamId, change.hidden));
    if (change.badgeRequests !== undefined) {
      await ok(commands.socialTeamBadgeRequests(teamId, change.badgeRequests));
    }
  },
  async requestFromBadge(teamId, via) {
    const answer = await ok(commands.socialRequestFromBadge(teamId, via));
    return answer.status === "sent"
      ? { status: "sent" }
      : { status: "refused", reason: answer.reason, until: answer.until };
  },
  async greetNewcomer(crew, id) {
    await ok(commands.socialCrewGreet(crew, id));
  },
  onTagOff(listener) {
    const unlisten = listen<TagOffView>("social-tag-off", (event) =>
      listener({ crew: event.payload.crew, name: event.payload.name }),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
  makePlan: (crew, at, note) => ok(commands.socialPlanMake(crew, at, note ?? null)),
  async setPlanNote(planId, note) {
    await ok(commands.socialPlanNote(planId, note));
  },
  answerPlan: (planId, answer) => ok(commands.socialPlanAnswer(planId, answer)),
  async cancelPlan(planId) {
    await ok(commands.socialPlanCancel(planId));
  },
  async setTogether(on) {
    await commands.socialTogether(on);
  },
  async clearTogether() {
    await commands.socialTogetherClear();
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
  // CHAT2 (pl-social): a reply goes with its own command; the interface asks for the extras
  // only while `chat.extras` has them on (an older service has none).
  chatSend: (conv, text, reply) =>
    reply === undefined
      ? ok(commands.socialChatSend(conv, text))
      : ok(commands.socialChatReply(conv, text, reply)),
  chatSendSticker: (conv, sticker, reply) =>
    ok(commands.socialChatSticker(conv, sticker, reply ?? null)),
  async chatReact(conv, id, emoji, on) {
    await ok(commands.socialChatReact(conv, id, emoji, on));
  },
  chatTyping(conv) {
    void commands.socialChatTyping(conv).catch(() => {});
  },
  async chatUnfurl(conv, key) {
    await ok(commands.socialChatUnfurl(conv, key));
  },
  async chatCard(url) {
    return chatCard(await ok(commands.socialChatLinkCard(url)));
  },
  onChatTyping(listener) {
    const unlisten = listen<ChatTypingView>("social-chat-typing", (event) =>
      listener(event.payload),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
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
        ? // TEAM6: from a tag's card, the wearer as `via` (the backend sends one of code and via).
          {
            kind: "crew" as const,
            id: target.id,
            code: target.code ?? null,
            via: target.via ?? null,
          }
        : target.kind === "player"
          ? {
              kind: "player" as const,
              id: target.id,
              plan: target.plan ?? null,
              reaction: target.reaction ?? null,
            }
          : target;
    await ok(commands.socialReport(wire, reason, note ?? null, block));
  },
  async noticesSeen(ids) {
    await ok(commands.socialNoticesSeen(ids));
  },
};
